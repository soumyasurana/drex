//! Linux Computer Control - Real mouse/keyboard using enigo
//!
//! Provides actual computer control for Linux using the enigo crate.
//! Uses std::sync::Mutex for interior mutability.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::debug;

use enigo::{Enigo, Settings, Keyboard, Mouse, Coordinate, Direction, Button, Axis, Key as EnigoKey};

use crate::control::{ComputerController, ControlAction, ControlConfig, ControlError, ControlResult, Key, MouseButton, ScrollDirection};
use crate::coordinate::ScreenCoordinate;

/// Enigo-based computer controller for Linux.
pub struct EnigoController {
    enigo: Arc<std::sync::Mutex<Enigo>>,
    config: ControlConfig,
}

impl EnigoController {
    /// Create a new enigo controller.
    pub fn new(config: ControlConfig) -> Result<Self, ControlError> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| ControlError::NotAvailable(format!("Failed to initialize enigo: {}", e)))?;
        Ok(Self { 
            enigo: Arc::new(std::sync::Mutex::new(enigo)),
            config 
        })
    }

    /// Check if the controller is available.
    pub fn is_available() -> bool {
        Enigo::new(&Settings::default()).is_ok()
    }

    fn to_enigo_key(key: &Key) -> EnigoKey {
        match key {
            Key::A => EnigoKey::Unicode('a'),
            Key::B => EnigoKey::Unicode('b'),
            Key::C => EnigoKey::Unicode('c'),
            Key::D => EnigoKey::Unicode('d'),
            Key::E => EnigoKey::Unicode('e'),
            Key::F => EnigoKey::Unicode('f'),
            Key::G => EnigoKey::Unicode('g'),
            Key::H => EnigoKey::Unicode('h'),
            Key::I => EnigoKey::Unicode('i'),
            Key::J => EnigoKey::Unicode('j'),
            Key::K => EnigoKey::Unicode('k'),
            Key::L => EnigoKey::Unicode('l'),
            Key::M => EnigoKey::Unicode('m'),
            Key::N => EnigoKey::Unicode('n'),
            Key::O => EnigoKey::Unicode('o'),
            Key::P => EnigoKey::Unicode('p'),
            Key::Q => EnigoKey::Unicode('q'),
            Key::R => EnigoKey::Unicode('r'),
            Key::S => EnigoKey::Unicode('s'),
            Key::T => EnigoKey::Unicode('t'),
            Key::U => EnigoKey::Unicode('u'),
            Key::V => EnigoKey::Unicode('v'),
            Key::W => EnigoKey::Unicode('w'),
            Key::X => EnigoKey::Unicode('x'),
            Key::Y => EnigoKey::Unicode('y'),
            Key::Z => EnigoKey::Unicode('z'),
            Key::N0 => EnigoKey::Unicode('0'),
            Key::N1 => EnigoKey::Unicode('1'),
            Key::N2 => EnigoKey::Unicode('2'),
            Key::N3 => EnigoKey::Unicode('3'),
            Key::N4 => EnigoKey::Unicode('4'),
            Key::N5 => EnigoKey::Unicode('5'),
            Key::N6 => EnigoKey::Unicode('6'),
            Key::N7 => EnigoKey::Unicode('7'),
            Key::N8 => EnigoKey::Unicode('8'),
            Key::N9 => EnigoKey::Unicode('9'),
            Key::Return => EnigoKey::Return,
            Key::Escape => EnigoKey::Escape,
            Key::Tab => EnigoKey::Tab,
            Key::Space => EnigoKey::Space,
            Key::Backspace => EnigoKey::Backspace,
            Key::Delete => EnigoKey::Delete,
            Key::Up => EnigoKey::UpArrow,
            Key::Down => EnigoKey::DownArrow,
            Key::Left => EnigoKey::LeftArrow,
            Key::Right => EnigoKey::RightArrow,
            Key::Home => EnigoKey::Home,
            Key::End => EnigoKey::End,
            Key::PageUp => EnigoKey::PageUp,
            Key::PageDown => EnigoKey::PageDown,
            Key::Shift => EnigoKey::Shift,
            Key::Control => EnigoKey::Control,
            Key::Alt => EnigoKey::Alt,
            Key::Command => EnigoKey::Meta,
            Key::F1 => EnigoKey::F1,
            Key::F2 => EnigoKey::F2,
            Key::F3 => EnigoKey::F3,
            Key::F4 => EnigoKey::F4,
            Key::F5 => EnigoKey::F5,
            Key::F6 => EnigoKey::F6,
            Key::F7 => EnigoKey::F7,
            Key::F8 => EnigoKey::F8,
            Key::F9 => EnigoKey::F9,
            Key::F10 => EnigoKey::F10,
            Key::F11 => EnigoKey::F11,
            Key::F12 => EnigoKey::F12,
        }
    }

    fn to_enigo_button(button: &MouseButton) -> Button {
        match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        }
    }
}

