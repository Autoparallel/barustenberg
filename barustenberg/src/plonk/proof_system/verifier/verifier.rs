use crate::barretenberg::common::throw_or_abort;
use crate::barretenberg::ecc::curves::bn254::{fq12::Fq12, g1::AffineElement as G1AffineElement, pairing};
use crate::barretenberg::plonk::proof_system::constants::{
    standard_verifier_settings, turbo_verifier_settings, ultra_to_standard_verifier_settings, ultra_verifier_settings,
    ultra_with_keccak_verifier_settings, ProgramSettings,
};
use crate::barretenberg::plonk::proof_system::transcript::Manifest;
use crate::barretenberg::plonk::proof_system::verifier::{KateVerificationScheme, Verifier};
use crate::barretenberg::plonk::proof_system::PlonkProof;
use crate::barretenberg::plonk::public_inputs::PublicInputs;
use crate::barretenberg::polynomials::polynomial_arithmetic;
use crate::barretenberg::scalar_multiplication;

use ark_ff::{Field, PrimeField, Zero};
use std::collections::HashMap;
use std::sync::Arc;


pub struct VerifierBase<PS: ProgramSettings> {
    manifest: Manifest,
    key: Arc<PS::VerificationKey>,
    commitment_scheme: Option<KateVerificationScheme<PS>>,
}
impl Verifier for VerifierBase<standard_verifier_settings> {}
impl Verifier for VerifierBase<turbo_verifier_settings> {}
impl Verifier for VerifierBase<ultra_verifier_settings> {}
impl Verifier for VerifierBase<ultra_to_standard_verifier_settings> {}
impl Verifier for VerifierBase<ultra_with_keccak_verifier_settings> {}


impl<PS: ProgramSettings> VerifierBase<PS> {
    pub fn from_other(other: &Self) -> Self {
        Self {
            manifest: other.manifest.clone(),
            key: other.key.clone(),
            commitment_scheme: other.commitment_scheme.clone(),
        }
    }
}




