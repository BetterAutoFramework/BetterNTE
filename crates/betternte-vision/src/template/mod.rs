//! Template matching module.
//!
//! The active matcher is [`OpenCvTemplateMatcher`], which delegates to
//! OpenCV `matchTemplate` with `TM_CCOEFF_NORMED`.

pub mod cache;
pub mod opencv_matcher;

pub use cache::TemplateCache;
pub use opencv_matcher::OpenCvTemplateMatcher;

pub use betternte_core::{MatchResult, TemplateMatcher};