#[async_trait::async_trait]
impl ComputerController for EnigoController {
    async fn execute(&self, action: ControlAction) -> Result<ControlResult, ControlError> {
        let start = Instant::now();
        debug!("Executing: {}", action.description());

        // Clone for spawn_blocking
        let enigo = self.enigo.clone();
        let config = self.config.clone();
        let action_clone = action.clone();

        let result = tokio::task::spawn_blocking(move || {
            let mut enigo = enigo.lock().unwrap();
            
            match &action_clone {
                ControlAction::MoveTo { x, y } => {
                    if *x < 0 || *y < 0 {
                        return Err(ControlError::InvalidCoordinates(*x, *y));
                    }
                    enigo.move_mouse(*x, *y, Coordinate::Abs)
                        .map_err(|e| ControlError::ActionFailed(format!("Mouse move failed: {}", e)))?;
                }
                ControlAction::Click { button } => {
                    let btn = Self::to_enigo_button(button);
                    enigo.button(btn, Direction::Click)
                        .map_err(|e| ControlError::ActionFailed(format!("Click failed: {}", e)))?;
                }
                ControlAction::DoubleClick { button } => {
                    let btn = Self::to_enigo_button(button);
                    enigo.button(btn, Direction::Click)
                        .map_err(|e| ControlError::ActionFailed(format!("Double click failed: {}", e)))?;
                    std::thread::sleep(Duration::from_millis(50));
                    enigo.button(btn, Direction::Click)
                        .map_err(|e| ControlError::ActionFailed(format!("Double click failed: {}", e)))?;
                }
                ControlAction::ClickAt { x, y, button } => {
                    if *x < 0 || *y < 0 {
                        return Err(ControlError::InvalidCoordinates(*x, *y));
                    }
                    enigo.move_mouse(*x, *y, Coordinate::Abs)
                        .map_err(|e| ControlError::ActionFailed(format!("Mouse move failed: {}", e)))?;
                    std::thread::sleep(Duration::from_millis(50));
                    let btn = Self::to_enigo_button(button);
                    enigo.button(btn, Direction::Click)
                        .map_err(|e| ControlError::ActionFailed(format!("Click failed: {}", e)))?;
                }
                ControlAction::Type { text } => {
                    for ch in text.chars() {
                        enigo.key(EnigoKey::Unicode(ch), Direction::Click)
                            .map_err(|e| ControlError::ActionFailed(format!("Type failed: {}", e)))?;
                        std::thread::sleep(Duration::from_millis(config.type_delay_ms));
                    }
                }
                ControlAction::KeyPress { key, modifiers } => {
                    for mod_key in modifiers {
                        enigo.key(Self::to_enigo_key(mod_key), Direction::Press)
                            .map_err(|e| ControlError::ActionFailed(format!("Key press failed: {}", e)))?;
                    }
                    enigo.key(Self::to_enigo_key(key), Direction::Click)
                        .map_err(|e| ControlError::ActionFailed(format!("Key press failed: {}", e)))?;
                    for mod_key in modifiers {
                        enigo.key(Self::to_enigo_key(mod_key), Direction::Release)
                            .map_err(|e| ControlError::ActionFailed(format!("Key release failed: {}", e)))?;
                    }
                }
                ControlAction::Scroll { direction, amount } => {
                    for _ in 0..*amount {
                        match direction {
                            ScrollDirection::Up => {
                                enigo.scroll(1, Axis::Vertical)
                                    .map_err(|e| ControlError::ActionFailed(format!("Scroll failed: {}", e)))?;
                            }
                            ScrollDirection::Down => {
                                enigo.scroll(-1, Axis::Vertical)
                                    .map_err(|e| ControlError::ActionFailed(format!("Scroll failed: {}", e)))?;
                            }
                            ScrollDirection::Left => {
                                enigo.scroll(-1, Axis::Horizontal)
                                    .map_err(|e| ControlError::ActionFailed(format!("Scroll failed: {}", e)))?;
                            }
                            ScrollDirection::Right => {
                                enigo.scroll(1, Axis::Horizontal)
                                    .map_err(|e| ControlError::ActionFailed(format!("Scroll failed: {}", e)))?;
                            }
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
                ControlAction::Drag { from, to } => {
                    enigo.move_mouse(from.x, from.y, Coordinate::Abs)
                        .map_err(|e| ControlError::ActionFailed(format!("Drag start failed: {}", e)))?;
                    std::thread::sleep(Duration::from_millis(50));
                    enigo.button(Button::Left, Direction::Press)
                        .map_err(|e| ControlError::ActionFailed(format!("Drag press failed: {}", e)))?;
                    std::thread::sleep(Duration::from_millis(50));
                    enigo.move_mouse(to.x, to.y, Coordinate::Abs)
                        .map_err(|e| ControlError::ActionFailed(format!("Drag move failed: {}", e)))?;
                    std::thread::sleep(Duration::from_millis(50));
                    enigo.button(Button::Left, Direction::Release)
                        .map_err(|e| ControlError::ActionFailed(format!("Drag release failed: {}", e)))?;
                }
                ControlAction::Wait { duration_ms } => {
                    std::thread::sleep(Duration::from_millis(*duration_ms));
                }
            }
            Ok::<(), ControlError>(())
        }).await;

        let duration_ms = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(Ok(())) => Ok(ControlResult {
                action,
                success: true,
                duration_ms,
                error: None,
            }),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(ControlError::ActionFailed(format!("Task failed: {}", e))),
        }
    }

    async fn execute_sequence(&self, actions: Vec<ControlAction>) -> Result<Vec<ControlResult>, ControlError> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.execute(action).await?);
        }
        Ok(results)
    }

    async fn get_mouse_position(&self) -> Result<ScreenCoordinate, ControlError> {
        let enigo = self.enigo.clone();
        
        let result = tokio::task::spawn_blocking(move || {
            let mut enigo = enigo.lock().unwrap();
            let (x, y) = enigo.location()
                .map_err(|e| ControlError::ActionFailed(format!("Failed to get position: {}", e)))?;
            Ok::<_, ControlError>(ScreenCoordinate::new(x as i32, y as i32))
        }).await;

        match result {
            Ok(Ok(pos)) => Ok(pos),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(ControlError::ActionFailed(format!("Task failed: {}", e))),
        }
    }

    fn screen_dimensions(&self) -> (i32, i32) {
        (self.config.screen_width, self.config.screen_height)
    }

    fn is_available(&self) -> bool {
        EnigoController::is_available()
    }
}

/// Create a real computer controller.
pub fn create_real_controller(config: ControlConfig) -> Result<std::sync::Arc<dyn ComputerController>, ControlError> {
    let controller = EnigoController::new(config)?;
    Ok(std::sync::Arc::new(controller))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enigo_available() {
        let available = EnigoController::is_available();
        println!("Enigo available: {}", available);
    }
}
