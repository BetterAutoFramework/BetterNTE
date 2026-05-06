//! Morphological operations module
//!
//! Provides dilation, erosion, opening, closing, and gradient operations.

pub mod ops;

pub use ops::{MorphKernel, Morphology};
