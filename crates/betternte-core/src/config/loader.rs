//! Load and persist engine configuration files (`engine.yaml`).

use crate::config::{EngineConfig, OcrTuningProfile};
use crate::error::{CoreError, Result};
use std::path::{Path, PathBuf};

fn validate_ocr_profile(
    profile: &OcrTuningProfile,
    name: &str,
) -> std::result::Result<(), CoreError> {
    if profile.max_side_len < 32 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_presets.{}.max_side_len must be >= 32: {}",
            name, profile.max_side_len
        )));
    }
    if !(0.0..=1.0).contains(&profile.det_threshold) {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_presets.{}.det_threshold out of range (0.0~1.0): {}",
            name, profile.det_threshold
        )));
    }
    if !(0.0..=1.0).contains(&profile.rec_threshold) {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_presets.{}.rec_threshold out of range (0.0~1.0): {}",
            name, profile.rec_threshold
        )));
    }
    if profile.batch_size == 0 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_presets.{}.batch_size must be >= 1",
            name
        )));
    }
    if profile.unclip_ratio <= 0.0 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_presets.{}.unclip_ratio must be > 0.0: {}",
            name, profile.unclip_ratio
        )));
    }
    Ok(())
}

/// Default configuration directory.
///
/// Windows: `%USERPROFILE%\.betternte\`
/// Linux/macOS: `~/.betternte/`
pub fn default_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".betternte")
}

/// Path to the default engine config file (`engine.yaml`).
pub fn default_engine_config_path() -> PathBuf {
    default_config_dir().join("engine.yaml")
}

/// Load engine configuration from `path`.
///
/// If the file is missing, returns [`EngineConfig::default`].
/// If the file exists but cannot be parsed, returns [`CoreError::ConfigParseError`].
pub fn load_engine_config<P: AsRef<Path>>(path: P) -> Result<EngineConfig> {
    let path = path.as_ref();

    if !path.exists() {
        tracing::warn!("config file missing, using defaults: {:?}", path);
        return Ok(EngineConfig::default());
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| CoreError::ConfigNotFound(format!("{}: {}", path.display(), e)))?;

    let config: EngineConfig = serde_yaml::from_str(&content)
        .map_err(|e| CoreError::ConfigParseError(format!("{}: {}", path.display(), e)))?;

    validate_engine_config(&config)?;

    Ok(config)
}

/// Save engine configuration to `path`.
///
/// Creates parent directories as needed. Writes YAML.
pub fn save_engine_config<P: AsRef<Path>>(config: &EngineConfig, path: P) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_yaml::to_string(config)
        .map_err(|e| CoreError::ConfigParseError(format!("serialize error: {}", e)))?;

    std::fs::write(path, content)?;
    tracing::info!("config saved: {:?}", path);

    Ok(())
}

/// Load engine configuration from the default [`default_engine_config_path`].
pub fn load_default_engine_config() -> Result<EngineConfig> {
    load_engine_config(default_engine_config_path())
}

/// Save engine configuration to the default [`default_engine_config_path`].
pub fn save_default_engine_config(config: &EngineConfig) -> Result<()> {
    save_engine_config(config, default_engine_config_path())
}

