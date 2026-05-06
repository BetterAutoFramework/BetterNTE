//! Template cache with LRU eviction policy
//!
//! Thread-safe template storage using RwLock + LruCache

use crate::error::VisionError;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::RwLock;
use std::time::SystemTime;

#[derive(Clone)]
struct CacheEntry {
    image: image::DynamicImage,
    modified: Option<SystemTime>,
}

/// Thread-safe template cache with LRU eviction
pub struct TemplateCache {
    max_size: usize,
    cache: RwLock<LruCache<String, CacheEntry>>,
}

impl TemplateCache {
    /// Create a new cache with specified maximum size
    pub fn new(max_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(max_size.max(1)).unwrap_or(NonZeroUsize::MIN);
        Self {
            max_size,
            cache: RwLock::new(LruCache::new(cache_size)),
        }
    }

    /// Load template from file path
    pub fn load(&self, path: &Path) -> Result<image::DynamicImage, VisionError> {
        let key = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();
        let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());

        // Check cache first and validate mtime to avoid stale hits.
        {
            let mut cache = self.cache.write().unwrap();
            if let Some(entry) = cache.get(&key) {
                if entry.modified == modified {
                    tracing::trace!(
                        target: "betternte_perf",
                        event = "template_cache_hit",
                        key = %key,
                        "template_cache_hit"
                    );
                    return Ok(entry.image.clone());
                }
            }
        }

        // Load from file
        if !path.exists() {
            return Err(VisionError::TemplateNotFound(path.display().to_string()));
        }

        let img = image::open(path).map_err(|e| {
            VisionError::ImageProcessingError(format!("Failed to load template: {}", e))
        })?;

        // Store in cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.push(
                key,
                CacheEntry {
                    image: img.clone(),
                    modified,
                },
            );
        }
        tracing::trace!(
            target: "betternte_perf",
            event = "template_cache_miss",
            path = %path.display(),
            "template_cache_miss"
        );

        Ok(img)
    }

    /// Get cached template by key
    pub fn get(&self, key: &str) -> Option<image::DynamicImage> {
        let mut cache = self.cache.write().unwrap();
        cache.get(key).map(|entry| entry.image.clone())
    }

    /// Insert template into cache
    pub fn insert(&self, key: String, template: image::DynamicImage) {
        let mut cache = self.cache.write().unwrap();
        cache.push(
            key,
            CacheEntry {
                image: template,
                modified: None,
            },
        );
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Current number of cached templates
    pub fn len(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        let cache = self.cache.read().unwrap();
        cache.is_empty()
    }

    /// Get maximum cache size
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

impl Default for TemplateCache {
    fn default() -> Self {
        Self::new(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_cache_basic() {
        let cache = TemplateCache::new(2);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_insert_get() {
        let cache = TemplateCache::new(2);
        let img = image::DynamicImage::new_rgb8(10, 10);

        cache.insert("test".to_string(), img.clone());
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("test");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = TemplateCache::new(2);
        let img1 = image::DynamicImage::new_rgb8(10, 10);
        let img2 = image::DynamicImage::new_rgb8(20, 20);
        let img3 = image::DynamicImage::new_rgb8(30, 30);

        cache.insert("img1".to_string(), img1);
        cache.insert("img2".to_string(), img2);
        assert_eq!(cache.len(), 2);

        // Access img1 to make it recently used
        let _ = cache.get("img1");

        // Add third item, img2 should be evicted (LRU)
        cache.insert("img3".to_string(), img3);

        assert_eq!(cache.len(), 2);
        assert!(cache.get("img2").is_none());
        assert!(cache.get("img1").is_some());
        assert!(cache.get("img3").is_some());
    }

    #[test]
    fn test_cache_clear() {
        let cache = TemplateCache::new(2);
        let img = image::DynamicImage::new_rgb8(10, 10);

        cache.insert("test".to_string(), img);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_load_from_file() {
        // Create temp directory and file
        let temp_dir = std::env::temp_dir().join("betternte_cache_test");
        let _ = fs::create_dir_all(&temp_dir);
        let template_path = temp_dir.join("test_template.png");

        // Create a simple test image
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10));
        img.save(&template_path).unwrap();

        let cache = TemplateCache::new(2);
        let loaded = cache.load(&template_path);

        assert!(loaded.is_ok());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.width(), 10);
        assert_eq!(loaded.height(), 10);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_reload_when_file_changes() {
        let temp_dir = std::env::temp_dir().join("betternte_cache_test_reload");
        let _ = fs::create_dir_all(&temp_dir);
        let template_path = temp_dir.join("reload_template.png");

        let img1 = image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 8));
        img1.save(&template_path).unwrap();

        let cache = TemplateCache::new(4);
        let first = cache.load(&template_path).unwrap();
        assert_eq!(first.width(), 8);

        std::thread::sleep(std::time::Duration::from_millis(10));
        let img2 = image::DynamicImage::ImageRgb8(image::RgbImage::new(12, 12));
        img2.save(&template_path).unwrap();

        let second = cache.load(&template_path).unwrap();
        assert_eq!(second.width(), 12);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_load_missing_file() {
        let cache = TemplateCache::new(2);
        let result = cache.load(Path::new("nonexistent.png"));
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_converts_key_from_path() {
        let cache = TemplateCache::new(2);
        let img = image::DynamicImage::new_rgb8(10, 10);

        // Insert with a path-like key
        cache.insert("subdir/template.png".to_string(), img.clone());

        let retrieved = cache.get("subdir/template.png");
        assert!(retrieved.is_some());
    }
}