pub fn verify_proof(self, proof: &PlonkProof) -> Result<bool, &'static str> {
    // This function verifies a PLONK proof for given program settings.
    // A PLONK proof for standard PLONK is of the form:
    //
    // π_SNARK =   { [a]_1,[b]_1,[c]_1,[z]_1,[t_{low}]_1,[t_{mid}]_1,[t_{high}]_1,[W_z]_1,[W_zω]_1 \in G,
    //                a_eval, b_eval, c_eval, sigma1_eval, sigma2_eval, sigma3_eval,
    //                  q_l_eval, q_r_eval, q_o_eval, q_m_eval, q_c_eval, z_eval_omega \in F }
    //
    // Proof π_SNARK must first be added to the transcript with the other program_settings.
    self.key.program_width = PS::PROGRAM_WIDTH;

    // Initialize the transcript.
    let mut transcript = transcript::StandardTranscript::new(
        proof.proof_data.clone(),
        self.manifest.clone(),
        PS::HASH_TYPE,
        PS::NUM_CHALLENGE_BYTES,
    );

    // Add circuit size and public input size to the transcript.
    transcript.add_element("circuit_size", key.circuit_size.to_be_bytes());
    transcript.add_element("public_input_size", key.num_public_inputs.to_be_bytes());

    // Compute challenges using Fiat-Shamir heuristic.
    transcript.apply_fiat_shamir("init");
    transcript.apply_fiat_shamir("eta");
    transcript.apply_fiat_shamir("beta");
    transcript.apply_fiat_shamir("alpha");
    transcript.apply_fiat_shamir("z");

    // Deserialize alpha and zeta from the transcript.
    let alpha = fr::deserialize_from_buffer(transcript.get_challenge("alpha"));
    let zeta = fr::deserialize_from_buffer(transcript.get_challenge("z"));

    // Compute the evaluations of the Lagrange polynomials and the vanishing polynomial.
    let lagrange_evals = barretenberg::polynomial_arithmetic::get_lagrange_evaluations(zeta, &key.domain);

    // Compute quotient polynomial evaluation at zeta.
    let mut t_numerator_eval = fr::default();
    PS::compute_quotient_evaluation_contribution(&key, alpha, &transcript, &mut t_numerator_eval);
    let t_eval = t_numerator_eval * lagrange_evals.vanishing_poly.inverse();
    transcript.add_element("t", t_eval.to_buffer());

    // Compute nu and separator challenges.
    transcript.apply_fiat_shamir("nu");
    transcript.apply_fiat_shamir("separator");
    let separator_challenge = fr::deserialize_from_buffer(transcript.get_challenge("separator"));

    // Verify the commitments using Kate commitment scheme.
    self.commitment_scheme.batch_verify(&transcript, &mut kate_g1_elements, &mut kate_fr_elements, &key)?;

    // Append scalar multiplication inputs.
    PS::append_scalar_multiplication_inputs(&key, alpha, &transcript, &mut kate_fr_elements);

    // Get PI_Z and PI_Z_OMEGA from the transcript.
    let pi_z = g1::AffineElement::deserialize_from_buffer(transcript.get_element("PI_Z"));
    let pi_z_omega = g1::AffineElement::deserialize_from_buffer(transcript.get_element("PI_Z_OMEGA"));

    // Check if PI_Z and PI_Z_OMEGA are valid points.
    if !pi_z.on_curve() || pi_z.is_point_at_infinity() {
        return Err("opening proof group element PI_Z not a valid point".into());
    }
    if !pi_z_omega.on_curve() || pi_z_omega.is_point_at_infinity() {
        return Err("opening proof group element PI_Z_OMEGA not a valid point".into());
    }

    // get kate_g1_elements: HashMap<u64, G1Affine> and kate_fr_elements: HashMap<u64, Fr>
    let mut kate_g1_elements: HashMap<String, G1AffineElement> = HashMap::new();
    let mut kate_fr_elements: HashMap<String, PS::Fr> = HashMap::new();

    // Initialize vectors for scalars and elements
    let mut scalars: Vec<Fr> = Vec::new();
    let mut elements: Vec<G1Affine> = Vec::new();

    // Iterate through the kate_g1_elements and accumulate scalars and elements
    for (key, element) in &kate_g1_elements {
        if element.is_on_curve() && !element.is_zero() {
            if let Some(scalar) = kate_fr_elements.get(key) {
                scalars.push(*scalar);
                elements.push(*element);
            }
        }
    }

    // Resize elements vector to make room for Pippenger point table
    let n = elements.len();
    elements.resize(2 * n, G1Affine::zero());

    // Generate Pippenger point table
    generate_pippenger_point_table(&mut elements[..]);

    // Create Pippenger runtime state
    let mut state = pippenger_runtime_state::new(n);

    // Perform Pippenger multi-scalar multiplication
    let p0 = pippenger(&scalars, &elements, &mut state);


    // Calculate P[1]
    let p1 = -((G1Projective::from(PI_Z_OMEGA) * separator_challenge) + G1Projective::from(PI_Z));

    // Check if recursive proof is present
    if let Some(recursive_proof_indices) = key.recursive_proof_public_input_indices {
        assert_eq!(recursive_proof_indices.len(), 16);

        let inputs = transcript.get_field_element_vector("public_inputs");

        //  Recover Fq values from public inputs
        let recover_fq_from_public_inputs = |idx0: usize, idx1: usize, idx2: usize, idx3: usize| {
            let l0 = inputs[idx0];
            let l1 = inputs[idx1];
            let l2 = inputs[idx2];
            let l3 = inputs[idx3];

            let limb = l0 + (l1 << NUM_LIMB_BITS_IN_FIELD_SIMULATION) +
                    (l2 << (NUM_LIMB_BITS_IN_FIELD_SIMULATION * 2)) +
                    (l3 << (NUM_LIMB_BITS_IN_FIELD_SIMULATION * 3));
            Fq::from(limb)
        };

        // Get recursion_separator_challenge
        let recursion_separator_challenge = transcript.get_challenge_field_element("separator").square();

        // Recover x0, y0, x1, and y1
        let x0 = recover_fq_from_public_inputs(recursive_proof_indices[0],
                                            recursive_proof_indices[1],
                                            recursive_proof_indices[2],
                                            recursive_proof_indices[3]);
        let y0 = recover_fq_from_public_inputs(recursive_proof_indices[4],
                                            recursive_proof_indices[5],
                                            recursive_proof_indices[6],
                                            recursive_proof_indices[7]);
        let x1 = recover_fq_from_public_inputs(recursive_proof_indices[8],
                                            recursive_proof_indices[9],
                                            recursive_proof_indices[10],
                                            recursive_proof_indices[11]);
        let y1 = recover_fq_from_public_inputs(recursive_proof_indices[12],
                                            recursive_proof_indices[13],
                                            recursive_proof_indices[14],
                                            recursive_proof_indices[15]);

        // Update P[0] and P[1] with recursive proof values
        let p0 = p0 + (G1Projective::new(x0, y0, Fq::one()) * recursion_separator_challenge);
        let p1 = p1 + (G1Projective::new(x1, y1, Fq::one()) * recursion_separator_challenge);
    }

    // Normalize P[0] and P[1]
    let p_affine = [G1Affine::from(p0), G1Affine::from(p1)];

    // Perform final pairing check
    let result = reduced_ate_pairing_batch_precomputed(&p_affine, &key.reference_string.get_precomputed_g2_lines());

    // Check if result equals Fq12::one()
    OK((result == Fq12::one()));
    Err("opening proof group element PI_Z not a valid point".into());

}
