//! Computer Control - Safe mouse and keyboard interaction
//!
//! Provides abstraction for:
//! - Mouse movement and clicks
//! - Keyboard input (typing, shortcuts)
//! - Scrolling
//! - Drag and drop
//!
//! All actions are logged and subject to user confirmation
//! when used in the Observe-Act-Verify loop.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::coordinate::{ScreenCoordinate, ScreenRegion};

/// Keys that can be pressed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Key {
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    // Numbers
    N0, N1, N2, N3, N4, N5, N6, N7, N8, N9,
    // Special
    Return, Escape, Tab, Space, Backspace, Delete,
    // Navigation
    Up, Down, Left, Right, Home, End, PageUp, PageDown,
    // Modifiers
    Shift, Control, Alt, Command,
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

/// Mouse button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Scroll direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Control action to perform.
#[derive(Debug, Clone)]
pub enum ControlAction {
    /// Move mouse to coordinates.
    MoveTo { x: i32, y: i32 },
    /// Click at current position.
    Click { button: MouseButton },
    /// Double click.
    DoubleClick { button: MouseButton },
    /// Click at specific coordinates.
    ClickAt { x: i32, y: i32, button: MouseButton },
    /// Type text.
    Type { text: String },
    /// Press a key.
    KeyPress { key: Key, modifiers: Vec<Key> },
    /// Scroll.
    Scroll { direction: ScrollDirection, amount: u32 },
    /// Drag from one point to another.
    Drag { from: ScreenCoordinate, to: ScreenCoordinate },
    /// Wait for duration.
    Wait { duration_ms: u64 },
}

impl ControlAction {
    /// Get human-readable description of the action.
    pub fn description(&self) -> String {
        match self {
            ControlAction::MoveTo { x, y } => format!("move mouse to ({}, {})", x, y),
            ControlAction::Click { button } => format!("{} click", format_button(button)),
            ControlAction::DoubleClick { button } => format!("double {} click", format_button(button)),
            ControlAction::ClickAt { x, y, button } => format!("{} click at ({}, {})", format_button(button), x, y),
            ControlAction::Type { text } => format!("type '{}'", text.chars().take(50).collect::<String>()),
            ControlAction::KeyPress { key, modifiers } => {
                if modifiers.is_empty() {
                    format!("press {:?}", key)
                } else {
                    format!("press {}+{:?}", modifiers.iter().map(|k| format!("{:?}", k)).collect::<Vec<_>>().join("+"), key)
                }
            }
            ControlAction::Scroll { direction, amount } => format!("scroll {} {} units", format_direction(direction), amount),
            ControlAction::Drag { from, to } => format!("drag from ({}, {}) to ({}, {})", from.x, from.y, to.x, to.y),
            ControlAction::Wait { duration_ms } => format!("wait {}ms", duration_ms),
        }
    }
}

fn format_button(button: &MouseButton) -> &str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

fn format_direction(dir: &ScrollDirection) -> &str {
    match dir {
        ScrollDirection::Up => "up",
        ScrollDirection::Down => "down",
        ScrollDirection::Left => "left",
        ScrollDirection::Right => "right",
    }
}

/// Control configuration.
#[derive(Debug, Clone)]
pub struct ControlConfig {
    /// Delay between actions (ms).
    pub action_delay_ms: u64,
    /// Mouse movement duration (ms).
    pub move_duration_ms: u64,
    /// Default click duration (ms).
    pub click_duration_ms: u64,
    /// Type delay per character (ms).
    pub type_delay_ms: u64,
    /// Screen width.
    pub screen_width: i32,
    /// Screen height.
    pub screen_height: i32,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            action_delay_ms: 50,
            move_duration_ms: 300,
            click_duration_ms: 100,
            type_delay_ms: 10,
            screen_width: 1920,
            screen_height: 1080,
        }
    }
}

/// Result of a control action.
#[derive(Debug, Clone)]
pub struct ControlResult {
    /// The action that was performed.
    pub action: ControlAction,
    /// Whether it succeeded.
    pub success: bool,
    /// Duration of the action.
    pub duration_ms: u64,
    /// Any error message.
    pub error: Option<String>,
}

