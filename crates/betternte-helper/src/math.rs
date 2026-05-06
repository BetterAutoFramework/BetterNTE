//! Math utilities

/// Point structure for 2D coordinates
#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Calculate distance between two points
pub fn distance(p1: &Point, p2: &Point) -> f64 {
    ((p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2)).sqrt()
}

/// Calculate distance from point to line defined by two points
/// Uses the formula: |Ax + By + C| / sqrt(A^2 + B^2)
pub fn point_to_line_distance(point: &Point, line_p1: &Point, line_p2: &Point) -> f64 {
    let a = line_p2.y - line_p1.y;
    let b = line_p1.x - line_p2.x;
    let c = line_p2.x * line_p1.y - line_p1.x * line_p2.y;

    (a * point.x + b * point.y + c).abs() / (a * a + b * b).sqrt()
}

/// Calculate the perpendicular distance from point to line segment
pub fn point_to_segment_distance(point: &Point, seg_p1: &Point, seg_p2: &Point) -> f64 {
    let dx = seg_p2.x - seg_p1.x;
    let dy = seg_p2.y - seg_p1.y;

    if dx == 0.0 && dy == 0.0 {
        return distance(point, seg_p1);
    }

    let t = ((point.x - seg_p1.x) * dx + (point.y - seg_p1.y) * dy) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);

    let closest = Point::new(seg_p1.x + t * dx, seg_p1.y + t * dy);

    distance(point, &closest)
}

/// Linear interpolation between two values
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Normalize angle to [0, 360) range
pub fn normalize_angle(angle: f64) -> f64 {
    let angle = angle % 360.0;
    if angle < 0.0 {
        angle + 360.0
    } else {
        angle
    }
}

/// Calculate angle between two points in degrees
pub fn angle_between_points(p1: &Point, p2: &Point) -> f64 {
    (p2.y - p1.y).atan2(p2.x - p1.x) * 180.0 / std::f64::consts::PI
}

/// Check if point is inside rectangle
pub fn point_in_rect(point: &Point, rect: &Rect) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

/// Rectangle structure
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// Calculate intersection area of two rectangles
pub fn rect_intersection(r1: &Rect, r2: &Rect) -> Option<Rect> {
    let x1 = r1.x.max(r2.x);
    let y1 = r1.y.max(r2.y);
    let x2 = (r1.x + r1.width).min(r2.x + r2.width);
    let y2 = (r1.y + r1.height).min(r2.y + r2.height);

    if x1 < x2 && y1 < y2 {
        Some(Rect::new(x1, y1, x2 - x1, y2 - y1))
    } else {
        None
    }
}

/// Calculate union area of two rectangles
pub fn rect_union(r1: &Rect, r2: &Rect) -> Rect {
    let x1 = r1.x.min(r2.x);
    let y1 = r1.y.min(r2.y);
    let x2 = (r1.x + r1.width).max(r2.x + r2.width);
    let y2 = (r1.y + r1.height).max(r2.y + r2.height);

    Rect::new(x1, y1, x2 - x1, y2 - y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        assert!((distance(&p1, &p2) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_to_line() {
        let point = Point::new(0.0, 0.0);
        let line_p1 = Point::new(-1.0, 0.0);
        let line_p2 = Point::new(1.0, 0.0);
        assert!((point_to_line_distance(&point, &line_p1, &line_p2) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_to_segment_distance_middle() {
        let point = Point::new(0.0, 1.0);
        let p1 = Point::new(-1.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        assert!((point_to_segment_distance(&point, &p1, &p2) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_to_segment_distance_beyond_endpoint() {
        let point = Point::new(3.0, 0.0);
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        // Closest point is (1,0), distance = 2.0
        assert!((point_to_segment_distance(&point, &p1, &p2) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_to_segment_distance_degenerate() {
        let point = Point::new(3.0, 4.0);
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(0.0, 0.0);
        assert!((point_to_segment_distance(&point, &p1, &p2) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp() {
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 1e-10);
        assert!((lerp(0.0, 10.0, 0.0) - 0.0).abs() < 1e-10);
        assert!((lerp(0.0, 10.0, 1.0) - 10.0).abs() < 1e-10);
        assert!((lerp(5.0, 15.0, 0.25) - 7.5).abs() < 1e-10);
    }

    #[test]
    fn test_normalize_angle() {
        assert!((normalize_angle(370.0) - 10.0).abs() < 1e-10);
        assert!((normalize_angle(-10.0) - 350.0).abs() < 1e-10);
        assert!((normalize_angle(0.0) - 0.0).abs() < 1e-10);
        assert!((normalize_angle(360.0) - 0.0).abs() < 1e-10);
        assert!((normalize_angle(720.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_angle_between_points() {
        let origin = Point::new(0.0, 0.0);
        // Right (0 degrees)
        assert!((angle_between_points(&origin, &Point::new(1.0, 0.0)) - 0.0).abs() < 1e-10);
        // Up (90 degrees)
        assert!((angle_between_points(&origin, &Point::new(0.0, 1.0)) - 90.0).abs() < 1e-10);
        // Left (180 degrees)
        assert!(
            (angle_between_points(&origin, &Point::new(-1.0, 0.0)).abs() - 180.0).abs() < 1e-10
        );
    }

    #[test]
    fn test_point_in_rect_inside() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(point_in_rect(&Point::new(5.0, 5.0), &rect));
    }

    #[test]
    fn test_point_in_rect_on_edge() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(point_in_rect(&Point::new(0.0, 0.0), &rect));
        assert!(point_in_rect(&Point::new(10.0, 10.0), &rect));
    }

    #[test]
    fn test_point_in_rect_outside() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!point_in_rect(&Point::new(11.0, 5.0), &rect));
        assert!(!point_in_rect(&Point::new(-1.0, 5.0), &rect));
    }

    #[test]
    fn test_rect_center() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let c = rect.center();
        assert!((c.x - 5.0).abs() < 1e-10);
        assert!((c.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_rect_intersection_overlapping() {
        let r1 = Rect::new(0.0, 0.0, 10.0, 10.0);
        let r2 = Rect::new(5.0, 5.0, 10.0, 10.0);
        let inter = rect_intersection(&r1, &r2).unwrap();
        assert!((inter.x - 5.0).abs() < 1e-10);
        assert!((inter.y - 5.0).abs() < 1e-10);
        assert!((inter.width - 5.0).abs() < 1e-10);
        assert!((inter.height - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_rect_intersection_disjoint() {
        let r1 = Rect::new(0.0, 0.0, 5.0, 5.0);
        let r2 = Rect::new(10.0, 10.0, 5.0, 5.0);
        assert!(rect_intersection(&r1, &r2).is_none());
    }

    #[test]
    fn test_rect_intersection_contained() {
        let r1 = Rect::new(0.0, 0.0, 10.0, 10.0);
        let r2 = Rect::new(2.0, 2.0, 3.0, 3.0);
        let inter = rect_intersection(&r1, &r2).unwrap();
        assert!((inter.x - 2.0).abs() < 1e-10);
        assert!((inter.width - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_rect_union() {
        let r1 = Rect::new(0.0, 0.0, 5.0, 5.0);
        let r2 = Rect::new(10.0, 10.0, 5.0, 5.0);
        let u = rect_union(&r1, &r2);
        assert!((u.x - 0.0).abs() < 1e-10);
        assert!((u.y - 0.0).abs() < 1e-10);
        assert!((u.width - 15.0).abs() < 1e-10);
        assert!((u.height - 15.0).abs() < 1e-10);
    }
}
