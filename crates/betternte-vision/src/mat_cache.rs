//! Mat buffer cache for reusing OpenCV matrices across template matches.
//!
//! Instead of allocating new `Mat` objects for every template match operation,
//! the cache recycles matrices of matching size and type.  This reduces
//! allocator pressure in the hot vision pipeline.

use opencv::core::{Mat, MatTraitConst, CV_8UC4, CV_32FC1};
use std::collections::HashMap;

/// A cache of reusable OpenCV Mat buffers.
///
/// Matrices are keyed by (rows, cols, type) and stored in small buckets.
/// When a matrix is returned its data is not cleared (the next writer will
/// overwrite it completely).
pub struct MatCache {
    inner: std::sync::Mutex<CacheInner>,
}

struct CacheInner {
    /// Free matrices organised by (rows, cols, type) bucket.
    buckets: HashMap<MatKey, Vec<Mat>>,
    /// Maximum number of matrices kept per bucket.
    max_per_bucket: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MatKey {
    rows: i32,
    cols: i32,
    mat_type: i32,
}

impl MatKey {
    fn from_mat(mat: &Mat) -> Option<Self> {
        if mat.empty() {
            return None;
        }
        Some(Self {
            rows: mat.rows(),
            cols: mat.cols(),
            mat_type: mat.typ(),
        })
    }

    fn new(rows: i32, cols: i32, mat_type: i32) -> Self {
        Self { rows, cols, mat_type }
    }
}

impl MatCache {
    /// Create a new cache.
    ///
    /// `max_per_bucket` controls how many free matrices of each size/type are
    /// kept.  A value of 2–4 is usually enough.
    pub fn new(max_per_bucket: usize) -> Self {
        Self {
            inner: std::sync::Mutex::new(CacheInner {
                buckets: HashMap::new(),
                max_per_bucket: max_per_bucket.max(1),
            }),
        }
    }

    /// Obtain a matrix with the specified dimensions and type.
    ///
    /// If the cache has a matching matrix it is returned; otherwise a new
    /// matrix is created via `Mat::new_rows_cols()`.
    pub fn acquire(&self, rows: i32, cols: i32, mat_type: i32) -> opencv::Result<Mat> {
        let key = MatKey::new(rows, cols, mat_type);
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(bucket) = inner.buckets.get_mut(&key) {
            if let Some(mat) = bucket.pop() {
                return Ok(mat);
            }
        }

        // Nothing in cache – allocate fresh.
        // SAFETY: Mat::new_rows_cols creates a new matrix with the specified dimensions.
        // The caller is responsible for ensuring the dimensions are valid.
        unsafe { Mat::new_rows_cols(rows, cols, mat_type) }
    }

    /// Return a matrix to the cache for reuse.
    ///
    /// Callers **must** ensure the matrix is not used after this call.
    pub fn release(&self, mat: Mat) {
        if mat.empty() {
            return;
        }

        let key = match MatKey::from_mat(&mat) {
            Some(k) => k,
            None => return,
        };

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let max_per_bucket = inner.max_per_bucket;
        let bucket = inner.buckets.entry(key).or_insert_with(Vec::new);
        
        if bucket.len() < max_per_bucket {
            bucket.push(mat);
        }
        // else: bucket full, drop the matrix.
    }

    /// Clear all cached matrices.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.buckets.clear();
    }

    /// Number of free matrices currently cached.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.buckets.values().map(|v| v.len()).sum()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MatCache {
    fn default() -> Self {
        Self::new(3)
    }
}

/// RAII guard that returns the Mat to the cache on drop.
pub struct CachedMat<'a> {
    cache: &'a MatCache,
    mat: Option<Mat>,
}

impl<'a> CachedMat<'a> {
    /// Create a new guard.
    pub fn new(cache: &'a MatCache, rows: i32, cols: i32, mat_type: i32) -> opencv::Result<Self> {
        Ok(Self {
            cache,
            mat: Some(cache.acquire(rows, cols, mat_type)?),
        })
    }

    /// Consume the guard and return the inner Mat without returning it to
    /// the cache.  Useful when you need to store the Mat elsewhere.
    pub fn into_inner(mut self) -> Mat {
        self.mat.take().unwrap_or_default()
    }
}

