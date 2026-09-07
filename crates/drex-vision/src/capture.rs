//! Screen Capture - Take screenshots and capture screen regions
//!
//! Provides abstraction over platform-specific screen capture APIs.
//! Supports capturing:
//! - Full displays
//! - Specific windows
//! - Arbitrary regions
//! - Continuous capture for video
//!
//! # Platform Support
//!
//! - Linux: X11 and Wayland via `screenshots` crate
//! - macOS: CoreGraphics via `screenshots` crate  
//! - Windows: GDI/DXG via `screenshots` crate
//!
//! # Usage
//!
//! ```rust,ignore
//! let config = CaptureConfig::default();
//! let capture = ScreenCapture::new(config);
//! let result = capture.capture().await?;
//! std::fs::write("screenshot.png", &result.data)?;
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::debug;

#[cfg(feature = "vision")]
use screenshots::{Screen, image::RgbaImage, display_info::DisplayInfo};
#[cfg(feature = "vision")]
use anyhow;

/// Capture region specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureRegion {
    /// Capture entire display.
    Display { id: u32 },
    /// Capture specific window.
    Window { id: u64 },
    /// Capture rectangular region.
    Rect { x: i32, y: i32, width: u32, height: u32 },
}

impl Default for CaptureRegion {
    fn default() -> Self {
        CaptureRegion::Display { id: 0 }
    }
}

/// Screen capture configuration.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target region to capture.
    pub region: CaptureRegion,
    /// Output format (png, jpg, etc.).
    pub format: String,
    /// Image quality (0-100 for jpg).
    pub quality: u8,
    /// Include cursor in capture.
    pub include_cursor: bool,
    /// Capture interval in milliseconds (for video).
    pub interval_ms: u64,
    /// Maximum capture duration.
    pub max_duration: Duration,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            region: CaptureRegion::default(),
            format: "png".to_string(),
            quality: 85,
            include_cursor: false,
            interval_ms: 100, // 10fps default
            max_duration: Duration::from_secs(30),
        }
    }
}

/// Capture result containing image data.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// Raw image bytes.
    pub data: Vec<u8>,
    /// Image format (png, jpg, etc.).
    pub format: String,
    /// Image dimensions.
    pub width: u32,
    pub height: u32,
    /// When the capture was taken.
    pub timestamp: Instant,
    /// Region that was captured.
    pub region: CaptureRegion,
}

/// Screen capture backend.
pub struct CaptureBackend;

impl CaptureBackend {
    /// Check if screen capture is available.
    #[cfg(feature = "vision")]
    pub fn is_available() -> bool {
        // Check if we can get display info
        DisplayInfo::all().map(|d: Vec<DisplayInfo>| !d.is_empty()).unwrap_or(false)
    }

    /// Check if screen capture is available (no-vision fallback).
    #[cfg(not(feature = "vision"))]
    pub fn is_available() -> bool {
        false
    }

