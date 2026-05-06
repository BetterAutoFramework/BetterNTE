//! Homography estimation using DLT + RANSAC
//!
//! Provides Direct Linear Transform (DLT) algorithm and RANSAC for robust estimation.

use betternte_core::PointF;

/// 3x3 Homography matrix
#[derive(Debug, Clone)]
pub struct Homography {
    /// 3x3 transformation matrix (row-major)
    pub data: [[f64; 3]; 3],
}

impl Homography {
    /// Create identity homography
    pub fn identity() -> Self {
        Self {
            data: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    /// Create from 3x3 matrix
    pub fn from_matrix(data: [[f64; 3]; 3]) -> Self {
        Self { data }
    }

    /// Create from flat array (9 elements, row-major)
    pub fn from_flat(data: &[f64; 9]) -> Self {
        Self {
            data: [
                [data[0], data[1], data[2]],
                [data[3], data[4], data[5]],
                [data[6], data[7], data[8]],
            ],
        }
    }

    /// Get as flat array
    pub fn as_flat(&self) -> [f64; 9] {
        [
            self.data[0][0],
            self.data[0][1],
            self.data[0][2],
            self.data[1][0],
            self.data[1][1],
            self.data[1][2],
            self.data[2][0],
            self.data[2][1],
            self.data[2][2],
        ]
    }

    /// Direct Linear Transform (DLT) algorithm
    ///
    /// Computes homography H such that dst = H * src for each point pair.
    /// Requires at least 4 non-collinear point correspondences.
    ///
    /// For 4 points: uses direct solving with h33 = 1 constraint
    /// For more points: uses least squares via normal equations
    pub fn dlt(src: &[PointF], dst: &[PointF]) -> Result<Self, &'static str> {
        if src.len() != dst.len() {
            return Err("Source and destination must have same number of points");
        }
        if src.len() < 4 {
            return Err("At least 4 point correspondences required for DLT");
        }

        let n = src.len();
        let rows = 2 * n;

        // Build the 2n x 9 design matrix A
        // For each point correspondence (x, y) -> (x', y'):
        //   Row 2i:   [x, y, 1, 0, 0, 0, -x*x', -y*x', -x']
        //   Row 2i+1: [0, 0, 0, x, y, 1, -x*y', -y*y', -y']
        let mut a_data = vec![0.0; rows * 9];
        let mut b_data = vec![0.0; rows];

        for i in 0..n {
            let sx = src[i].x;
            let sy = src[i].y;
            let dx = dst[i].x;
            let dy = dst[i].y;

            // Row 2i: x*h11 + y*h12 + h13 - x*dx*h31 - y*dx*h32 = dx
            let row0 = (2 * i) * 9;
            a_data[row0] = sx;
            a_data[row0 + 1] = sy;
            a_data[row0 + 2] = 1.0;
            a_data[row0 + 6] = -sx * dx;
            a_data[row0 + 7] = -sy * dx;
            b_data[2 * i] = dx;

            // Row 2i+1: x*h21 + y*h22 + h23 - x*dy*h31 - y*dy*h32 = dy
            let row1 = (2 * i + 1) * 9;
            a_data[row1 + 3] = sx;
            a_data[row1 + 4] = sy;
            a_data[row1 + 5] = 1.0;
            a_data[row1 + 6] = -sx * dy;
            a_data[row1 + 7] = -sy * dy;
            b_data[2 * i + 1] = dy;
        }

        // For 4 points: use direct solve with h33 = 1 constraint
        if rows == 8 && n == 4 {
            return Self::solve_dlt_4point(&a_data, &b_data, src, dst);
        }

        // For more points: use least squares via normal equations
        // Solve (A^T A) h = A^T b
        let mut ata = vec![0.0; 81]; // 9x9
        let mut atb = vec![0.0; 9];

        for i in 0..rows {
            for j in 0..9 {
                atb[j] += a_data[i * 9 + j] * b_data[i];
                for k in 0..9 {
                    ata[j * 9 + k] += a_data[i * 9 + j] * a_data[i * 9 + k];
                }
            }
        }

        // Solve 9x9 system using Gaussian elimination
        let h = Self::solve_9x9(&ata, &atb)?;

        Ok(Self::from_flat(&h))
    }

    /// Solve DLT for exactly 4 points using direct method with h33 = 1
    fn solve_dlt_4point(
        _a_data: &[f64],
        _b_data: &[f64],
        src: &[PointF],
        dst: &[PointF],
    ) -> Result<Self, &'static str> {
        // For 4 points: 8 equations, 9 unknowns
        // Set h33 = 1 (scale constraint) and solve 8x8 system directly
        // Build 8x8 matrix M and 8-vector b
        let mut m_data = vec![0.0; 64]; // 8x8
        let mut b_data = vec![0.0; 8];

        for i in 0..4 {
            let sx = src[i].x;
            let sy = src[i].y;
            let dx = dst[i].x;
            let dy = dst[i].y;

            // Equation: x*h11 + y*h12 + h13 - x*dx*h31 - y*dx*h32 = dx
            let row = 2 * i;
            let base = row * 8;
            m_data[base] = sx; // h11
            m_data[base + 1] = sy; // h12
            m_data[base + 2] = 1.0; // h13
            m_data[base + 6] = -sx * dx; // h31
            m_data[base + 7] = -sy * dx; // h32
            b_data[row] = dx;

            // Equation: x*h21 + y*h22 + h23 - x*dy*h31 - y*dy*h32 = dy
            let row = 2 * i + 1;
            let base = row * 8;
            m_data[base + 3] = sx; // h21
            m_data[base + 4] = sy; // h22
            m_data[base + 5] = 1.0; // h23
            m_data[base + 6] = -sx * dy; // h31
            m_data[base + 7] = -sy * dy; // h32
            b_data[row] = dy;
        }

        // Solve 8x8 system using Gaussian elimination
        let x = Self::solve_8x8(&m_data, &b_data)?;

        Ok(Self::from_flat(&[
            x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], 1.0,
        ]))
    }

    /// Solve 8x8 linear system M * x = b using Gaussian elimination with partial pivoting
    fn solve_8x8(m: &[f64], b: &[f64]) -> Result<[f64; 8], &'static str> {
        const N: usize = 8;
        let mut aug = [[0.0; N + 1]; N];

        // Build augmented matrix [M | b]
        for i in 0..N {
            for j in 0..N {
                aug[i][j] = m[i * N + j];
            }
            aug[i][N] = b[i];
        }

        // Gaussian elimination with partial pivoting
        for k in 0..N {
            // Find pivot row
            let mut max_row = k;
            for i in k + 1..N {
                if aug[i][k].abs() > aug[max_row][k].abs() {
                    max_row = i;
                }
            }

            // Swap rows
            for j in 0..=N {
                let temp = aug[k][j];
                aug[k][j] = aug[max_row][j];
                aug[max_row][j] = temp;
            }

            // Check for singular matrix
            if aug[k][k].abs() < 1e-12 {
                return Err("Singular matrix in DLT solve");
            }

            // Eliminate
            for i in k + 1..N {
                let factor = aug[i][k] / aug[k][k];
                for j in k..=N {
                    aug[i][j] -= factor * aug[k][j];
                }
            }
        }

        // Back substitution
        let mut x = [0.0; N];
        for i in (0..N).rev() {
            x[i] = aug[i][N];
            for j in i + 1..N {
                x[i] -= aug[i][j] * x[j];
            }
            x[i] /= aug[i][i];
        }

        Ok(x)
    }

    /// Solve 9x9 linear system using Gaussian elimination with partial pivoting
    fn solve_9x9(m: &[f64], b: &[f64]) -> Result<[f64; 9], &'static str> {
        const N: usize = 9;
        let mut aug = [[0.0; N + 1]; N];

        // Build augmented matrix [M | b]
        for i in 0..N {
            for j in 0..N {
                aug[i][j] = m[i * N + j];
            }
            aug[i][N] = b[i];
        }

        // Gaussian elimination with partial pivoting
        for k in 0..N {
            // Find pivot row
            let mut max_row = k;
            for i in k + 1..N {
                if aug[i][k].abs() > aug[max_row][k].abs() {
                    max_row = i;
                }
            }

            // Swap rows
            for j in 0..=N {
                let temp = aug[k][j];
                aug[k][j] = aug[max_row][j];
                aug[max_row][j] = temp;
            }

            // Check for singular matrix
            if aug[k][k].abs() < 1e-12 {
                return Err("Singular matrix in DLT solve");
            }

            // Eliminate
            for i in k + 1..N {
                let factor = aug[i][k] / aug[k][k];
                for j in k..=N {
                    aug[i][j] -= factor * aug[k][j];
                }
            }
        }

        // Back substitution
        let mut x = [0.0; N];
        for i in (0..N).rev() {
            x[i] = aug[i][N];
            for j in i + 1..N {
                x[i] -= aug[i][j] * x[j];
            }
            x[i] /= aug[i][i];
        }

        Ok(x)
    }

    /// Apply homography to a point
    pub fn transform_point(&self, p: &PointF) -> PointF {
        let x = p.x;
        let y = p.y;
        let w = self.data[2][0] * x + self.data[2][1] * y + self.data[2][2];

        if w.abs() < 1e-10 {
            return *p;
        }

        let nx = (self.data[0][0] * x + self.data[0][1] * y + self.data[0][2]) / w;
        let ny = (self.data[1][0] * x + self.data[1][1] * y + self.data[1][2]) / w;

        PointF { x: nx, y: ny }
    }

    /// RANSAC-based robust homography estimation
    ///
    /// # Arguments
    /// * `src` - Source points
    /// * `dst` - Destination points
    /// * `threshold` - Inlier threshold (in pixels)
    /// * `iterations` - Number of RANSAC iterations
    ///
    /// # Returns
    /// * `Some((Homography, Vec<bool>))` - Best homography and inlier mask
    /// * `None` - If RANSAC fails
    pub fn ransac(
        src: &[PointF],
        dst: &[PointF],
        threshold: f64,
        iterations: usize,
    ) -> Option<(Self, Vec<bool>)> {
        let n = src.len();
        if n < 4 {
            return None;
        }

        let mut best_h = None;
        let mut best_inliers = Vec::new();
        let mut best_inlier_count = 0;

        for iter_idx in 0..iterations {
            // Deterministically pick 4 distinct points (LCG progression) so we
            // keep RANSAC behavior without depending on external RNG crates.
            let seed = best_inlier_count + best_inliers.len() + iterations + iter_idx;
            let mut cursor = seed.wrapping_mul(1103515245).wrapping_add(12345) % n.max(1);
            let mut indices: Vec<usize> = Vec::with_capacity(4);
            while indices.len() < 4 {
                if !indices.contains(&cursor) {
                    indices.push(cursor);
                }
                cursor = cursor.wrapping_mul(1664525).wrapping_add(1013904223) % n.max(1);
            }
            let src_sample: Vec<PointF> = indices.iter().map(|&i| src[i]).collect();
            let dst_sample: Vec<PointF> = indices.iter().map(|&i| dst[i]).collect();

            // Compute homography from sample
            let h = match Self::dlt(&src_sample, &dst_sample) {
                Ok(h) => h,
                Err(_) => continue,
            };

            // Count inliers
            let mut inliers = Vec::new();
            let mut inlier_count = 0;

            for i in 0..n {
                let projected = h.transform_point(&src[i]);
                let dx = projected.x - dst[i].x;
                let dy = projected.y - dst[i].y;
                let error = (dx * dx + dy * dy).sqrt();

                if error < threshold {
                    inliers.push(true);
                    inlier_count += 1;
                } else {
                    inliers.push(false);
                }
            }

            if inlier_count > best_inlier_count {
                best_inlier_count = inlier_count;
                best_h = Some(h);
                best_inliers = inliers;
            }
        }

        if let Some(h) = best_h {
            // Refine using all inliers
            let inlier_src: Vec<PointF> = src
                .iter()
                .zip(best_inliers.iter())
                .filter(|(_, &is_inlier)| is_inlier)
                .map(|(p, _)| *p)
                .collect();

            let inlier_dst: Vec<PointF> = dst
                .iter()
                .zip(best_inliers.iter())
                .filter(|(_, &is_inlier)| is_inlier)
                .map(|(p, _)| *p)
                .collect();

            if inlier_src.len() >= 4 {
                if let Ok(refined_h) = Self::dlt(&inlier_src, &inlier_dst) {
                    return Some((refined_h, best_inliers));
                }
            }

            Some((h, best_inliers))
        } else {
            None
        }
    }

    /// Compose two homographies (H1 * H2)
    pub fn compose(&self, other: &Self) -> Self {
        let mut result = [[0.0; 3]; 3];
        for (i, row) in result.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                for k in 0..3 {
                    *cell += self.data[i][k] * other.data[k][j];
                }
            }
        }
        Self { data: result }
    }

    /// Invert homography
    pub fn inverse(&self) -> Option<Self> {
        // Compute 3x3 matrix inverse
        let det = self.data[0][0]
            * (self.data[1][1] * self.data[2][2] - self.data[1][2] * self.data[2][1])
            - self.data[0][1]
                * (self.data[1][0] * self.data[2][2] - self.data[1][2] * self.data[2][0])
            + self.data[0][2]
                * (self.data[1][0] * self.data[2][1] - self.data[1][1] * self.data[2][0]);

        if det.abs() < 1e-10 {
            return None;
        }

        let inv_det = 1.0 / det;

        let result = [
            [
                (self.data[1][1] * self.data[2][2] - self.data[1][2] * self.data[2][1]) * inv_det,
                (self.data[0][2] * self.data[2][1] - self.data[0][1] * self.data[2][2]) * inv_det,
                (self.data[0][1] * self.data[1][2] - self.data[0][2] * self.data[1][1]) * inv_det,
            ],
            [
                (self.data[1][2] * self.data[2][0] - self.data[1][0] * self.data[2][2]) * inv_det,
                (self.data[0][0] * self.data[2][2] - self.data[0][2] * self.data[2][0]) * inv_det,
                (self.data[0][2] * self.data[1][0] - self.data[0][0] * self.data[1][2]) * inv_det,
            ],
            [
                (self.data[1][0] * self.data[2][1] - self.data[1][1] * self.data[2][0]) * inv_det,
                (self.data[0][1] * self.data[2][0] - self.data[0][0] * self.data[2][1]) * inv_det,
                (self.data[0][0] * self.data[1][1] - self.data[0][1] * self.data[1][0]) * inv_det,
            ],
        ];

        Some(Self { data: result })
    }
}

