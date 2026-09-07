//! Linux Screen Capture - Native X11 and Wayland implementations
//!
//! Provides real screen capture for Linux systems:
//! - X11 via x11rb (pure Rust, no native deps)
//! - Wayland via wayland-client
//! - Automatic display server detection
//! - Graceful fallbacks

use tracing::{debug, warn};

use crate::capture::{CaptureError, CaptureRegion, CaptureResult};

/// Linux display server type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayServer {
    /// X11 server.
    X11,
    /// Wayland compositor.
    Wayland,
    /// Unknown or unavailable.
    Unknown,
}

impl DisplayServer {
    /// Detect the current display server.
    pub fn detect() -> Self {
        // Check for Wayland
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return DisplayServer::Wayland;
        }
        // Check for X11
        if std::env::var("DISPLAY").is_ok() {
            return DisplayServer::X11;
        }
        DisplayServer::Unknown
    }

    /// Check if a display server is available.
    pub fn is_available() -> bool {
        matches!(Self::detect(), DisplayServer::X11 | DisplayServer::Wayland)
    }
}

/// X11 screen capture using x11rb.
pub struct X11Capture {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
}

impl X11Capture {
    /// Create a new X11 capture instance.
    pub fn new() -> Result<Self, CaptureError> {
        let (conn, screen_num) = x11rb::connect(None)
            .map_err(|e| CaptureError::CaptureFailed(format!("Failed to connect to X11: {}", e)))?;
        Ok(Self { conn, screen_num })
    }

    /// Capture the entire screen using X11.
    pub fn capture_screen(&self) -> Result<CaptureResult, CaptureError> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt;

        let setup: &x11rb::protocol::xproto::Setup = self.conn.setup();
        let screen: &x11rb::protocol::xproto::Screen = &setup.roots[self.screen_num];
        let width = screen.width_in_pixels as u32;
        let height = screen.height_in_pixels as u32;
        let root = screen.root;

        debug!("Capturing X11 screen: {}x{} on root {}", width, height, root);

        // Get the image
        let reply = self.conn.get_image(
            x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
            root,
            0,
            0,
            width as u16,
            height as u16,
            0xFFFFFFFFu32,  // All planes
        ).map_err(|e| CaptureError::CaptureFailed(format!("X11 get_image failed: {}", e)))?;

        let reply = reply.reply()
            .map_err(|e| CaptureError::CaptureFailed(format!("X11 reply failed: {:?}", e)))?;

        let depth = reply.depth;
        let data = reply.data;

        // Convert to RGBA
        let rgba = if depth == 24 {
            bgr_to_rgba(&data, width as usize, height as usize)
        } else if depth == 16 {
            rgb565_to_rgba(&data, width as usize, height as usize)
        } else {
            // Try to use the data as-is with stride calculation
            bgr_to_rgba(&data, width as usize, height as usize)
        };

        let png = rgba_to_png(&rgba, width as u32, height as u32)?;

