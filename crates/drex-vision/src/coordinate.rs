//! Coordinate Mapping - Map semantic descriptions to screen coordinates
//!
//! Converts natural language descriptions into specific
//! screen coordinates that can be acted upon.
//!
//! Examples:
//! - "the OK button" -> (960, 540)
//! - "top-left corner" -> (50, 50)
//! - "the text field labeled 'Search'" -> (400, 200)

use std::sync::Arc;
use tracing::{debug, error, info};

/// Screen coordinate (x, y) position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenCoordinate {
    pub x: i32,
    pub y: i32,
}

impl ScreenCoordinate {
    /// Create a new coordinate.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Calculate distance to another coordinate.
    pub fn distance_to(&self, other: &ScreenCoordinate) -> f64 {
        let dx = (self.x - other.x) as f64;
        let dy = (self.y - other.y) as f64;
        (dx * dx + dy * dy).sqrt()
    }

    /// Check if coordinate is within bounds.
    pub fn is_within(&self, screen_width: i32, screen_height: i32) -> bool {
        self.x >= 0 && self.x < screen_width && self.y >= 0 && self.y < screen_height
    }
}

/// Coordinate configuration.
#[derive(Debug, Clone)]
pub struct CoordinateConfig {
    /// Screen width.
    pub screen_width: i32,
    /// Screen height.
    pub screen_height: i32,
    /// Offset for window decorations (x, y).
    pub window_offset: (i32, i32),
    /// Scale factor (for high-DPI displays).
    pub scale_factor: f64,
}

impl Default for CoordinateConfig {
    fn default() -> Self {
        Self {
            screen_width: 1920,
            screen_height: 1080,
            window_offset: (0, 0),
            scale_factor: 1.0,
        }
    }
}

/// Errors that can occur during coordinate operations.
#[derive(Debug, thiserror::Error)]
pub enum CoordinateError {
    /// Description not understood.
    #[error("Could not understand description: {0}")]
    UnknownDescription(String),

    /// Coordinate out of bounds.
    #[error("Coordinate ({0}, {1}) is outside screen bounds")]
    OutOfBounds(i32, i32),

    /// Element not found.
    #[error("Element not found: {0}")]
    ElementNotFound(String),

    /// Ambiguous description (multiple matches).
    #[error("Ambiguous description - multiple elements match: {0}")]
    AmbiguousDescription(String),
}

/// Rectangle region on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRegion {
    /// Create a new region.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Get center point of the region.
    pub fn center(&self) -> ScreenCoordinate {
        ScreenCoordinate::new(
            self.x + (self.width as i32 / 2),
            self.y + (self.height as i32 / 2),
        )
    }

    /// Check if a point is inside the region.
    pub fn contains(&self, point: &ScreenCoordinate) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width as i32
            && point.y >= self.y
            && point.y < self.y + self.height as i32
    }
}

/// Coordinate mapper that converts descriptions to coordinates.
pub struct CoordinateMapper {
    config: CoordinateConfig,
    known_positions: std::sync::Mutex<Vec<(String, ScreenCoordinate)>>,
}

impl CoordinateMapper {
    /// Create a new coordinate mapper.
    pub fn new(config: CoordinateConfig) -> Self {
        debug!("Creating coordinate mapper for {}x{} screen", config.screen_width, config.screen_height);

        // Initialize with common positions
        let mut positions = Vec::new();

        // Center
        positions.push(("center".to_string(), ScreenCoordinate::new(
            config.screen_width / 2,
            config.screen_height / 2,
        )));

        // Corners
        positions.push(("top-left".to_string(), ScreenCoordinate::new(0, 0)));
        positions.push(("top-right".to_string(), ScreenCoordinate::new(config.screen_width - 1, 0)));
        positions.push(("bottom-left".to_string(), ScreenCoordinate::new(0, config.screen_height - 1)));
        positions.push(("bottom-right".to_string(), ScreenCoordinate::new(config.screen_width - 1, config.screen_height - 1)));

        // Center of edges
        positions.push(("top-center".to_string(), ScreenCoordinate::new(config.screen_width / 2, 0)));
        positions.push(("bottom-center".to_string(), ScreenCoordinate::new(config.screen_width / 2, config.screen_height - 1)));
        positions.push(("left-center".to_string(), ScreenCoordinate::new(0, config.screen_height / 2)));
        positions.push(("right-center".to_string(), ScreenCoordinate::new(config.screen_width - 1, config.screen_height / 2)));

        Self {
            config,
            known_positions: std::sync::Mutex::new(positions),
        }
    }

