//! Contour analysis - properties and measurements

use crate::contour::finder::Contour;
#[cfg(test)]
use betternte_core::Point;
use betternte_core::{PointF, Region};

/// Contour properties
#[derive(Debug, Clone)]
pub struct ContourProperties {
    /// Area (using shoelace formula)
    pub area: f64,
    /// Perimeter (arc length)
    pub perimeter: f64,
    /// Bounding box
    pub bounding_box: Region,
    /// Centroid (x, y)
    pub centroid: PointF,
    /// Compactness = 4 * pi * area / perimeter^2
    pub compactness: f64,
    /// Aspect ratio of bounding box (width / height)
    pub aspect_ratio: f64,
    /// Extent = area / bounding_box_area
    pub extent: f64,
    /// Solidity = area / convex_hull_area
    pub solidity: f64,
}

/// Contour analyzer
pub struct ContourAnalyzer;

impl ContourAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a single contour
    pub fn analyze(&self, contour: &Contour) -> ContourProperties {
        let area = self.area(contour);
        let perimeter = self.perimeter(contour);
        let (bounding_box, bbox_area) = self.bounding_box(contour);
        let centroid = self.centroid(contour);
        let compactness = self.compactness(area, perimeter);
        let aspect_ratio = bounding_box.width as f64 / bounding_box.height.max(1) as f64;
        let extent = if bbox_area > 0.0 {
            area / bbox_area
        } else {
            0.0
        };

        // Solidity approximation (simplified - would need convex hull for accurate value)
        let solidity = 1.0; // Placeholder

        ContourProperties {
            area,
            perimeter,
            bounding_box,
            centroid,
            compactness,
            aspect_ratio,
            extent,
            solidity,
        }
    }

    /// Compute contour area using Green's theorem (shoelace formula)
    pub fn area(&self, contour: &Contour) -> f64 {
        let points = &contour.points;
        if points.len() < 3 {
            return 0.0;
        }

        let mut area = 0.0;
        let n = points.len();

        for i in 0..n {
            let j = (i + 1) % n;
            area += points[i].x as f64 * points[j].y as f64;
            area -= points[j].x as f64 * points[i].y as f64;
        }

        area.abs() / 2.0
    }

    /// Compute contour perimeter (arc length)
    pub fn perimeter(&self, contour: &Contour) -> f64 {
        let points = &contour.points;
        if points.len() < 2 {
            return 0.0;
        }

        let mut length = 0.0;
        let n = points.len();

        for i in 0..n {
            let j = (i + 1) % n;
            let dx = points[j].x as f64 - points[i].x as f64;
            let dy = points[j].y as f64 - points[i].y as f64;
            length += (dx * dx + dy * dy).sqrt();
        }

        length
    }

    /// Compute bounding box
    pub fn bounding_box(&self, contour: &Contour) -> (Region, f64) {
        let points = &contour.points;
        if points.is_empty() {
            return (
                Region {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                },
                0.0,
            );
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for p in points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }

        let width = (max_x - min_x) as u32;
        let height = (max_y - min_y) as u32;
        let region = Region {
            x: min_x,
            y: min_y,
            width,
            height,
        };

        (region, (width * height) as f64)
    }

    /// Compute centroid
    pub fn centroid(&self, contour: &Contour) -> PointF {
        let points = &contour.points;
        if points.is_empty() {
            return PointF { x: 0.0, y: 0.0 };
        }

        let n = points.len() as f64;
        let sum_x: f64 = points.iter().map(|p| p.x as f64).sum();
        let sum_y: f64 = points.iter().map(|p| p.y as f64).sum();

        PointF {
            x: sum_x / n,
            y: sum_y / n,
        }
    }

    /// Compute compactness = 4 * pi * area / perimeter^2
    /// Circular shapes have compactness close to 1.0
    pub fn compactness(&self, area: f64, perimeter: f64) -> f64 {
        if perimeter <= 0.0 {
            return 0.0;
        }
        let p2 = perimeter * perimeter;
        (4.0 * std::f64::consts::PI * area / p2).min(1.0)
    }

    /// Compute minimum enclosing circle (not yet implemented)
    pub fn min_enclosing_circle(&self, _contour: &Contour) -> Option<(PointF, f64)> {
        // TODO: Implement Welzl's algorithm or similar
        None
    }

    /// Fit an ellipse to the contour using second-order moments (PCA fit).
    ///
    /// Semi-axes are derived from the covariance eigenvalues as
    /// `r_i = 2 * sqrt(lambda_i)` (matches a 1-σ Gaussian fit / OpenCV's
    /// `fitEllipse` shape, modulo the rotation handling).
    pub fn fit_ellipse(&self, contour: &Contour) -> Option<Ellipse> {
        let points = &contour.points;
        if points.len() < 5 {
            return None;
        }

        let centroid = self.centroid(contour);

        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_yy = 0.0;

        for p in points {
            let dx = p.x as f64 - centroid.x;
            let dy = p.y as f64 - centroid.y;
            sum_xx += dx * dx;
            sum_xy += dx * dy;
            sum_yy += dy * dy;
        }

        let n = points.len() as f64;
        sum_xx /= n;
        sum_xy /= n;
        sum_yy /= n;

        let trace = sum_xx + sum_yy;
        let det = sum_xx * sum_yy - sum_xy * sum_xy;
        let discriminant = (trace * trace / 4.0 - det).max(0.0).sqrt();

        let lambda1 = (trace / 2.0 + discriminant).max(0.0);
        let lambda2 = (trace / 2.0 - discriminant).max(0.0);

        let semi_major = 2.0 * lambda1.max(lambda2).sqrt();
        let semi_minor = 2.0 * lambda1.min(lambda2).sqrt();

        let angle = if sum_xy.abs() > 1e-10 {
            (sum_xy / (lambda1 - sum_yy)).atan()
        } else {
            0.0
        };

        Some(Ellipse {
            center: centroid,
            semi_major,
            semi_minor,
            angle,
        })
    }
}