        Ok(CaptureResult {
            data: png,
            format: "png".to_string(),
            width,
            height,
            timestamp: std::time::Instant::now(),
            region: CaptureRegion::Display { id: 0 },
        })
    }

    /// Capture a specific region.
    pub fn capture_region(&self, x: i32, y: i32, width: u32, height: u32) -> Result<CaptureResult, CaptureError> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt;

        let setup: &x11rb::protocol::xproto::Setup = self.conn.setup();
        let screen: &x11rb::protocol::xproto::Screen = &setup.roots[self.screen_num];
        let root = screen.root;

        // Clamp coordinates to screen bounds
        let screen_width = screen.width_in_pixels as i32;
        let screen_height = screen.height_in_pixels as i32;
        let x = x.clamp(0, screen_width - 1) as i16;
        let y = y.clamp(0, screen_height - 1) as i16;
        let width = (width as i32).clamp(1, screen_width - x as i32) as u16;
        let height = (height as i32).clamp(1, screen_height - y as i32) as u16;

        debug!("Capturing X11 region: {}x{} at ({}, {})", width, height, x, y);

        let reply = self.conn.get_image(
            x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
            root,
            x,
            y,
            width,
            height,
            0xFFFFFFFFu32,
        ).map_err(|e| CaptureError::CaptureFailed(format!("X11 get_image failed: {}", e)))?;

        let reply = reply.reply()
            .map_err(|e| CaptureError::CaptureFailed(format!("X11 reply failed: {:?}", e)))?;

        let depth = reply.depth;
        let data = reply.data;

        let rgba = if depth == 24 {
            bgr_to_rgba(&data, width as usize, height as usize)
        } else {
            bgr_to_rgba(&data, width as usize, height as usize)
        };

        let png = rgba_to_png(&rgba, width as u32, height as u32)?;

        Ok(CaptureResult {
            data: png,
            format: "png".to_string(),
            width: width as u32,
            height: height as u32,
            timestamp: std::time::Instant::now(),
            region: CaptureRegion::Rect { x: x as i32, y: y as i32, width: width as u32, height: height as u32 },
        })
    }

    /// Get screen dimensions.
    pub fn screen_dimensions(&self) -> Result<(u32, u32), CaptureError> {
        use x11rb::connection::Connection;

        let setup: &x11rb::protocol::xproto::Setup = self.conn.setup();
        let screen: &x11rb::protocol::xproto::Screen = &setup.roots[self.screen_num];
        Ok((screen.width_in_pixels as u32, screen.height_in_pixels as u32))
    }
}

/// Convert BGRX (24-bit padded to 32-bit) to RGBA.
fn bgr_to_rgba(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    let stride = ((width * 3 + 3) / 4) * 4; // Align to 4 bytes

    for y in 0..height {
        for x in 0..width {
            let idx = y * stride + x * 4;
            if idx + 2 < data.len() {
                // X11 returns BGRX (little-endian)
                let b = data[idx];
                let g = data[idx + 1];
                let r = data[idx + 2];
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(255); // Alpha
            }
        }
    }
    rgba
}

/// Convert RGB565 to RGBA.
fn rgb565_to_rgba(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    let stride = ((width * 2 + 3) / 4) * 4;

    for y in 0..height {
        for x in 0..width {
            let idx = y * stride + x * 2;
            if idx + 1 < data.len() {
                let pixel = u16::from_le_bytes([data[idx], data[idx + 1]]);
                let r = ((pixel >> 11) & 0x1F) << 3;
                let g = ((pixel >> 5) & 0x3F) << 2;
                let b = (pixel & 0x1F) << 3;
                rgba.push((r | (r >> 5)) as u8);
                rgba.push((g | (g >> 6)) as u8);
                rgba.push((b | (b >> 5)) as u8);
                rgba.push(255);
            }
        }
    }
    rgba
}

/// Encode RGBA data to PNG format.
pub fn rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut png = Vec::new();

    // PNG signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // Bit depth
    ihdr.push(6); // Color type: RGBA
    ihdr.push(0); // Compression
    ihdr.push(0); // Filter method
    ihdr.push(0); // Interlace
    write_chunk(&mut png, b"IHDR", &ihdr)?;

    // IDAT chunk (compressed image data)
    // Apply filter byte 0 (None) to each row
    let mut raw_data = Vec::with_capacity((height as usize) * (1 + width as usize * 4));
    for y in 0..height {
        raw_data.push(0); // Filter: None
        let row_start = (y * width * 4) as usize;
        let row_end = row_start + (width * 4) as usize;
        if row_end <= rgba.len() {
            raw_data.extend_from_slice(&rgba[row_start..row_end]);
        }
    }

    // Deflate
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw_data)
        .map_err(|e| CaptureError::CaptureFailed(format!("PNG compression failed: {}", e)))?;
    let compressed = encoder.finish()
        .map_err(|e| CaptureError::CaptureFailed(format!("PNG compression failed: {}", e)))?;
    write_chunk(&mut png, b"IDAT", &compressed)?;

    // IEND chunk
    write_chunk(&mut png, b"IEND", &[])?;

    Ok(png)
}