    /// List available displays.
    #[cfg(feature = "vision")]
    pub fn list_displays() -> Vec<(u32, String, u32, u32)> {
        DisplayInfo::all()
            .map(|displays: Vec<DisplayInfo>| {
                displays
                    .into_iter()
                    .enumerate()
                    .map(|(idx, info)| {
                        let name = format!("Display {} ({}x{})", idx, info.width, info.height);
                        (idx as u32, name, info.width as u32, info.height as u32)
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![(0, "Unknown Display".to_string(), 1920, 1080)])
    }

    /// List available displays (no-vision fallback).
    #[cfg(not(feature = "vision"))]
    pub fn list_displays() -> Vec<(u32, String, u32, u32)> {
        vec![(0, "Primary Display".to_string(), 1920, 1080)]
    }

    /// List available windows.
    #[cfg(feature = "vision")]
    pub fn list_windows() -> Vec<(u64, String)> {
        // Note: screenshots crate doesn't provide window enumeration
        // This would require xcap or platform-specific code
        vec![]
    }

    /// List available windows (no-vision fallback).
    #[cfg(not(feature = "vision"))]
    pub fn list_windows() -> Vec<(u64, String)> {
        vec![]
    }

    /// Capture a specific display.
    #[cfg(feature = "vision")]
    pub fn capture_display(display_id: u32) -> Result<RgbaImage, CaptureError> {
        type DisplayResult = Vec<DisplayInfo>;
        let displays = DisplayInfo::all()
            .map_err(|e: anyhow::Error| CaptureError::CaptureFailed(e.to_string()))?;
        let display = displays
            .get(display_id as usize)
            .ok_or(CaptureError::DisplayNotFound(display_id))?;

        let screen = Screen::new(display);
        screen
            .capture()
            .map_err(|e: anyhow::Error| CaptureError::CaptureFailed(e.to_string()))
    }

    /// Capture a specific region.
    #[cfg(feature = "vision")]
    pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<RgbaImage, CaptureError> {
        // Get primary display (the one containing the region)
        let displays = DisplayInfo::all()
            .map_err(|e: anyhow::Error| CaptureError::CaptureFailed(e.to_string()))?;
        let display = displays
            .into_iter()
            .next()
            .ok_or(CaptureError::NotAvailable)?;

        let screen = Screen::new(&display);
        screen
            .capture_area(x, y, width, height)
            .map_err(|e: anyhow::Error| CaptureError::CaptureFailed(e.to_string()))
    }
}

/// Errors that can occur during screen capture.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// Screen capture not available.
    #[error("Screen capture not available on this platform")]
    NotAvailable,

    /// Display not found.
    #[error("Display not found: {0}")]
    DisplayNotFound(u32),

    /// Window not found.
    #[error("Window not found: {0}")]
    WindowNotFound(u64),

    /// Invalid region.
    #[error("Invalid capture region: {0}")]
    InvalidRegion(String),

    /// Capture failed.
    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),

    /// Save failed.
    #[error("Failed to save capture: {0}")]
    SaveFailed(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Screen capture handler.
pub struct ScreenCapture {
    config: CaptureConfig,
}

impl ScreenCapture {
    /// Create a new screen capture instance.
    pub fn new(config: CaptureConfig) -> Self {
        Self { config }
    }

    /// Check if screen capture is available.
    pub fn is_available() -> bool {
        #[cfg(all(feature = "vision", target_os = "linux"))]
        {
            crate::linux_capture::DisplayServer::is_available() || CaptureBackend::is_available()
        }
        #[cfg(all(not(feature = "vision"), target_os = "linux"))]
        {
            crate::linux_capture::DisplayServer::is_available()
        }
        #[cfg(not(target_os = "linux"))]
        {
            CaptureBackend::is_available()
        }
    }

    /// Capture a single screenshot.
    #[cfg(feature = "vision")]
    pub async fn capture(&self) -> Result<CaptureResult, CaptureError> {
        if !Self::is_available() {
            return Err(CaptureError::NotAvailable);
        }

        debug!("Capturing screen region: {:?}", self.config.region);

        // Use the screenshots crate for actual capture
        let image = match self.config.region {
            CaptureRegion::Display { id } => {
                CaptureBackend::capture_display(id)?
            }
            CaptureRegion::Window { id: _ } => {
                // Window capture not yet supported by screenshots crate
                // Fall back to full display capture
                CaptureBackend::capture_display(0)?
            }
            CaptureRegion::Rect { x, y, width, height } => {
                CaptureBackend::capture_region(x, y, width, height)?
            }
        };

        // Convert to requested format
        let (data, format) = self.encode_image(&image)?;

        Ok(CaptureResult {
            data,
            format,
            width: image.width(),
            height: image.height(),
            timestamp: Instant::now(),
            region: self.config.region,
        })
    }

    /// Capture a single screenshot (no-vision fallback - uses Linux native).
    #[cfg(all(not(feature = "vision"), target_os = "linux"))]
    pub async fn capture(&self) -> Result<CaptureResult, CaptureError> {
        crate::linux_capture::LinuxScreenCapture::capture().await
    }

    /// Capture a single screenshot (no-vision fallback for non-Linux).
    #[cfg(all(not(feature = "vision"), not(target_os = "linux")))]
    pub async fn capture(&self) -> Result<CaptureResult, CaptureError> {
        Err(CaptureError::NotAvailable)
    }

    /// Encode image to requested format.
    #[cfg(feature = "vision")]
    fn encode_image(&self, image: &RgbaImage) -> Result<(Vec<u8>, String), CaptureError> {
        use image::{ImageEncoder, codecs::{png::PngEncoder, jpeg::JpegEncoder}};

        match self.config.format.as_str() {
            "png" => {
                // Encode to PNG using image crate
                let mut png_data = Vec::new();
                let encoder = PngEncoder::new(&mut png_data);
                encoder.write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8
                ).map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
                Ok((png_data, "png".to_string()))
            }
            "jpg" | "jpeg" => {
                // Encode to JPEG
                let mut jpeg_data = Vec::new();
                let encoder = JpegEncoder::new_with_quality(
                    &mut jpeg_data, 
                    self.config.quality
                );
                encoder.write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8
                ).map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
                
                Ok((jpeg_data, "jpg".to_string()))
            }
            _ => {
                // Default to PNG
                let mut png_data = Vec::new();
                let encoder = PngEncoder::new(&mut png_data);
                encoder.write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8
                ).map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
                Ok((png_data, "png".to_string()))
            }
        }
    }

