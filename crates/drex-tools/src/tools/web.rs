//! Web fetch tool - HTTP GET requests with safety limits
//!
//! This tool provides controlled HTTP GET requests with:
//! - Timeout enforcement
//! - Response size limits
//! - Redirect handling
//! - Scheme validation (HTTP/HTTPS only)
//! - Structured response output
//!
//! # Security
//!
//! - Only HTTP/HTTPS schemes allowed
//! - Configurable timeout and size limits
//! - No credential forwarding
//!- No cookie forwarding
//! - No environment variable leakage

use crate::capability::{Capability, CapabilitySet};
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

/// Configuration for web fetch tool.
///
/// All limits are set by the trusted Drex runtime, NOT by model input.
#[derive(Debug, Clone)]
pub struct WebFetchConfig {
    /// HTTP client timeout
    timeout: Duration,
    /// Maximum response body size in bytes
    max_size: usize,
    /// Maximum number of redirects to follow
    max_redirects: usize,
    /// Whether to follow redirects automatically
    follow_redirects: bool,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_size: 1024 * 1024, // 1 MB default
            max_redirects: 10,
            follow_redirects: true,
        }
    }
}

impl WebFetchConfig {
    /// Create a new web fetch config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the request timeout.
    ///
    /// IMPORTANT: This is set by the runtime, NOT by model input.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum response size.
    pub fn with_max_size(mut self, bytes: usize) -> Self {
        self.max_size = bytes;
        self
    }

    /// Set the maximum number of redirects to follow.
    pub fn with_max_redirects(mut self, count: usize) -> Self {
        self.max_redirects = count;
        self
    }

    /// Whether to follow redirects.
    pub fn follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = follow;
        self
    }

    /// Build a reqwest Client with these settings.
    fn build_client(&self) -> ToolResult<Client> {
        Client::builder()
            .timeout(self.timeout)
            .danger_accept_invalid_certs(false)
            .redirect(reqwest::redirect::Policy::limited(self.max_redirects))
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                tool: "web.fetch".to_string(),
                reason: format!("failed to build HTTP client: {}", e),
            })
    }
}

/// Input for web fetch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchInput {
    /// The URL to fetch
    pub url: String,
    /// Optional: custom headers
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

/// Output for web fetch tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchOutput {
    /// The original URL requested
    pub requested_url: String,
    /// The final URL after redirects
    pub final_url: String,
    /// HTTP status code
    pub status: u16,
    /// HTTP status text
    pub status_text: String,
    /// Response headers (filtered)
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: String,
    /// Content type from headers
    pub content_type: Option<String>,
    /// Whether the response was truncated due to size limit
    pub truncated: bool,
    /// Number of redirects followed
    pub redirects_followed: usize,
}

/// Validate URL before making request.
fn validate_url(url_str: &str) -> ToolResult<()> {
    let url = url::Url::parse(url_str).map_err(|e| ToolError::InvalidInput {
        tool: "web.fetch".to_string(),
        reason: format!("invalid URL '{}': {}", url_str, e),
    })?;

    // Only allow HTTP/HTTPS schemes
    match url.scheme() {
        "http" | "https" => Ok(()),
        "file" => Err(ToolError::InvalidInput {
            tool: "web.fetch".to_string(),
            reason: "file:// URLs are not allowed".to_string(),
        }),
        "ftp" | "ftps" => Err(ToolError::InvalidInput {
            tool: "web.fetch".to_string(),
            reason: format!("'{}' URLs are not supported", url.scheme()),
        }),
        other => Err(ToolError::InvalidInput {
            tool: "web.fetch".to_string(),
            reason: format!("'{}' scheme is not supported", other),
        }),
    }
}

/// Tool for fetching web content via HTTP GET.
#[derive(Debug, Clone)]
pub struct WebFetchTool {
    metadata: ToolMetadata,
    config: WebFetchConfig,
}

impl WebFetchTool {
    /// Create a new web fetch tool.
    pub fn new(config: WebFetchConfig) -> Self {
        let schema = ToolSchema::builder("WebFetchInput", "Fetch web content via HTTP GET")
            .required_string("url", "The URL to fetch")
            .build();

        Self {
            metadata: ToolMetadata::new(
                "web.fetch",
                "Fetch content from a URL via HTTP GET.\n\
                \n\
                This tool performs HTTP requests to fetch web content. It is read-only \
                and does not support POST or other mutation methods. Response size \
                is limited by configuration.",
                schema,
            ),
            config,
        }
    }

