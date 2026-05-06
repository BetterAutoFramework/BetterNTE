//! Condition handler trait and built-in stub handlers.
//!
//! Each handler evaluates one `Condition` variant. The evaluator dispatches
//! to the matching handler via `condition_type()`.

use async_trait::async_trait;

use crate::error::FlowResult;
use crate::types::Condition;
use crate::variables::VariableStore;

/// Trait for pluggable condition handlers.
///
/// Each handler is responsible for one `Condition` variant (identified by `condition_type()`).
#[async_trait]
pub trait ConditionHandler: Send + Sync {
    /// The condition variant this handler evaluates (e.g., "template", "ocr", "color").
    fn condition_type(&self) -> &str;

    /// Evaluate the condition, returning `true` or `false`.
    async fn evaluate(&self, condition: &Condition, variables: &VariableStore) -> FlowResult<bool>;
}

// ============================================================================
// Stub handlers (vision / capture / input integration pending; track on roadmap.)
// ============================================================================

pub struct TemplateConditionHandler;

#[async_trait]
impl ConditionHandler for TemplateConditionHandler {
    fn condition_type(&self) -> &str {
        "template"
    }

    async fn evaluate(
        &self,
        condition: &Condition,
        _variables: &VariableStore,
    ) -> FlowResult<bool> {
        if let Condition::Template {
            template,
            threshold,
            roi: _,
        } = condition
        {
            // Stub: template match via vision crate
            tracing::debug!("Evaluate template: {} (threshold: {})", template, threshold);
        }
        Ok(false)
    }
}

pub struct OcrConditionHandler;

#[async_trait]
impl ConditionHandler for OcrConditionHandler {
    fn condition_type(&self) -> &str {
        "ocr"
    }

    async fn evaluate(
        &self,
        condition: &Condition,
        _variables: &VariableStore,
    ) -> FlowResult<bool> {
        if let Condition::Ocr { expected, roi: _ } = condition {
            // Stub: OCR via vision crate
            tracing::debug!("Evaluate OCR: expected '{}'", expected);
        }
        Ok(false)
    }
}

pub struct ColorConditionHandler;

#[async_trait]
impl ConditionHandler for ColorConditionHandler {
    fn condition_type(&self) -> &str {
        "color"
    }

    async fn evaluate(
        &self,
        condition: &Condition,
        _variables: &VariableStore,
    ) -> FlowResult<bool> {
        if let Condition::Color {
            x,
            y,
            color,
            tolerance: _,
        } = condition
        {
            // Stub: color sample via capture + vision
            tracing::debug!("Evaluate color at ({}, {}): {}", x, y, color);
        }
        Ok(false)
    }
}

pub struct HotkeyConditionHandler;

#[async_trait]
impl ConditionHandler for HotkeyConditionHandler {
    fn condition_type(&self) -> &str {
        "hotkey"
    }

    async fn evaluate(
        &self,
        condition: &Condition,
        _variables: &VariableStore,
    ) -> FlowResult<bool> {
        if let Condition::Hotkey { key } = condition {
            // Stub: hotkey state from input layer
            tracing::debug!("Evaluate hotkey: {}", key);
        }
        Ok(false)
    }
}

pub struct ScriptConditionHandler;

#[async_trait]
impl ConditionHandler for ScriptConditionHandler {
    fn condition_type(&self) -> &str {
        "script"
    }

    async fn evaluate(
        &self,
        condition: &Condition,
        _variables: &VariableStore,
    ) -> FlowResult<bool> {
        if let Condition::Script { script } = condition {
            // Stub: invoke script runtime for predicate
            tracing::debug!("Evaluate script: {}", script);
        }
        Ok(false)
    }
}

/// Create the default set of condition handlers (all stubs for now).
pub fn default_condition_handlers() -> Vec<Box<dyn ConditionHandler>> {
    vec![
        Box::new(TemplateConditionHandler),
        Box::new(OcrConditionHandler),
        Box::new(ColorConditionHandler),
        Box::new(HotkeyConditionHandler),
        Box::new(ScriptConditionHandler),
    ]
}

/// Create the default set of condition handlers as reusable Arc trait objects.
pub fn default_condition_handler_arcs() -> Vec<std::sync::Arc<dyn ConditionHandler>> {
    vec![
        std::sync::Arc::new(TemplateConditionHandler),
        std::sync::Arc::new(OcrConditionHandler),
        std::sync::Arc::new(ColorConditionHandler),
        std::sync::Arc::new(HotkeyConditionHandler),
        std::sync::Arc::new(ScriptConditionHandler),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_variables() -> VariableStore {
        VariableStore::new("test".into(), HashMap::new(), None)
    }

    // ── default_condition_handlers ──

    #[test]
    fn test_default_handlers_count() {
        let handlers = default_condition_handlers();
        assert_eq!(handlers.len(), 5);
    }

    // ── condition_type strings ──

    #[test]
    fn test_template_handler_type() {
        assert_eq!(TemplateConditionHandler.condition_type(), "template");
    }

    #[test]
    fn test_ocr_handler_type() {
        assert_eq!(OcrConditionHandler.condition_type(), "ocr");
    }

    #[test]
    fn test_color_handler_type() {
        assert_eq!(ColorConditionHandler.condition_type(), "color");
    }

    #[test]
    fn test_hotkey_handler_type() {
        assert_eq!(HotkeyConditionHandler.condition_type(), "hotkey");
    }

    #[test]
    fn test_script_handler_type() {
        assert_eq!(ScriptConditionHandler.condition_type(), "script");
    }

    // ── evaluate returns Ok(false) for matching variants ──

    #[tokio::test]
    async fn test_template_handler_evaluate() {
        let vars = empty_variables();
        let cond = Condition::Template {
            template: "btn.png".into(),
            threshold: 0.8,
            roi: None,
        };
        let result = TemplateConditionHandler
            .evaluate(&cond, &vars)
            .await
            .unwrap();
        assert!(!result); // stub returns false
    }

    #[tokio::test]
    async fn test_ocr_handler_evaluate() {
        let vars = empty_variables();
        let cond = Condition::Ocr {
            expected: "hello".into(),
            roi: None,
        };
        let result = OcrConditionHandler.evaluate(&cond, &vars).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_color_handler_evaluate() {
        let vars = empty_variables();
        let cond = Condition::Color {
            x: 100,
            y: 200,
            color: "#FF0000".into(),
            tolerance: 10,
        };
        let result = ColorConditionHandler.evaluate(&cond, &vars).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_hotkey_handler_evaluate() {
        let vars = empty_variables();
        let cond = Condition::Hotkey { key: "F1".into() };
        let result = HotkeyConditionHandler.evaluate(&cond, &vars).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_script_handler_evaluate() {
        let vars = empty_variables();
        let cond = Condition::Script {
            script: "check.js".into(),
        };
        let result = ScriptConditionHandler.evaluate(&cond, &vars).await.unwrap();
        assert!(!result);
    }

    // ── evaluate with mismatched condition does not panic ──

    #[tokio::test]
    async fn test_template_handler_mismatched_condition() {
        let vars = empty_variables();
        let cond = Condition::Always;
        // Should not panic, just returns Ok(false)
        let result = TemplateConditionHandler
            .evaluate(&cond, &vars)
            .await
            .unwrap();
        assert!(!result);
    }
}
