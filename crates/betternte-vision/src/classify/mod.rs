//! Classification module
//!
//! Provides color range detection, histogram computation, and back projection.

pub mod color_range;
pub mod histogram;

pub use color_range::ColorRangeDetector;
pub use histogram::Histogram;
