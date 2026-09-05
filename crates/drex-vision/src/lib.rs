//! Drex Vision - Screen Capture and Vision Model Integration
//!
//! This crate provides computer vision capabilities for Drex:
//! - Screen capture for visual context
//! - Coordinate mapping for UI elements
//! - Vision model integration for describing what's on screen
//! - Observe-Act-Verify loop for safe computer control
//!
//! # Safety
//!
//! All vision operations require explicit user permission.
//! The Observe-Act-Verify loop ensures Drex:
//! 1. Observes what's on screen
//! 2. Plans safe actions
//! 3. Acts with human-readable descriptions
//! 4. Verifies changes match expectations
//!
//! # Architecture
//!
//! - **ScreenCapture**: Capture screen regions, windows, or displays
//! - **VisionModel**: Process images through vision-capable models
//! - **CoordinateMapper**: Map semantic descriptions to screen coordinates
//! - **ComputerController**: Safe mouse/keyboard control through verification loop
//!
//! # System Dependencies
//!
//! Full vision support requires:
//! - libxcb and xrandr on Linux (X11/Wayland)
//! - CoreGraphics on macOS
//! - Win32 GDI on Windows

pub mod capture;
pub mod vision;
pub mod coordinate;
pub mod control;
pub mod observe_act_verify;

pub use capture::{ScreenCapture, CaptureConfig, CaptureResult, CaptureRegion, CaptureError};
pub use vision::{VisionModel, VisionConfig, VisionResult, VisionError, ElementDescription};
pub use coordinate::{CoordinateMapper, CoordinateConfig, CoordinateError, ScreenCoordinate};
pub use control::{ComputerController, ControlConfig, ControlAction, ControlResult, ControlError};
pub use observe_act_verify::{ObserveActVerifyLoop, OAVConfig, OAVState, OAVError, VerificationResult};

/// Version of the vision crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if vision support is available.
pub fn is_vision_available() -> bool {
    cfg!(feature = "vision")
}

/// Check if screen capture is available.
pub fn is_capture_available() -> bool {
    capture::CaptureBackend::is_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_capture_availability() {
        // Should return false or true based on platform
        let _available = is_capture_available();
    }
}
