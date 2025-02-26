#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_debug_implementations, missing_docs)]
#![deny(unreachable_pub, private_in_public)]
//! This module provides the `Polynomial` struct, which represents a polynomial over a field `F`.
//!
//! The `Polynomial` struct contains methods for constructing polynomials, accessing their 
//! components, and performing operations on them. It also provides implementations for common 
//! arithmetic operations such as addition, subtraction, and scalar multiplication.
//! 
//! Here is a brief description of the main features of this module:
//!
//! # `Polynomial` Struct
//!
//! The `Polynomial` struct holds a vector of coefficients and the size of the polynomial. 
//! It is parameterized over a type `F` which is required to implement the `Field` trait.
//! The size of the polynomial corresponds to the number of terms in the polynomial (i.e., degree + 1).
//! The coefficients of the polynomial are represented by a vector of field elements. 
//! The `i`-th element of this vector represents the coefficient of the `x^i` term of the polynomial.
//!
//! # Constructors
//!
//! Two constructors are provided:
//!
//! - `from_interpolations`: Constructs a `Polynomial` instance from given interpolation points 
//! and function evaluations at these points.
//! - `new`: Creates a new `Polynomial` of a given size, with all coefficients initialized to zero.
//!
//! # Methods
//!
//! Methods provided by the `Polynomial` struct include:
//!
//! - `size`: Returns the size (number of terms) of the polynomial.
//! - `set_coefficient`: Sets the coefficient of the polynomial at a given index.
//! - `resize`: Resizes the polynomial to a new length, filling any additional space with a given value.
//!
//! # Trait Implementations
//!
//! The `Polynomial` struct also implements several traits to provide convenient and efficient 
//! operations on the polynomials:
//!
//! - `AddAssign`, `SubAssign`, `MulAssign`: For in-place addition, subtraction, and scalar multiplication.
//! - `IntoIterator`: To convert a `Polynomial` into an iterator over its coefficients.
//! - `Index` and `IndexMut`: To access or modify the coefficients of a `Polynomial` by their indices.
//!
//! This module serves as the core for polynomial operations in this library, providing the basis 
//! for efficient polynomial arithmetic over arbitrary fields.

use std::{
    ops::{AddAssign, Index, IndexMut, MulAssign, Range, SubAssign},
};

use ark_ff::Field;

use crate::polynomials::polynomial_arithmetic::compute_efficient_interpolation;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Polynomial<F: Field> {
    size: usize,
    pub(crate) coefficients: Vec<F>,
}

impl<F: Field> Polynomial<F> {
    /// Creates a polynomial by interpolation, given a list of interpolation points and 
    /// function evaluations at these points.
    pub(crate) fn from_interpolations(interpolation_points: &[F], evaluations: &[F]) -> anyhow::Result<Self> {
        assert!(!interpolation_points.is_empty());
        let mut coefficients = vec![F::zero(); interpolation_points.len()];
        compute_efficient_interpolation(
            evaluations,
            &mut coefficients,
            interpolation_points,
            interpolation_points.len(),
        )?;
        Ok(Self {
            size: interpolation_points.len(),
            coefficients,
        })
    }

    /// Creates a new polynomial of the given size with all coefficients initialized to zero.
    #[inline]
    pub(crate) fn new(size: usize) -> Self {
        let underlying = vec![F::zero(); size];
        Self {
            size,
            coefficients: underlying,
        }
    }

    /// Returns the number of terms of the polynomial (i.e., degree + 1).
    #[inline]
    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// Sets the coefficient at a given index of the polynomial.
    #[inline]
    pub(crate) fn set_coefficient(&mut self, idx: usize, v: F) {
        self.coefficients[idx] = v
    }

    /// Resizes the polynomial to a new length, filling any additional space with a given value.
    #[inline]
    pub(crate) fn resize(&mut self, new_len: usize, val: F) {
        self.coefficients.resize(new_len, val)
    }
}

impl<F: Field> AddAssign for Polynomial<F> {
    /// Adds another polynomial to `self` in-place.
    /// If the other polynomial has a higher degree, `self` is extended with zeros.
    fn add_assign(&mut self, rhs: Self) {
        // pad the smaller polynomial with zeros
        if self.size < rhs.size {
            self.resize(rhs.size, F::zero());
        }
        for i in 0..rhs.size {
            self.coefficients[i] += rhs.coefficients[i];
        }
    }
}

impl<F: Field> SubAssign for Polynomial<F> {
    /// Subtracts another polynomial from `self` in-place.
    /// If the other polynomial has a higher degree, `self` is extended with zeros.
    fn sub_assign(&mut self, rhs: Self) {
        // pad the smaller polynomial with zeros
        if self.size < rhs.size {
            self.resize(rhs.size, F::zero());
        }
        for i in 0..rhs.size {
            self.coefficients[i] -= rhs.coefficients[i];
        }
    }
}

impl<F: Field> MulAssign<F> for Polynomial<F> {
    /// Multiplies `self` by a scalar in-place.
    fn mul_assign(&mut self, rhs: F) {
        for i in 0..self.size {
            self.coefficients[i] *= rhs;
        }
    }
}

impl<F: Field> IntoIterator for Polynomial<F> {
    type Item = F;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    /// Returns an iterator over the coefficients of `self`.
    fn into_iter(self) -> Self::IntoIter {
        self.coefficients.into_iter()
    }
}

impl<F: Field> Index<usize> for Polynomial<F> {
    type Output = F;

    /// Returns a reference to the coefficient at a given index.
    fn index(&self, index: usize) -> &Self::Output {
        &self.coefficients[index]
    }
}

impl<F: Field> IndexMut<usize> for Polynomial<F> {
    /// Returns a mutable reference to the coefficient at a given index.
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.coefficients[index]
    }
}

impl<F: Field> Index<Range<usize>> for Polynomial<F> {
    type Output = [F];

    /// Returns a mutable slice of coefficients for a range of indices.
    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.coefficients[index]
    }
}

impl<F: Field> IndexMut<Range<usize>> for Polynomial<F> {
    fn index_mut(&mut self, index: Range<usize>) -> &mut Self::Output {
        &mut self.coefficients[index]
    }
}
impl<F: Field> Index<std::ops::RangeFrom<usize>> for Polynomial<F> {
    type Output = [F];

    /// Returns a slice of coefficients for a range starting from a given index.
    fn index(&self, index: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.coefficients[index]
    }
}