/// Errors that can occur during control operations.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// Control backend not available.
    #[error("Control backend not available: {0}")]
    NotAvailable(String),

    /// Invalid coordinates.
    #[error("Invalid coordinates: ({0}, {1})")]
    InvalidCoordinates(i32, i32),

    /// Coordinates outside screen bounds.
    #[error("Coordinates ({0}, {1}) outside screen bounds")]
    OutOfBounds(i32, i32),

    /// Action failed.
    #[error("Action failed: {0}")]
    ActionFailed(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Computer controller trait.
#[async_trait::async_trait]
pub trait ComputerController: Send + Sync {
    /// Execute a single action.
    async fn execute(&self, action: ControlAction) -> Result<ControlResult, ControlError>;

    /// Execute multiple actions.
    async fn execute_sequence(&self, actions: Vec<ControlAction>) -> Result<Vec<ControlResult>, ControlError>;

    /// Get current mouse position.
    async fn get_mouse_position(&self) -> Result<ScreenCoordinate, ControlError>;

    /// Get screen dimensions.
    fn screen_dimensions(&self) -> (i32, i32);

    /// Check if available.
    fn is_available(&self) -> bool;
}

/// Placeholder computer controller.
pub struct PlaceholderComputerController {
    config: ControlConfig,
    mouse_position: std::sync::Mutex<ScreenCoordinate>,
}

impl PlaceholderComputerController {
    /// Create a new placeholder controller.
    pub fn new(config: ControlConfig) -> Result<Self, ControlError> {
        info!("Creating placeholder computer controller");
        Ok(Self {
            config: config.clone(),
            mouse_position: std::sync::Mutex::new(ScreenCoordinate::new(0, 0)),
        })
    }

    /// Check if coordinates are valid.
    fn check_coordinates(&self, x: i32, y: i32) -> Result<(), ControlError> {
        if x < 0 || y < 0 {
            return Err(ControlError::InvalidCoordinates(x, y));
        }
        if x >= self.config.screen_width || y >= self.config.screen_height {
            return Err(ControlError::OutOfBounds(x, y));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl ComputerController for PlaceholderComputerController {
    async fn execute(&self, action: ControlAction) -> Result<ControlResult, ControlError> {
        let start = Instant::now();

        debug!("Executing: {}", action.description());

        match &action {
            ControlAction::MoveTo { x, y } => {
                self.check_coordinates(*x, *y)?;
                {
                    let mut pos = self.mouse_position.lock().unwrap();
                    *pos = ScreenCoordinate::new(*x, *y);
                }
                tokio::time::sleep(Duration::from_millis(self.config.move_duration_ms)).await;
            }
            ControlAction::Click { .. } => {
                tokio::time::sleep(Duration::from_millis(self.config.click_duration_ms)).await;
            }
            ControlAction::DoubleClick { .. } => {
                tokio::time::sleep(Duration::from_millis(self.config.click_duration_ms * 2)).await;
            }
            ControlAction::ClickAt { x, y, .. } => {
                self.check_coordinates(*x, *y)?;
                {
                    let mut pos = self.mouse_position.lock().unwrap();
                    *pos = ScreenCoordinate::new(*x, *y);
                }
                tokio::time::sleep(Duration::from_millis(self.config.click_duration_ms)).await;
            }
            ControlAction::Type { text } => {
                let delay = self.config.type_delay_ms * text.len() as u64;
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            ControlAction::KeyPress { .. } => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            ControlAction::Scroll { amount, .. } => {
                tokio::time::sleep(Duration::from_millis(20 * *amount as u64)).await;
            }
            ControlAction::Drag { from, to } => {
                self.check_coordinates(from.x, from.y)?;
                self.check_coordinates(to.x, to.y)?;
                {
                    let mut pos = self.mouse_position.lock().unwrap();
                    *pos = *to;
                }
                tokio::time::sleep(Duration::from_millis(self.config.move_duration_ms * 2)).await;
            }
            ControlAction::Wait { duration_ms } => {
                tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(self.config.action_delay_ms)).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ControlResult {
            action,
            success: true,
            duration_ms,
            error: None,
        })
    }

    async fn execute_sequence(&self, actions: Vec<ControlAction>) -> Result<Vec<ControlResult>, ControlError> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.execute(action).await?);
        }
        Ok(results)
    }

    async fn get_mouse_position(&self) -> Result<ScreenCoordinate, ControlError> {
        let pos = self.mouse_position.lock().unwrap();
        Ok(*pos)
    }

    fn screen_dimensions(&self) -> (i32, i32) {
        (self.config.screen_width, self.config.screen_height)
    }

    fn is_available(&self) -> bool {
        false // Placeholder not actually available
    }
}

/// Type alias for computer controller.
pub type BoxedController = Arc<dyn ComputerController>;

/// Create a default controller.
pub fn create_controller(config: ControlConfig) -> Result<BoxedController, ControlError> {
    let controller = PlaceholderComputerController::new(config)?;
    Ok(Arc::new(controller))
}

/// Build a click action at coordinates.
pub fn click_at(x: i32, y: i32) -> ControlAction {
    ControlAction::ClickAt { x, y, button: MouseButton::Left }
}

/// Build a type action.
pub fn type_text(text: impl Into<String>) -> ControlAction {
    ControlAction::Type { text: text.into() }
}

/// Build a wait action.
pub fn wait(ms: u64) -> ControlAction {
    ControlAction::Wait { duration_ms: ms }
}

/// Build a keyboard shortcut.
pub fn shortcut(key: Key, modifiers: Vec<Key>) -> ControlAction {
    ControlAction::KeyPress { key, modifiers }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_action_description() {
        assert!(ControlAction::MoveTo { x: 100, y: 200 }.description().contains("move mouse"));
        assert!(ControlAction::Click { button: MouseButton::Left }.description().contains("left click"));
        assert!(ControlAction::Type { text: "hello".to_string() }.description().contains("type"));
    }

    #[tokio::test]
    async fn test_controller_execute_move() {
        let config = ControlConfig::default();
        let controller = PlaceholderComputerController::new(config).unwrap();

        let result = controller.execute(ControlAction::MoveTo { x: 100, y: 200 }).await.unwrap();
        assert!(result.success);
        assert!(result.duration_ms > 0);
    }

    #[tokio::test]
    async fn test_controller_execute_click() {
        let config = ControlConfig::default();
        let controller = PlaceholderComputerController::new(config).unwrap();

        let result = controller.execute(ControlAction::Click { button: MouseButton::Left }).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_controller_type() {
        let config = ControlConfig::default();
        let controller = PlaceholderComputerController::new(config).unwrap();

        let result = controller.execute(ControlAction::Type { text: "hello world".to_string() }).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_controller_sequence() {
        let config = ControlConfig::default();
        let controller = PlaceholderComputerController::new(config).unwrap();

        let actions = vec![
            ControlAction::MoveTo { x: 100, y: 100 },
            ControlAction::Click { button: MouseButton::Left },
            ControlAction::Type { text: "test".to_string() },
        ];

        let results = controller.execute_sequence(actions).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn test_controller_invalid_coordinates() {
        let config = ControlConfig::default();
        let controller = PlaceholderComputerController::new(config).unwrap();

        let result = controller.execute(ControlAction::MoveTo { x: 2000, y: 1200 }).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_click_at_helper() {
        let action = click_at(100, 200);
        match action {
            ControlAction::ClickAt { x, y, button } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
                assert_eq!(button, MouseButton::Left);
            }
            _ => panic!("Expected ClickAt"),
        }
    }

    #[test]
    fn test_type_text_helper() {
        let action = type_text("hello");
        match action {
            ControlAction::Type { text } => assert_eq!(text, "hello"),
            _ => panic!("Expected Type"),
        }
    }

    #[test]
    fn test_wait_helper() {
        let action = wait(500);
        match action {
            ControlAction::Wait { duration_ms } => assert_eq!(duration_ms, 500),
            _ => panic!("Expected Wait"),
        }
    }

    #[test]
    fn test_shortcut_helper() {
        let action = shortcut(Key::C, vec![Key::Control]);
        match action {
            ControlAction::KeyPress { key, modifiers } => {
                assert_eq!(key, Key::C);
                assert_eq!(modifiers, vec![Key::Control]);
            }
            _ => panic!("Expected KeyPress"),
        }
    }

    #[test]
    fn test_create_controller() {
        let config = ControlConfig::default();
        let controller = create_controller(config).unwrap();
        assert!(!controller.is_available());
    }
}