impl Default for Homography {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_homography() {
        let h = Homography::identity();
        let p = PointF { x: 10.0, y: 20.0 };
        let transformed = h.transform_point(&p);
        assert!((transformed.x - p.x).abs() < 1e-6);
        assert!((transformed.y - p.y).abs() < 1e-6);
    }

    #[test]
    fn test_dlt_translation() {
        // Translation by (dx, dy)
        let src = vec![
            PointF { x: 0.0, y: 0.0 },
            PointF { x: 1.0, y: 0.0 },
            PointF { x: 0.0, y: 1.0 },
            PointF { x: 1.0, y: 1.0 },
        ];
        let dst = vec![
            PointF { x: 10.0, y: 20.0 },
            PointF { x: 11.0, y: 20.0 },
            PointF { x: 10.0, y: 21.0 },
            PointF { x: 11.0, y: 21.0 },
        ];

        let h = Homography::dlt(&src, &dst).unwrap();
        let transformed = h.transform_point(&src[0]);
        assert!((transformed.x - 10.0).abs() < 0.1);
        assert!((transformed.y - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_homography_compose() {
        let h1 = Homography::identity();
        let h2 = Homography::identity();
        let composed = h1.compose(&h2);
        let p = PointF { x: 5.0, y: 10.0 };
        let transformed = composed.transform_point(&p);
        assert!((transformed.x - 5.0).abs() < 1e-6);
        assert!((transformed.y - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_inverse() {
        let h = Homography::identity();
        let h_inv = h.inverse().unwrap();
        let composed = h.compose(&h_inv);
        let p = PointF { x: 100.0, y: 200.0 };
        let transformed = composed.transform_point(&p);
        assert!((transformed.x - p.x).abs() < 1e-6);
        assert!((transformed.y - p.y).abs() < 1e-6);
    }

    #[test]
    fn test_ransac_basic() {
        // Simple translation case with non-collinear points
        let mut src = Vec::new();
        let mut dst = Vec::new();

        // Use points in a grid pattern to avoid collinearity
        for i in 0..4 {
            for j in 0..3 {
                let x = i as f64;
                let y = j as f64;
                src.push(PointF { x, y });
                dst.push(PointF {
                    x: x + 100.0,
                    y: y + 50.0,
                });
            }
        }

        // Add some outliers
        src.push(PointF { x: 100.0, y: 0.0 });
        dst.push(PointF { x: 0.0, y: 0.0 }); // outlier

        let result = Homography::ransac(&src, &dst, 1.0, 100);
        assert!(result.is_some());

        let (_h, inliers) = result.unwrap();
        // Most points should be inliers
        let inlier_count = inliers.iter().filter(|&&x| x).count();
        assert!(inlier_count >= 11); // 12 inliers out of 13
    }
}
