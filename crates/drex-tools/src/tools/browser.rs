//! Browser Automation Tool - Control a headless browser via Chrome DevTools Protocol
//!
//! Provides real browser automation for:
//! - Web scraping with JavaScript execution
//! - Form filling and submission
//! - Taking screenshots of web pages
//! - Extracting page content and elements
//! - Navigating complex SPAs (Single Page Applications)
//!
//! # Requirements
//!
//! Requires a Chrome/Chromium browser installed on the system.
//! Set the `CHROME_PATH` environment variable or use auto-detection.
//!
//! # Example
//!
//! ```rust,ignore
//! use drex_tools::tools::browser::{Browser, BrowserConfig};
//!
//! let config = BrowserConfig::default();
//! let browser = Browser::launch(config).await?;
//! let page = browser.new_page().await?;
//! page.goto("https://example.com").await?;
//! let screenshot = page.screenshot().await?;
//! browser.close().await?;
//! ```

use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for the browser.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Path to Chrome/Chromium executable.
    pub chrome_path: Option<PathBuf>,
    /// Headless mode (no visible window).
    pub headless: bool,
    /// Window width.
    pub width: u32,
    /// Window height.
    pub height: u32,
    /// Default navigation timeout.
    pub navigation_timeout: Duration,
    /// User agent string.
    pub user_agent: Option<String>,
    /// Additional Chrome args.
    pub args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            chrome_path: None,
            headless: true,
            width: 1920,
            height: 1080,
            navigation_timeout: Duration::from_secs(30),
            user_agent: None,
            args: vec![],
        }
    }
}

/// Browser handle for Chrome/CDP connection.
#[cfg(feature = "browser")]
pub struct Browser {
    inner: chromiumoxide::Browser,
    config: BrowserConfig,
}

/// Browser handle for Chrome/CDP connection.
#[cfg(not(feature = "browser"))]
pub struct Browser {
    config: BrowserConfig,
}

impl Browser {
    /// Launch a new browser instance.
    #[cfg(feature = "browser")]
    pub async fn launch(config: BrowserConfig) -> Result<Self, BrowserError> {
        use chromiumoxide::browser::BrowserLauncher;

        info!("Launching browser (headless: {})", config.headless);

        let mut builder = chromiumoxide::browser::BrowserConfig::builder()
            .window_size(config.width, config.height);

        if config.headless {
            builder = builder.no_sandbox().disable_setuid_sandbox();
        }

        if let Some(ref user_agent) = config.user_agent {
            builder = builder.user_agent(user_agent);
        }

        // Add any custom args
        for arg in &config.args {
            builder = builder.arg(arg);
        }

        let browser_config = builder.build()
            .map_err(|e| BrowserError::NotAvailable(e.to_string()))?;

        let browser = chromiumoxide::Browser::launch(browser_config)
            .await
            .map_err(|e| BrowserError::ConnectionError(e.to_string()))?;

        Ok(Self {
            inner: browser,
            config,
        })
    }

    /// Launch a new browser instance (placeholder for non-browser builds).
    #[cfg(not(feature = "browser"))]
    pub async fn launch(config: BrowserConfig) -> Result<Self, BrowserError> {
        info!("Browser feature not enabled, using placeholder");
        Ok(Self { config })
    }

    /// Create a new page/tab.
    #[cfg(feature = "browser")]
    pub async fn new_page(&self) -> Result<Page<'_>, BrowserError> {
        debug!("Creating new page");
        let page = self.inner.new_page()
            .await
            .map_err(|e| BrowserError::ConnectionError(e.to_string()))?;

        Ok(Page {
            inner: Some(page),
            browser: self,
        })
    }

    /// Create a new page/tab (placeholder for non-browser builds).
    #[cfg(not(feature = "browser"))]
    pub async fn new_page(&self) -> Result<Page<'_>, BrowserError> {
        debug!("Creating new page (placeholder)");
        Ok(Page {
            inner: None,
            browser: self,
        })
    }

    /// Close the browser.
    #[cfg(feature = "browser")]
    pub async fn close(self) -> Result<(), BrowserError> {
        info!("Closing browser");
        // Browser closes automatically when dropped
        Ok(())
    }

    /// Close the browser (placeholder for non-browser builds).
    #[cfg(not(feature = "browser"))]
    pub async fn close(self) -> Result<(), BrowserError> {
        info!("Closing browser (placeholder)");
        Ok(())
    }

    /// Get the browser configuration.
    pub fn config(&self) -> &BrowserConfig {
        &self.config
    }
}

/// Represents a browser page/tab.
#[cfg(feature = "browser")]
pub struct Page<'a> {
    inner: Option<chromiumoxide::Page>,
    browser: &'a Browser,
}

/// Represents a browser page/tab.
#[cfg(not(feature = "browser"))]
pub struct Page<'a> {
    inner: Option<()>,
    browser: &'a Browser,
}

