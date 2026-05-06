//! Feature module: descriptor matching utilities and common feature types.
//!
//! Concrete feature detectors (e.g. SuperPoint) live under [`crate::models`].
//! This module hosts the descriptor matcher and shared "result" types used
//! across the vision pipeline.

pub mod matching;

pub use matching::{FeatureMatch, FeatureMatcher};

use betternte_core::{Color, Point};
use serde::{Deserialize, Serialize};

pub use crate::template::MatchResult;
pub use betternte_core::TextRegion;

/// A detected color hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPoint {
    pub position: Point,
    pub color: Color,
    pub distance: u8,
}

/// Unified output type for vision pipelines.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Feature {
    Text(TextRegion),
    Match(MatchResult),
    Color(ColorPoint),
    None,
}