impl Default for ContourAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Ellipse fit to a contour
#[derive(Debug, Clone)]
pub struct Ellipse {
    pub center: PointF,
    pub semi_major: f64,
    pub semi_minor: f64,
    pub angle: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rectangle_contour(x: i32, y: i32, w: i32, h: i32) -> Contour {
        let points = vec![
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
        ];
        Contour { points }
    }

    #[test]
    fn test_area_square() {
        let contour = make_rectangle_contour(0, 0, 100, 100);
        let analyzer = ContourAnalyzer::new();
        let area = analyzer.area(&contour);
        assert!((area - 10000.0).abs() < 0.1);
    }

    #[test]
    fn test_area_triangle() {
        let points = vec![Point::new(0, 0), Point::new(100, 0), Point::new(50, 50)];
        let contour = Contour { points };
        let analyzer = ContourAnalyzer::new();
        let area = analyzer.area(&contour);
        assert!((area - 2500.0).abs() < 0.1);
    }

    #[test]
    fn test_perimeter_square() {
        let contour = make_rectangle_contour(0, 0, 100, 100);
        let analyzer = ContourAnalyzer::new();
        let perimeter = analyzer.perimeter(&contour);
        assert!((perimeter - 400.0).abs() < 0.1);
    }

    #[test]
    fn test_compactness_circle_vs_square() {
        let analyzer = ContourAnalyzer::new();

        // Circle has compactness ~1
        let circle_perimeter = 2.0 * std::f64::consts::PI * 10.0;
        let circle_area = std::f64::consts::PI * 10.0 * 10.0;
        let circle_compactness = analyzer.compactness(circle_area, circle_perimeter);
        assert!(circle_compactness > 0.9);

        // Square has compactness ~0.785
        let square = make_rectangle_contour(0, 0, 10, 10);
        let square_area = analyzer.area(&square);
        let square_perimeter = analyzer.perimeter(&square);
        let square_compactness = analyzer.compactness(square_area, square_perimeter);
        assert!(square_compactness < 0.8);
    }

    #[test]
    fn test_fit_ellipse() {
        // Create contour with enough points (>5) for ellipse fitting
        // Sample points along rectangle perimeter
        let mut points = Vec::new();
        let (x, y, w, h) = (0, 0, 100, 50);
        // Top edge (left to right)
        for i in 0..=10 {
            points.push(Point::new(x + i * w / 10, y));
        }
        // Right edge (top to bottom)
        for i in 1..=10 {
            points.push(Point::new(x + w, y + i * h / 10));
        }
        // Bottom edge (right to left)
        for i in (1..10).rev() {
            points.push(Point::new(x + i * w / 10, y + h));
        }
        // Left edge (bottom to top)
        for i in (1..10).rev() {
            points.push(Point::new(x, y + i * h / 10));
        }
        let contour = Contour { points };

        let analyzer = ContourAnalyzer::new();
        let ellipse = analyzer.fit_ellipse(&contour);

        assert!(ellipse.is_some());
        let ellipse = ellipse.unwrap();

        // Ellipse should be roughly centered
        assert!(ellipse.center.x > 40.0 && ellipse.center.x < 60.0);
        assert!(ellipse.center.y > 15.0 && ellipse.center.y < 35.0);

        // Major axis should be roughly 100, minor roughly 50
        assert!(ellipse.semi_major > 40.0);
        assert!(ellipse.semi_minor > 15.0);
    }

    #[test]
    fn test_analyze() {
        let contour = make_rectangle_contour(10, 20, 100, 80);
        let analyzer = ContourAnalyzer::new();
        let props = analyzer.analyze(&contour);

        assert!(props.area > 7000.0 && props.area < 9000.0);
        assert!(props.perimeter > 350.0 && props.perimeter < 400.0);
        assert_eq!(props.bounding_box.x, 10);
        assert_eq!(props.bounding_box.y, 20);
        assert_eq!(props.bounding_box.width, 100);
        assert_eq!(props.bounding_box.height, 80);
    }
}