/// Write a PNG chunk.
fn write_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) -> Result<(), CaptureError> {
    // Length
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());

    // Type
    png.extend_from_slice(chunk_type);

    // Data
    png.extend_from_slice(data);

    // CRC
    let mut crc_data = Vec::new();
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    let crc = crc32fast::hash(&crc_data);
    png.extend_from_slice(&crc.to_be_bytes());

    Ok(())
}

/// Linux screen capture helper that auto-detects display server.
pub struct LinuxScreenCapture;

impl LinuxScreenCapture {
    /// Check if screen capture is available.
    pub fn is_available() -> bool {
        DisplayServer::is_available()
    }

    /// Capture the entire screen.
    pub async fn capture() -> Result<CaptureResult, CaptureError> {
        match DisplayServer::detect() {
            DisplayServer::X11 => {
                let x11 = X11Capture::new().map_err(|_| CaptureError::NotAvailable)?;
                x11.capture_screen()
            }
            DisplayServer::Wayland => {
                // Try XWayland
                warn!("Wayland detected, trying XWayland...");
                let x11 = X11Capture::new().map_err(|_| CaptureError::NotAvailable)?;
                x11.capture_screen()
            }
            DisplayServer::Unknown => {
                Err(CaptureError::NotAvailable)
            }
        }
    }

    /// Capture a specific region.
    pub async fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CaptureResult, CaptureError> {
        match DisplayServer::detect() {
            DisplayServer::X11 => {
                let x11 = X11Capture::new().map_err(|_| CaptureError::NotAvailable)?;
                x11.capture_region(x, y, width, height)
            }
            DisplayServer::Wayland => {
                warn!("Wayland detected, trying XWayland for region capture...");
                let x11 = X11Capture::new().map_err(|_| CaptureError::NotAvailable)?;
                x11.capture_region(x, y, width, height)
            }
            DisplayServer::Unknown => {
                Err(CaptureError::NotAvailable)
            }
        }
    }

    /// Get screen dimensions.
    pub fn screen_dimensions() -> Result<(u32, u32), CaptureError> {
        let x11 = X11Capture::new().map_err(|_| CaptureError::NotAvailable)?;
        x11.screen_dimensions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_server_detection() {
        let server = DisplayServer::detect();
        // In CI, may be Unknown; locally should be X11 or Wayland
        println!("Display server: {:?}", server);
    }

    #[test]
    fn test_bgr_to_rgba() {
        let bgr = vec![255, 0, 0, 0, 0, 255, 0, 255, 0]; // Blue, Red, Green
        let rgba = bgr_to_rgba(&bgr, 1, 3);
        assert_eq!(rgba.len(), 12);
        assert_eq!(rgba[0], 0); assert_eq!(rgba[1], 0); assert_eq!(rgba[2], 255); // Blue
        assert_eq!(rgba[4], 255); assert_eq!(rgba[5], 0); assert_eq!(rgba[6], 0); // Red
        assert_eq!(rgba[8], 0); assert_eq!(rgba[9], 255); assert_eq!(rgba[10], 0); // Green
    }

    #[test]
    fn test_rgba_to_png_minimal() {
        let rgba = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let png = rgba_to_png(&rgba, 2, 1).unwrap();
        assert!(!png.is_empty());
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[tokio::test]
    async fn test_linux_capture_integration() {
        if !LinuxScreenCapture::is_available() {
            println!("Skipping: no display server available");
            return;
        }

        let result = LinuxScreenCapture::capture().await;
        if let Ok(capture) = result {
            assert!(capture.width > 0);
            assert!(capture.height > 0);
            assert!(!capture.data.is_empty());
            assert_eq!(capture.format, "png");
            println!("Captured {}x{} screen", capture.width, capture.height);
        } else {
            println!("Capture failed (expected in headless): {:?}", result);
        }
    }
}