    /// Add a known position.
    pub fn add_position(&self, name: &str, coordinate: ScreenCoordinate) {
        let mut positions = self.known_positions.lock().unwrap();
        positions.push((name.to_string(), coordinate));
    }

    /// Map a description to a coordinate.
    pub fn map_to_coordinate(&self, description: &str) -> Result<ScreenCoordinate, CoordinateError> {
        let normalized = description.to_lowercase().trim().to_string();

        // Check known positions first
        let positions = self.known_positions.lock().unwrap();
        for (name, coord) in positions.iter() {
            if name == &normalized {
                return Ok(*coord);
            }
        }
        drop(positions);

        // Try to parse as explicit coordinates
        if let Some(coord) = self.parse_explicit_coordinates(&normalized) {
            return Ok(coord);
        }

        // Try to parse percentages
        if let Some(coord) = self.parse_percentages(&normalized) {
            return Ok(coord);
        }

        // Fallback: unknown
        Err(CoordinateError::UnknownDescription(description.to_string()))
    }

    /// Map a description to a region.
    pub fn map_to_region(&self, description: &str) -> Result<ScreenRegion, CoordinateError> {
        // Common regions
        let normalized = description.to_lowercase();

        match normalized.as_str() {
            "fullscreen" | "full screen" | "entire screen" => Ok(ScreenRegion::new(
                0, 0, self.config.screen_width as u32, self.config.screen_height as u32,
            )),
            "center quarter" | "center region" => {
                let w = self.config.screen_width / 2;
                let h = self.config.screen_height / 2;
                Ok(ScreenRegion::new(w / 2, h / 2, w as u32, h as u32))
            }
            "left half" => Ok(ScreenRegion::new(
                0, 0,
                (self.config.screen_width / 2) as u32,
                self.config.screen_height as u32,
            )),
            "right half" => Ok(ScreenRegion::new(
                self.config.screen_width / 2, 0,
                (self.config.screen_width / 2) as u32,
                self.config.screen_height as u32,
            )),
            "top half" => Ok(ScreenRegion::new(
                0, 0,
                self.config.screen_width as u32,
                (self.config.screen_height / 2) as u32,
            )),
            "bottom half" => Ok(ScreenRegion::new(
                0, self.config.screen_height / 2,
                self.config.screen_width as u32,
                (self.config.screen_height / 2) as u32,
            )),
            _ => {
                // Try as single coordinate (assume small region around it)
                let coord = self.map_to_coordinate(description)?;
                Ok(ScreenRegion::new(coord.x - 50, coord.y - 50, 100, 100))
            }
        }
    }

    /// Map normalized coordinates (0.0 to 1.0) to screen coordinates.
    pub fn normalize_to_screen(&self, nx: f64, ny: f64) -> ScreenCoordinate {
        let x = (nx * self.config.screen_width as f64) as i32;
        let y = (ny * self.config.screen_height as f64) as i32;
        ScreenCoordinate::new(x.clamp(0, self.config.screen_width - 1), y.clamp(0, self.config.screen_height - 1))
    }

    /// Get screen dimensions.
    pub fn screen_dimensions(&self) -> (i32, i32) {
        (self.config.screen_width, self.config.screen_height)
    }

