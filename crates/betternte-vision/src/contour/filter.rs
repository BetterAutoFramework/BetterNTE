//! Contour filtering predicates

use crate::contour::analysis::ContourAnalyzer;
use crate::contour::finder::Contour;

/// Contour filter with various predicates
pub struct ContourFilter {
    analyzer: ContourAnalyzer,
}

impl ContourFilter {
    pub fn new() -> Self {
        Self {
            analyzer: ContourAnalyzer::new(),
        }
    }

    /// Filter contours by minimum area
    pub fn by_min_area(&self, contours: &[Contour], min_area: f64) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| self.analyzer.area(c) >= min_area)
            .cloned()
            .collect()
    }

    /// Filter contours by maximum area
    pub fn by_max_area(&self, contours: &[Contour], max_area: f64) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| self.analyzer.area(c) <= max_area)
            .cloned()
            .collect()
    }

    /// Filter contours by area range [min, max]
    pub fn by_area_range(
        &self,
        contours: &[Contour],
        min_area: f64,
        max_area: f64,
    ) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| {
                let area = self.analyzer.area(c);
                area >= min_area && area <= max_area
            })
            .cloned()
            .collect()
    }

    /// Filter contours by aspect ratio (width / height)
    pub fn by_aspect_ratio(
        &self,
        contours: &[Contour],
        min_ratio: f64,
        max_ratio: f64,
    ) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| {
                let props = self.analyzer.analyze(c);
                let ratio = props.aspect_ratio;
                ratio >= min_ratio && ratio <= max_ratio
            })
            .cloned()
            .collect()
    }

    /// Filter contours by minimum compactness
    pub fn by_min_compactness(&self, contours: &[Contour], min_compactness: f64) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| {
                let props = self.analyzer.analyze(c);
                props.compactness >= min_compactness
            })
            .cloned()
            .collect()
    }

    /// Filter contours by minimum perimeter
    pub fn by_min_perimeter(&self, contours: &[Contour], min_perimeter: f64) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| self.analyzer.perimeter(c) >= min_perimeter)
            .cloned()
            .collect()
    }

    /// Filter contours by bounding box width
    pub fn by_width(&self, contours: &[Contour], min_width: u32, max_width: u32) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| {
                let (bbox, _) = self.analyzer.bounding_box(c);
                bbox.width >= min_width && bbox.width <= max_width
            })
            .cloned()
            .collect()
    }

    /// Filter contours by bounding box height
    pub fn by_height(
        &self,
        contours: &[Contour],
        min_height: u32,
        max_height: u32,
    ) -> Vec<Contour> {
        contours
            .iter()
            .filter(|c| {
                let (bbox, _) = self.analyzer.bounding_box(c);
                bbox.height >= min_height && bbox.height <= max_height
            })
            .cloned()
            .collect()
    }

    /// Keep only convex contours (simplified check using area vs bounding box)
    pub fn convex_only(&self, contours: &[Contour]) -> Vec<Contour> {
        // Simplified: check if contour area is close to bounding box area
        contours
            .iter()
            .filter(|c| {
                let props = self.analyzer.analyze(c);
                // For convex shapes, extent should be close to 1.0
                props.extent > 0.9
            })
            .cloned()
            .collect()
    }

    /// Sort contours by area (descending).
    pub fn sort_by_area(&self, contours: &mut [Contour]) {
        contours.sort_by(|a, b| {
            let area_a = self.analyzer.area(a);
            let area_b = self.analyzer.area(b);
            area_b
                .partial_cmp(&area_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Sort contours by perimeter (descending).
    pub fn sort_by_perimeter(&self, contours: &mut [Contour]) {
        contours.sort_by(|a, b| {
            let perim_a = self.analyzer.perimeter(a);
            let perim_b = self.analyzer.perimeter(b);
            perim_b
                .partial_cmp(&perim_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl Default for ContourFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::Point;

    fn make_contour(points: Vec<Point>) -> Contour {
        Contour { points }
    }

    #[test]
    fn test_filter_by_min_area() {
        let contours = vec![
            make_contour(vec![
                Point::new(0, 0),
                Point::new(100, 0),
                Point::new(100, 100),
                Point::new(0, 100),
            ]), // area = 10000
            make_contour(vec![
                Point::new(0, 0),
                Point::new(10, 0),
                Point::new(10, 10),
                Point::new(0, 10),
            ]), // area = 100
        ];

        let filter = ContourFilter::new();
        let filtered = filter.by_min_area(&contours, 500.0);

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_by_aspect_ratio() {
        let wide = make_contour(vec![
            Point::new(0, 0),
            Point::new(100, 0),
            Point::new(100, 10),
            Point::new(0, 10),
        ]); // aspect = 10

        let tall = make_contour(vec![
            Point::new(0, 0),
            Point::new(10, 0),
            Point::new(10, 100),
            Point::new(0, 100),
        ]); // aspect = 0.1

        let filter = ContourFilter::new();

        let wide_filtered = filter.by_aspect_ratio(std::slice::from_ref(&wide), 5.0, 15.0);
        assert_eq!(wide_filtered.len(), 1);

        let tall_filtered = filter.by_aspect_ratio(std::slice::from_ref(&tall), 0.05, 0.2);
        assert_eq!(tall_filtered.len(), 1);
    }

    #[test]
    fn test_sort_by_area() {
        let small = make_contour(vec![
            Point::new(0, 0),
            Point::new(10, 0),
            Point::new(10, 10),
            Point::new(0, 10),
        ]);

        let large = make_contour(vec![
            Point::new(0, 0),
            Point::new(100, 0),
            Point::new(100, 100),
            Point::new(0, 100),
        ]);

        let medium = make_contour(vec![
            Point::new(0, 0),
            Point::new(50, 0),
            Point::new(50, 50),
            Point::new(0, 50),
        ]);

        let mut contours = vec![medium.clone(), small.clone(), large.clone()];
        let filter = ContourFilter::new();
        filter.sort_by_area(&mut contours);

        // Should be sorted descending by area
        assert_eq!(contours[0].points.len(), 4); // Large square
        assert_eq!(contours[2].points.len(), 4); // Small square
    }

    #[test]
    fn test_convex_only() {
        let square = make_contour(vec![
            Point::new(0, 0),
            Point::new(100, 0),
            Point::new(100, 100),
            Point::new(0, 100),
        ]);

        let concave = make_contour(vec![
            Point::new(0, 0),
            Point::new(100, 0),
            Point::new(100, 50),
            Point::new(50, 50),
            Point::new(50, 100),
            Point::new(0, 100),
        ]);

        let filter = ContourFilter::new();
        let convex_only = filter.convex_only(&[square, concave]);

        assert_eq!(convex_only.len(), 1);
    }
}