/// Validate `config` constraints (public for tests).
pub fn validate_engine_config(config: &EngineConfig) -> Result<()> {
    if config.capture.fps_cap > 240 {
        return Err(CoreError::ConfigValidationError(format!(
            "capture.fps_cap out of range (0~240): {}",
            config.capture.fps_cap
        )));
    }

    if config.overlay.opacity < 0.0 || config.overlay.opacity > 1.0 {
        return Err(CoreError::ConfigValidationError(format!(
            "overlay.opacity out of range (0.0~1.0): {}",
            config.overlay.opacity
        )));
    }

    if config.advanced.template_match_threshold < 0.0
        || config.advanced.template_match_threshold > 1.0
    {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.template_match_threshold out of range (0.0~1.0): {}",
            config.advanced.template_match_threshold
        )));
    }

    if config.advanced.ocr_det_threshold < 0.0 || config.advanced.ocr_det_threshold > 1.0 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_det_threshold out of range (0.0~1.0): {}",
            config.advanced.ocr_det_threshold
        )));
    }

    if config.advanced.ocr_rec_threshold < 0.0 || config.advanced.ocr_rec_threshold > 1.0 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_rec_threshold out of range (0.0~1.0): {}",
            config.advanced.ocr_rec_threshold
        )));
    }

    if config.advanced.ocr_unclip_ratio <= 0.0 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_unclip_ratio must be > 0.0: {}",
            config.advanced.ocr_unclip_ratio
        )));
    }

    if config.advanced.ocr_batch_size == 0 {
        return Err(CoreError::ConfigValidationError(
            "advanced.ocr_batch_size must be >= 1".to_string(),
        ));
    }

    if config.advanced.ocr_max_side_len < 32 {
        return Err(CoreError::ConfigValidationError(format!(
            "advanced.ocr_max_side_len must be >= 32: {}",
            config.advanced.ocr_max_side_len
        )));
    }

    validate_ocr_profile(&config.advanced.ocr_presets.performance, "performance")?;
    validate_ocr_profile(&config.advanced.ocr_presets.balanced, "balanced")?;
    validate_ocr_profile(&config.advanced.ocr_presets.accuracy, "accuracy")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── validate_engine_config ──

    #[test]
    fn test_validate_default_config() {
        let config = EngineConfig::default();
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_fps_cap_too_high() {
        let mut config = EngineConfig::default();
        config.capture.fps_cap = 241;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_fps_cap_max_boundary() {
        let mut config = EngineConfig::default();
        config.capture.fps_cap = 240;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_fps_cap_zero() {
        let mut config = EngineConfig::default();
        config.capture.fps_cap = 0;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_opacity_too_low() {
        let mut config = EngineConfig::default();
        config.overlay.opacity = -0.1;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_opacity_too_high() {
        let mut config = EngineConfig::default();
        config.overlay.opacity = 1.1;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_opacity_boundaries() {
        let mut config = EngineConfig::default();
        config.overlay.opacity = 0.0;
        assert!(validate_engine_config(&config).is_ok());
        config.overlay.opacity = 1.0;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_template_threshold_too_high() {
        let mut config = EngineConfig::default();
        config.advanced.template_match_threshold = 1.01;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_template_threshold_too_low() {
        let mut config = EngineConfig::default();
        config.advanced.template_match_threshold = -0.01;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_ocr_det_threshold_boundaries() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_det_threshold = 0.0;
        assert!(validate_engine_config(&config).is_ok());
        config.advanced.ocr_det_threshold = 1.0;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ocr_rec_threshold_boundaries() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_rec_threshold = 0.0;
        assert!(validate_engine_config(&config).is_ok());
        config.advanced.ocr_rec_threshold = 1.0;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ocr_unclip_ratio_positive() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_unclip_ratio = 0.1;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ocr_unclip_ratio_non_positive() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_unclip_ratio = 0.0;
        assert!(validate_engine_config(&config).is_err());
        config.advanced.ocr_unclip_ratio = -1.0;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_ocr_batch_size_positive() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_batch_size = 1;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ocr_batch_size_zero() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_batch_size = 0;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_ocr_max_side_len_boundary() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_max_side_len = 32;
        assert!(validate_engine_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ocr_max_side_len_too_small() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_max_side_len = 31;
        assert!(validate_engine_config(&config).is_err());
    }

    #[test]
    fn test_validate_ocr_preset_profile_invalid() {
        let mut config = EngineConfig::default();
        config.advanced.ocr_presets.performance.batch_size = 0;
        assert!(validate_engine_config(&config).is_err());
    }

    // ── load_engine_config ──

    #[test]
    fn test_load_nonexistent_file_returns_default() {
        let result = load_engine_config("/nonexistent/path/engine.yaml").unwrap();
        assert_eq!(
            result.capture.fps_cap,
            EngineConfig::default().capture.fps_cap
        );
    }

    #[test]
    fn test_load_valid_yaml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("engine.yaml");
        let config = EngineConfig::default();
        save_engine_config(&config, &path).unwrap();
        let loaded = load_engine_config(&path).unwrap();
        assert_eq!(loaded.capture.fps_cap, config.capture.fps_cap);
    }

    #[test]
    fn test_load_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("engine.yaml");
        std::fs::write(&path, "not: valid: yaml: [").unwrap();
        let result = load_engine_config(&path);
        assert!(result.is_err());
    }

    // ── save_engine_config + round-trip ──

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("engine.yaml");
        let mut config = EngineConfig::default();
        config.capture.fps_cap = 120;
        config.overlay.opacity = 0.5;
        save_engine_config(&config, &path).unwrap();
        let loaded = load_engine_config(&path).unwrap();
        assert_eq!(loaded.capture.fps_cap, 120);
        assert!((loaded.overlay.opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("dir").join("engine.yaml");
        let config = EngineConfig::default();
        assert!(save_engine_config(&config, &path).is_ok());
        assert!(path.exists());
    }

    // ── default paths ──

    #[test]
    fn test_default_config_dir_ends_with_betternte() {
        let dir = default_config_dir();
        assert!(dir.ends_with(".betternte"));
    }

    #[test]
    fn test_default_engine_config_path_ends_with_yaml() {
        let path = default_engine_config_path();
        assert!(path.ends_with("engine.yaml"));
    }
}