    /// Parse explicit coordinate strings like "100, 200" or "x=100 y=200".
    fn parse_explicit_coordinates(&self, input: &str) -> Option<ScreenCoordinate> {
        // Handle "x,y" format
        if let Some(comma_pos) = input.find(',') {
            let x_str = &input[..comma_pos].trim();
            let y_str = &input[comma_pos + 1..].trim();
            if let (Ok(x), Ok(y)) = (x_str.parse::<i32>(), y_str.parse::<i32>()) {
                return Some(ScreenCoordinate::new(x, y));
            }
        }

        // Handle "x y" format with space
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() == 2 {
            if let (Ok(x), Ok(y)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                return Some(ScreenCoordinate::new(x, y));
            }
        }

        None
    }

    /// Parse percentage strings like "50% 50%" or "center at 50%".
    fn parse_percentages(&self, input: &str) -> Option<ScreenCoordinate> {
        // Simple percentage parsing - extract numbers followed by %
        let mut percentages = Vec::new();
        for word in input.split_whitespace() {
            if word.ends_with('%') {
                if let Ok(val) = word[..word.len() - 1].parse::<f64>() {
                    percentages.push(val / 100.0);
                }
            }
        }

        if percentages.len() >= 2 {
            return Some(self.normalize_to_screen(percentages[0], percentages[1]));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinate_new() {
        let coord = ScreenCoordinate::new(100, 200);
        assert_eq!(coord.x, 100);
        assert_eq!(coord.y, 200);
    }

    #[test]
    fn test_coordinate_distance() {
        let a = ScreenCoordinate::new(0, 0);
        let b = ScreenCoordinate::new(3, 4);
        assert_eq!(a.distance_to(&b), 5.0);
    }

    #[test]
    fn test_coordinate_is_within() {
        let coord = ScreenCoordinate::new(100, 100);
        assert!(coord.is_within(1920, 1080));
        assert!(!coord.is_within(50, 50));
    }

    #[test]
    fn test_region_center() {
        let region = ScreenRegion::new(0, 0, 200, 100);
        let center = region.center();
        assert_eq!(center, ScreenCoordinate::new(100, 50));
    }

    #[test]
    fn test_region_contains() {
        let region = ScreenRegion::new(10, 10, 100, 100);
        assert!(region.contains(&ScreenCoordinate::new(50, 50)));
        assert!(!region.contains(&ScreenCoordinate::new(200, 200)));
    }

    #[test]
    fn test_mapper_known_positions() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let center = mapper.map_to_coordinate("center").unwrap();
        assert_eq!(center, ScreenCoordinate::new(960, 540));

        let top_left = mapper.map_to_coordinate("top-left").unwrap();
        assert_eq!(top_left, ScreenCoordinate::new(0, 0));
    }

    #[test]
    fn test_mapper_explicit_coordinates() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let coord = mapper.map_to_coordinate("100, 200").unwrap();
        assert_eq!(coord, ScreenCoordinate::new(100, 200));

        let coord2 = mapper.map_to_coordinate("50 100").unwrap();
        assert_eq!(coord2, ScreenCoordinate::new(50, 100));
    }

    #[test]
    fn test_mapper_percentages() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let coord = mapper.map_to_coordinate("50% 50%").unwrap();
        assert_eq!(coord, ScreenCoordinate::new(960, 540));
    }

    #[test]
    fn test_mapper_regions() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let fullscreen = mapper.map_to_region("fullscreen").unwrap();
        assert_eq!(fullscreen.width, 1920);
        assert_eq!(fullscreen.height, 1080);

        let left_half = mapper.map_to_region("left half").unwrap();
        assert_eq!(left_half.width, 960);
    }

    #[test]
    fn test_mapper_add_position() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);
        mapper.add_position("custom", ScreenCoordinate::new(500, 500));

        let coord = mapper.map_to_coordinate("custom").unwrap();
        assert_eq!(coord, ScreenCoordinate::new(500, 500));
    }

    #[test]
    fn test_mapper_unknown_description() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let result = mapper.map_to_coordinate("unknown place xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_to_screen() {
        let config = CoordinateConfig::default();
        let mapper = CoordinateMapper::new(config);

        let coord = mapper.normalize_to_screen(0.5, 0.5);
        assert_eq!(coord, ScreenCoordinate::new(960, 540));
    }
}