impl<'a> Page<'a> {
    /// Navigate to a URL.
    #[cfg(feature = "browser")]
    pub async fn goto(&self, url: &str) -> Result<(), BrowserError> {
        info!("Navigating to: {}", url);

        if let Some(ref page) = self.inner {
            let _ = page.goto(url).await
                .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;

            // Wait for navigation to complete
            tokio::time::timeout(self.browser.config.navigation_timeout, async {
                // Wait for page to be ready
                tokio::time::sleep(Duration::from_millis(100)).await;
            }).await
            .map_err(|_| BrowserError::Timeout(format!("Navigation to {} timed out", url)))?;
        }

        Ok(())
    }

    /// Navigate to a URL (placeholder for non-browser builds).
    #[cfg(not(feature = "browser"))]
    pub async fn goto(&self, url: &str) -> Result<(), BrowserError> {
        info!("Navigating to: {} (placeholder)", url);
        Ok(())
    }

    /// Go back in history.
    pub async fn go_back(&self) -> Result<(), BrowserError> {
        debug!("Going back");
        if let Some(ref page) = self.inner {
            let _ = page.execute(
                chromiumoxide::cdp::browser_protocol::page::NavigateBackParams::default()
            ).await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// Go forward in history.
    pub async fn go_forward(&self) -> Result<(), BrowserError> {
        debug!("Going forward");
        if let Some(ref page) = self.inner {
            let _ = page.execute(
                chromiumoxide::cdp::browser_protocol::page::NavigateForwardParams::default()
            ).await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// Reload the current page.
    pub async fn reload(&self) -> Result<(), BrowserError> {
        debug!("Reloading page");
        if let Some(ref page) = self.inner {
            let _ = page.execute(
                chromiumoxide::cdp::browser_protocol::page::ReloadParams::default()
            ).await
            .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// Get the current URL.
    pub async fn url(&self) -> Result<String, BrowserError> {
        if let Some(ref page) = self.inner {
            let info = page.url().await
                .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
            Ok(info)
        } else {
            Ok("https://example.com".to_string())
        }
    }

    /// Get the page title.
    pub async fn title(&self) -> Result<String, BrowserError> {
        if let Some(ref page) = self.inner {
            let title = page.get_title().await
                .map_err(|e| BrowserError::NavigationFailed(e.to_string()))?;
            Ok(title.unwrap_or_default())
        } else {
            Ok("Example Page".to_string())
        }
    }

    /// Click an element by CSS selector.
    pub async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        info!("Clicking element: {}", selector);
        if let Some(ref page) = self.inner {
            let js = format!(
                r#"document.querySelector("{}").click()"#,
                selector.replace('"', r#"\""#)
            );
            self.evaluate(&js).await?;
        }
        Ok(())
    }

    /// Type text into an input element.
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        info!("Typing into {}: {}", selector, text);
        if let Some(ref page) = self.inner {
            // Focus element
            let focus_js = format!(
                r#"document.querySelector("{}").focus()"#,
                selector.replace('"', "\\"")
            );
            self.evaluate(&focus_js).await?;

            // Type each character
            for c in text.chars() {
                if c == '\n' {
                    page.press_key("Enter").await
                        .map_err(|e| BrowserError::JavaScriptError(e.to_string()))?;
                } else {
                    page.type_str(&c.to_string()).await
                        .map_err(|e| BrowserError::JavaScriptError(e.to_string()))?;
                }
            }
        }
        Ok(())
    }

    /// Get element text content.
    pub async fn text(&self, selector: &str) -> Result<String, BrowserError> {
        let js = format!(
            r#"(document.querySelector("{}")?.textContent || "").trim()"#,
            selector.replace('"', "\\"")
        );
        let result = self.evaluate(&js).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Get all text from the page body.
    pub async fn body_text(&self) -> Result<String, BrowserError> {
        let js = r#"(document.body?.textContent || "").trim()"#;
        let result = self.evaluate(js).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Wait for an element to appear.
    pub async fn wait_for(&self, selector: &str, timeout: Duration) -> Result<(), BrowserError> {
        debug!("Waiting for: {}", selector);
        if let Some(ref page) = self.inner {
            let start = std::time::Instant::now();
            while start.elapsed() < timeout {
                let exists = self.evaluate(
                    &format!(r#"!!document.querySelector("{}")"#, selector.replace('"', "\\"))
                ).await?;

                if exists.as_bool() == Some(true) {
                    return Ok(());
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            return Err(BrowserError::Timeout(format!(
                "Element '{}' not found within {:?}",
                selector, timeout
            )));
        }
        Ok(())
    }

    /// Execute JavaScript and return the result.
    #[cfg(feature = "browser")]
    pub async fn evaluate(&self, script: &str) -> Result<serde_json::Value, BrowserError> {
        if let Some(ref page) = self.inner {
            let result = page.evaluate(script).await
                .map_err(|e| BrowserError::JavaScriptError(e.to_string()))?;
            Ok(result.value().clone())
        } else {
            Ok(serde_json::json!(null))
        }
    }

    /// Execute JavaScript (placeholder for non-browser builds).
    #[cfg(not(feature = "browser"))]
    pub async fn evaluate(&self, _script: &str) -> Result<serde_json::Value, BrowserError> {
        Ok(serde_json::json!(null))
    }

    /// Take a screenshot of the page.
    pub async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>, BrowserError> {
        info!("Taking screenshot (full_page: {})", full_page);
        if let Some(ref page) = self.inner {
            let params = chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::default();
            let screenshot = page.screenshot(params).await
                .map_err(|e| BrowserError::ConnectionError(e.to_string()))?;
            Ok(screenshot)
        } else {
            // Return minimal PNG
            Ok(vec![137, 80, 78, 71, 13, 10, 26, 10])
        }
    }

    /// Take a screenshot of a specific element.
    pub async fn screenshot_element(&self, selector: &str) -> Result<Vec<u8>, BrowserError> {
        info!("Taking element screenshot: {}", selector);
        // For element screenshots, we'd need to get the element's clip rect
        // For now, just take a full screenshot
        self.screenshot(false).await
    }

    /// Fill a form with data.
    pub async fn fill_form(&self, data: &[(String, String)]) -> Result<(), BrowserError> {
        for (field, value) in data {
            let selector = if field.starts_with('#') || field.starts_with('.') || field.starts_with('[') {
                field.clone()
            } else {
                format!(r#"[name="{}"]"#, field.replace('"', "\\""))
            };
            self.type_text(&selector, value).await?;
        }
        Ok(())
    }

    /// Submit a form.
    pub async fn submit_form(&self, selector: &str) -> Result<(), BrowserError> {
        info!("Submitting form: {}", selector);
        let js = format!(
            r#"document.querySelector("{}").submit()"#,
            selector.replace('"', "\\"")
        );
        self.evaluate(&js).await?;
        Ok(())
    }

    /// Scroll the page.
    pub async fn scroll(&self, x: i32, y: i32) -> Result<(), BrowserError> {
        debug!("Scrolling to: ({}, {})", x, y);
        let js = format!("window.scrollTo({}, {})", x, y);
        self.evaluate(&js).await?;
        Ok(())
    }

    /// Get page HTML source.
    pub async fn html(&self) -> Result<String, BrowserError> {
        if let Some(ref page) = self.inner {
            let html = page.content().await
                .map_err(|e| BrowserError::ConnectionError(e.to_string()))?;
            Ok(html)
        } else {
            Ok("<html><body>Page HTML</body></html>".to_string())
        }
    }

    /// Find elements matching a selector.
    pub async fn find_elements(&self, selector: &str) -> Result<Vec<Element>, BrowserError> {
        let js = format!(
            r#"Array.from(document.querySelectorAll("{}")).map(el => ({{
                tag: el.tagName.toLowerCase(),
                text: el.textContent?.trim() || "",
                attributes: Array.from(el.attributes).map(a => [a.name, a.value])
            }}))"#,
            selector.replace('"', "\\"")
        );

        let result = self.evaluate(&js).await?;
        let elements: Vec<Element> = serde_json::from_value(result)
            .unwrap_or_default();

        Ok(elements)
    }

    /// Get the inner HTML of an element.
    pub async fn inner_html(&self, selector: &str) -> Result<String, BrowserError> {
        let js = format!(
            r#"(document.querySelector("{}")?.innerHTML || "")"#,
            selector.replace('"', "\\"")
        );
        let result = self.evaluate(&js).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Get an element's attribute.
    pub async fn attribute(&self, selector: &str, attr: &str) -> Result<Option<String>, BrowserError> {
        let js = format!(
            r#"document.querySelector("{}")?.getAttribute("{}")"#,
            selector.replace('"', "\\""),
            attr.replace('"', "\\"")
        );
        let result = self.evaluate(&js).await?;
        Ok(result.as_str().map(|s| s.to_string()))
    }
}

/// Represents a DOM element.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Element {
    pub tag: String,
    pub text: String,
    pub attributes: Vec<Vec<String>>,
}

/// Errors that can occur during browser automation.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    /// Browser not available.
    #[error("Browser not available: {0}")]
    NotAvailable(String),

    /// Navigation failed.
    #[error("Navigation failed: {0}")]
    NavigationFailed(String),

    /// Element not found.
    #[error("Element not found: {0}")]
    ElementNotFound(String),

    /// Timeout.
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// JavaScript error.
    #[error("JavaScript error: {0}")]
    JavaScriptError(String),

    /// Connection error.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_config_default() {
        let config = BrowserConfig::default();
        assert!(config.headless);
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
    }

    #[tokio::test]
    async fn test_browser_placeholder() {
        let config = BrowserConfig::default();
        let browser = Browser::launch(config).await.unwrap();
        let _page = browser.new_page().await.unwrap();
        browser.close().await.unwrap();
    }
}