impl<'a> std::ops::Deref for CachedMat<'a> {
    type Target = Mat;
    fn deref(&self) -> &Self::Target {
        self.mat.as_ref().unwrap()
    }
}

impl<'a> std::ops::DerefMut for CachedMat<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mat.as_mut().unwrap()
    }
}

impl<'a> Drop for CachedMat<'a> {
    fn drop(&mut self) {
        if let Some(mat) = self.mat.take() {
            self.cache.release(mat);
        }
    }
}

impl MatCache {
    /// Acquire a matrix wrapped in an RAII guard that returns it on drop.
    pub fn acquire_cached(&self, rows: i32, cols: i32, mat_type: i32) -> opencv::Result<CachedMat<'_>> {
        CachedMat::new(self, rows, cols, mat_type)
    }
}

/// Create a Mat from a byte slice (BGRA format).
///
/// This is a utility function that avoids the common pitfall of creating
/// a 1D Mat and then reshaping.
pub fn mat_from_bgra(data: &[u8], _width: i32, height: i32) -> opencv::Result<Mat> {
    // Create a 1D Mat from the raw bytes.
    let mat_1d = Mat::from_slice(data)?;
    // Reshape to 4 channels (BGRA), height rows.
    let mat_2d_ref = mat_1d.reshape(4, height)?;
    // Clone to get an owned Mat (reshape returns a reference).
    Ok(mat_2d_ref.try_clone()?)
}

/// Create a Mat from a byte slice (BGR format).
pub fn mat_from_bgr(data: &[u8], _width: i32, height: i32) -> opencv::Result<Mat> {
    let mat_1d = Mat::from_slice(data)?;
    let mat_2d_ref = mat_1d.reshape(3, height)?;
    Ok(mat_2d_ref.try_clone()?)
}

/// Create a Mat from a byte slice (single channel grayscale).
pub fn mat_from_gray(data: &[u8], _width: i32, height: i32) -> opencv::Result<Mat> {
    let mat_1d = Mat::from_slice(data)?;
    let mat_2d_ref = mat_1d.reshape(1, height)?;
    Ok(mat_2d_ref.try_clone()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_returns_mat_with_correct_size() {
        let cache = MatCache::new(3);
        let mat = cache.acquire(100, 200, CV_8UC4).unwrap();
        assert_eq!(mat.rows(), 100);
        assert_eq!(mat.cols(), 200);
        assert_eq!(mat.typ(), CV_8UC4);
    }

    #[test]
    fn test_release_and_reuse() {
        let cache = MatCache::new(3);
        let mat1 = cache.acquire(100, 200, CV_8UC4).unwrap();
        cache.release(mat1);
        assert_eq!(cache.len(), 1);

        let mat2 = cache.acquire(100, 200, CV_8UC4).unwrap();
        assert_eq!(mat2.rows(), 100);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_different_sizes_not_reused() {
        let cache = MatCache::new(3);
        let mat1 = cache.acquire(100, 200, CV_8UC4).unwrap();
        cache.release(mat1);

        let mat2 = cache.acquire(150, 300, CV_8UC4).unwrap();
        assert_eq!(mat2.rows(), 150);
        assert_eq!(cache.len(), 1); // The 100x200 matrix is still in cache.
    }

    #[test]
    fn test_cache_respects_max_per_bucket() {
        let cache = MatCache::new(2);
        let m1 = cache.acquire(100, 200, CV_8UC4).unwrap();
        let m2 = cache.acquire(100, 200, CV_8UC4).unwrap();
        let m3 = cache.acquire(100, 200, CV_8UC4).unwrap();

        cache.release(m1);
        cache.release(m2);
        cache.release(m3); // should be dropped (cache full)

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_guard_returns_on_drop() {
        let cache = MatCache::new(3);
        {
            let _guard = cache.acquire_cached(100, 200, CV_8UC4).unwrap();
            assert_eq!(cache.len(), 0);
        }
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_guard_into_inner_skips_return() {
        let cache = MatCache::new(3);
        let guard = cache.acquire_cached(100, 200, CV_8UC4).unwrap();
        let _mat = guard.into_inner();
        assert_eq!(cache.len(), 0);
    }
}