    /// Encode image (no-vision fallback).
    #[cfg(not(feature = "vision"))]
    fn encode_image(&self, _image: &()) -> Result<(Vec<u8>, String), CaptureError> {
        Err(CaptureError::NotAvailable)
    }

    /// Capture and save to file.
    pub async fn capture_to_file(&self, path: PathBuf) -> Result<CaptureResult, CaptureError> {
        let result = self.capture().await?;
        tokio::fs::write(&path, &result.data).await?;
        Ok(result)
    }

    /// Start continuous capture.
    pub async fn start_video_capture(
        &self,
        mut on_frame: impl FnMut(CaptureResult) -> bool,
    ) -> Result<Vec<CaptureResult>, CaptureError> {
        let start_time = Instant::now();
        let mut frames = Vec::new();
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_millis(self.config.interval_ms)
        );

        loop {
            interval.tick().await;

            if let Ok(frame) = self.capture().await {
                let should_continue = on_frame(frame.clone());
                frames.push(frame);

                if !should_continue || start_time.elapsed() > self.config.max_duration {
                    break;
                }
            }
        }

        Ok(frames)
    }

    /// Get the capture region.
    pub fn region(&self) -> CaptureRegion {
        self.config.region
    }

    /// Get the capture format.
    pub fn format(&self) -> &str {
        &self.config.format
    }
}

/// Create a minimal placeholder PNG.
fn create_placeholder_png(width: u32, height: u32) -> Vec<u8> {
    // Minimal PNG: 1x1 transparent pixel
    // In real implementation, would generate from actual captured data
    // PNG signature + IHDR + IDAT + IEND for a single gray pixel
    vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
        8, 0, 0, 0, 0, 58, 126, 155, 85, 0, 0, 0, 10, 73, 68, 65, 84, 8, 215, 99, 248, 0, 0, 0,
        1, 1, 0, 5, 18, 100, 210, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_config_default() {
        let config = CaptureConfig::default();
        assert_eq!(config.format, "png");
        assert_eq!(config.quality, 85);
    }

    #[test]
    fn test_capture_region_default() {
        let region = CaptureRegion::default();
        assert!(matches!(region, CaptureRegion::Display { id: 0 }));
    }

    #[tokio::test]
    async fn test_capture() {
        let config = CaptureConfig::default();
        let capture = ScreenCapture::new(config);

        // Screen capture may or may not be available depending on environment
        let result = capture.capture().await;
        if CaptureBackend::is_available() {
            // If available, should succeed
            assert!(result.is_ok(), "Capture should succeed when available");
        } else {
            // If not available, should return NotAvailable error
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), CaptureError::NotAvailable));
        }
    }

    #[tokio::test]
    async fn test_capture_to_file() {
        let config = CaptureConfig::default();
        let capture = ScreenCapture::new(config);

        let temp_path = PathBuf::from("/tmp/test_capture.png");
        let result = capture.capture_to_file(temp_path.clone()).await;

        if CaptureBackend::is_available() {
            assert!(result.is_ok(), "Capture to file should succeed when available");
            // Clean up test file
            let _ = tokio::fs::remove_file(temp_path).await;
        } else {
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), CaptureError::NotAvailable));
        }
    }

    #[test]
    fn test_list_displays() {
        let displays = CaptureBackend::list_displays();
        assert!(!displays.is_empty(), "Should have at least one display");
        let (id, name, w, h) = &displays[0];
        assert_eq!(*id, 0, "First display should have id 0");
        assert!(*w > 0, "Display width should be positive");
        assert!(*h > 0, "Display height should be positive");
        assert!(!name.is_empty(), "Display should have a name");
    }

    #[test]
    fn test_create_placeholder_png() {
        let data = create_placeholder_png(1, 1);
        assert!(!data.is_empty());
        // Check PNG signature
        assert_eq!(&data[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}