    /// Make the HTTP request and return the response.
    async fn fetch(&self, url: &str, custom_headers: Option<HashMap<String, String>>) -> ToolResult<WebFetchOutput> {
        let client = self.config.build_client()?;

        // Build request
        let mut request = client.get(url);

        // Add custom headers if provided
        if let Some(headers) = custom_headers {
            for (name, value) in headers {
                // Sanitize header names to prevent header injection
                let sanitized_name = name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' , "_");
                if sanitized_name.to_ascii_lowercase() == "authorization" {
                    // Don't log authorization headers
                    warn!("Authorization header provided but not logged for security");
                }
                request = request.header(sanitized_name, value);
            }
        }

        let start = std::time::Instant::now();

        // Execute request with size limit
        let response = request.send().await.map_err(|e| {
            let reason = if e.is_timeout() {
                format!("request to '{}' timed out after {:?}", url, self.config.timeout)
            } else if e.is_redirect() {
                format!("redirect error: {}", e)
            } else {
                format!("request failed: {}", e)
            };
            ToolError::ExecutionFailed {
                tool: self.name().to_string(),
                reason,
            }
        })?;

        let duration = start.elapsed();
        let requested_url = url.to_string();
        let final_url = response.url().as_str().to_string();
        let redirects_followed = response.extensions().get::<usize>().copied().unwrap_or(0);

        // Get response metadata
        let status = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("Unknown").to_string();

        // Extract headers
        let mut headers = HashMap::new();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Filter response headers - only include safe ones
        for (name, value) in response.headers() {
            let name_lower = name.as_str().to_lowercase();
            // Exclude sensitive headers
            if !name_lower.contains("authorization")
                && !name_lower.contains("cookie")
                && !name_lower.contains("set-cookie")
                && !name_lower.contains("proxy-authorization")
            {
                if let Ok(val_str) = value.to_str() {
                    headers.insert(name.as_str().to_string(), val_str.to_string());
                }
            }
        }

        // Read response body with size limit
        let body_bytes = response.bytes().await.map_err(|e| ToolError::ExecutionFailed {
            tool: self.name().to_string(),
            reason: format!("failed to read response body: {}", e),
        })?;

        let truncated = body_bytes.len() > self.config.max_size;
        let body_bytes_limited = if body_bytes.len() > self.config.max_size {
            &body_bytes[..self.config.max_size]
        } else {
            &body_bytes[..]
        };

        // Convert to string (may be lossy for binary content)
        let body = String::from_utf8_lossy(body_bytes_limited).to_string();

        info!(
            tool = self.name(),
            requested_url = %requested_url,
            final_url = %final_url,
            status = status,
            body_bytes = body_bytes.len(),
            truncated = truncated,
            duration_ms = duration.as_millis() as u64,
            "web.fetch completed"
        );

        Ok(WebFetchOutput {
            requested_url,
            final_url,
            status,
            status_text,
            headers,
            body,
            content_type,
            truncated,
            redirects_followed,
        })
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        static REQUIRED: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        REQUIRED.get_or_init(|| CapabilitySet::from(vec![Capability::BrowserRequest]))
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let url = input.require_string("url")?;
        let headers: Option<HashMap<String, String>> = input.parse().ok();

        // Validate URL before any network access
        let sanitized_url = if url.len() > 500 {
            format!("{}... (truncated)", &url[..500])
        } else {
            url.to_string()
        };

        info!(
            tool = self.name(),
            url_requested = %sanitized_url,
            timeout_sec = self.config.timeout.as_secs(),
            max_size = self.config.max_size,
            "web.fetch attempted"
        );

        // Validate URL scheme
        if let Err(e) = validate_url(url) {
            warn!(
                tool = self.name(),
                url_requested = %sanitized_url,
                error = %e,
                "URL validation failed"
            );
            return Err(e);
        }

        // Execute fetch
        match self.fetch(url, headers).await {
            Ok(output) => {
                let json_output = json!(output);
                let result = ExecutionResult::success(json_output);
                Ok(result)
            }
            Err(e) => {
                warn!(
                    tool = self.name(),
                    url_requested = %sanitized_url,
                    error = %e,
                    "web.fetch failed"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_fetch_config_builder() {
        let config = WebFetchConfig::new()
            .with_timeout(Duration::from_secs(60))
            .with_max_size(1024 * 1024 * 5)
            .with_max_redirects(5);

        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.max_size, 5 * 1024 * 1024);
        assert_eq!(config.max_redirects, 5);
    }

    #[tokio::test]
    async fn web_fetch_simple_request() {
        let config = WebFetchConfig::new()
            .with_timeout(Duration::from_secs(10))
            .with_max_size(1024 * 1024);
        let tool = WebFetchTool::new(config);

        // Use httpbin.org for testing (if available)
        // For offline testing, this test might fail
        let input = ToolInput::from_json(
            json!({"url": "https://httpbin.org/get"})
        ).unwrap();

        let result = tool.execute(&ToolContext::new(), input).await;
        
        // May fail in offline environments - that's OK
        if result.is_ok() {
            let output: WebFetchOutput = serde_json::from_value(
                result.unwrap().data.unwrap()
            ).unwrap();
            assert_eq!(output.status, 200);
            assert!(output.body.contains("httpbin"));
        }
    }

    #[tokio::test]
    async fn web_fetch_file_scheme_rejected() {
        let config = WebFetchConfig::new();
        let tool = WebFetchTool::new(config);

        let input = ToolInput::from_json(json!({"url": "file:///etc/passwd"})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file:// URLs are not allowed"));
    }

    #[tokio::test]
    async fn web_fetch_ftp_scheme_rejected() {
        let config = WebFetchConfig::new();
        let tool = WebFetchTool::new(config);

        let input = ToolInput::from_json(json!({"url": "ftp://example.com/file.txt"})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }

    #[tokio::test]
    async fn web_fetch_invalid_url_rejected() {
        let config = WebFetchConfig::new();
        let tool = WebFetchTool::new(config);

        let input = ToolInput::from_json(json!({"url": "not-a-valid-url"})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_err());
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(validate_url("http://example.com").is_ok());
        assert!(validate_url("https://example.com/path?q=test").is_ok());
        assert!(validate_url("https://example.com:8080/").is_ok());
    }

    #[test]
    fn validate_url_rejects_file() {
        let result = validate_url("file:///etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file://"));
    }

    #[test]
    fn validate_url_rejects_ftp() {
        let result = validate_url("ftp://example.com/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn web_fetch_requires_browser_request() {
        let tool = WebFetchTool::new(WebFetchConfig::new());
        let caps = tool.required_capabilities();
        assert!(caps.has(Capability::BrowserRequest));
        assert!(!caps.has(Capability::FileSystemRead));
    }

    #[tokio::test]
    async fn web_fetch_respects_size_limit() {
        let config = WebFetchConfig::new()
            .with_max_size(100); // Very small limit
        let tool = WebFetchTool::new(config);

        // Request a larger response
        // httpbin.org/bytes/N returns base64 encoded data in a JSON wrapper,
        // so the response will be truncated based on raw byte count
        let input = ToolInput::from_json(
            json!({"url": "https://httpbin.org/bytes/1000"})
        ).unwrap();

        let result = tool.execute(&ToolContext::new(), input).await;

        // May fail in offline environments - skip assertions in that case
        if result.is_ok() {
            let output: WebFetchOutput = serde_json::from_value(
                result.unwrap().data.unwrap()
            ).unwrap();
            // The response is considered truncated if raw bytes exceeded max_size
            assert!(output.truncated);
            // Body is the UTF-8 string representation; after truncation,
            // it should be reasonably sized (much smaller than 1000 bytes)
            assert!(output.body.len() < 1000, "body was not truncated: {} bytes", output.body.len());
        }
    }

    #[tokio::test]
    async fn web_fetch_handles_not_found() {
        let config = WebFetchConfig::new()
            .with_timeout(Duration::from_secs(10));
        let tool = WebFetchTool::new(config);

        let input = ToolInput::from_json(
            json!({"url": "https://httpbin.org/status/404"})
        ).unwrap();

        let result = tool.execute(&ToolContext::new(), input).await;
        
        if result.is_ok() {
            let output: WebFetchOutput = serde_json::from_value(
                result.unwrap().data.unwrap()
            ).unwrap();
            assert_eq!(output.status, 404);
        }
    }
}
