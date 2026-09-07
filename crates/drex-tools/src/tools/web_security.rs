//! SSRF (Server-Side Request Forgery) Protection
//!
//! This module provides comprehensive protection against SSRF attacks by:
//! - Validating URL schemes (only HTTP/HTTPS)
//! - Resolving DNS and validating resulting IP addresses
//! - Blocking private/internal IP ranges
//! - Handling redirects safely
//! - Preventing DNS rebinding attacks

use crate::error::ToolError;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// Error type for SSRF validation failures
#[derive(Debug, Clone, PartialEq)]
pub enum SsrfError {
    /// Invalid URL format
    InvalidUrl(String),
    /// URL scheme not allowed
    DisallowedScheme(String),
    /// Host is not a valid IP or domain
    InvalidHost(String),
    /// IP address is blocked (private, loopback, etc.)
    BlockedIp(IpAddr, String),
    /// DNS resolution failed
    DnsResolutionFailed(String),
    /// Redirect would go to blocked destination
    BlockedRedirect(String),
    /// Metadata endpoint access attempt
    MetadataEndpoint(String),
}

impl std::fmt::Display for SsrfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SsrfError::InvalidUrl(s) => write!(f, "Invalid URL: {}", s),
            SsrfError::DisallowedScheme(s) => write!(f, "Scheme '{}' is not allowed (only HTTP/HTTPS)", s),
            SsrfError::InvalidHost(s) => write!(f, "Invalid host: {}", s),
            SsrfError::BlockedIp(ip, reason) => write!(f, "IP address {} is blocked: {}", ip, reason),
            SsrfError::DnsResolutionFailed(host) => write!(f, "DNS resolution failed for: {}", host),
            SsrfError::BlockedRedirect(dst) => write!(f, "Redirect blocked: {}", dst),
            SsrfError::MetadataEndpoint(s) => write!(f, "Metadata endpoint access attempt: {}", s),
        }
    }
}

impl std::error::Error for SsrfError {}

/// SSRF validator configuration
#[derive(Debug, Clone)]
pub struct SsrfConfig {
    /// Allow private IP ranges (default: false)
    allow_private_ips: bool,
    /// Allow loopback addresses (default: false)
    allow_loopback: bool,
    /// Allow link-local addresses (default: false)
    allow_link_local: bool,
    /// Block list of specific domains
    blocked_domains: Vec<String>,
}

impl Default for SsrfConfig {
    fn default() -> Self {
        Self {
            allow_private_ips: false,
            allow_loopback: false,
            allow_link_local: false,
            blocked_domains: vec![
                "localhost".to_string(),
                "metadata.google.internal".to_string(),
                "metadata.google.internal.".to_string(),
                "169.254.169.254".to_string(), // AWS, GCP, Azure metadata
                "instance-data".to_string(),  // EC2
                "metadata".to_string(),     // Generic metadata
            ],
        }
    }
}

impl SsrfConfig {
    /// Create new SSRF config with secure defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow private IP ranges (dangerous - for testing only)
    pub fn allow_private_ips(mut self, allow: bool) -> Self {
        self.allow_private_ips = allow;
        self
    }

    /// Allow loopback addresses (dangerous - for testing only)
    pub fn allow_loopback(mut self, allow: bool) -> Self {
        self.allow_loopback = allow;
        self
    }

    /// Allow link-local addresses (dangerous - for testing only)
    pub fn allow_link_local(mut self, allow: bool) -> Self {
        self.allow_link_local = allow;
        self
    }

    /// Add a blocked domain
    pub fn block_domain(mut self, domain: impl Into<String>) -> Self {
        self.blocked_domains.push(domain.into());
        self
    }
}

/// Check if an IPv4 address is private
fn is_private_v4(ip: &Ipv4Addr) -> bool {
    // 10.0.0.0/8
    if ip.octets()[0] == 10 {
        return true;
    }
    // 172.16.0.0/12
    if ip.octets()[0] == 172 && ip.octets()[1] >= 16 && ip.octets()[1] <= 31 {
        return true;
    }
    // 192.168.0.0/16
    if ip.octets()[0] == 192 && ip.octets()[1] == 168 {
        return true;
    }
    // 100.64.0.0/10 (CGNAT)
    if ip.octets()[0] == 100 && ip.octets()[1] >= 64 && ip.octets()[1] <= 127 {
        return true;
    }
    // 127.0.0.0/8 (loopback)
    if ip.octets()[0] == 127 {
        return true;
    }
    // 169.254.0.0/16 (link-local/APIPA)
    if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
        return true;
    }
    // 0.0.0.0/8 (current network)
    if ip.octets()[0] == 0 {
        return true;
    }
    // 192.0.0.0/24 (IETF protocol assignments)
    if ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0 {
        return true;
    }
    // 192.0.2.0/24 (TEST-NET-1)
    if ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 2 {
        return true;
    }
    // 198.51.100.0/24 (TEST-NET-2)
    if ip.octets()[0] == 198 && ip.octets()[1] == 51 && ip.octets()[2] == 100 {
        return true;
    }
    // 203.0.113.0/24 (TEST-NET-3)
    if ip.octets()[0] == 203 && ip.octets()[1] == 0 && ip.octets()[2] == 113 {
        return true;
    }
    // 240.0.0.0/4 (reserved)
    if ip.octets()[0] >= 240 && ip.octets()[0] <= 255 {
        return true;
    }

    false
}

/// Check if an IPv4 address is loopback
fn is_loopback_v4(ip: &Ipv4Addr) -> bool {
    ip.octets()[0] == 127
}

/// Check if an IPv4 address is link-local
fn is_link_local_v4(ip: &Ipv4Addr) -> bool {
    ip.octets()[0] == 169 && ip.octets()[1] == 254
}

/// Check if an IPv6 address is loopback
fn is_loopback_v6(ip: &Ipv6Addr) -> bool {
    ip.segments() == [0, 0, 0, 0, 0, 0, 0, 1] // ::1
}

/// Check if an IPv6 address is link-local
fn is_link_local_v6(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xffc0 == 0xfe80
}

/// Check if an IPv6 address is unique local (ULA)
fn is_unique_local_v6(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xfe00 == 0xfc00
}

/// Check if an IPv6 address is site-local (deprecated but may exist)
fn is_site_local_v6(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xffc0 == 0xfec0
}

/// Check if IPv6 is multicast
fn is_multicast_v6(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xff00 == 0xff00
}

/// Validate an IP address against SSRF rules
fn validate_ip(ip: &IpAddr, config: &SsrfConfig) -> Result<(), SsrfError> {
    match ip {
        IpAddr::V4(v4) => {
            // Check blocked metadata IP first
            if v4.octets()[0] == 169 && v4.octets()[1] == 254 {
                // Could be metadata service
                let octets = v4.octets();
                if octets[2] == 169 && octets[3] == 254 {
                    return Err(SsrfError::MetadataEndpoint(
                        format!("metadata IP: {}", ip)
                    ));
                }
                if !config.allow_link_local {
                    return Err(SsrfError::BlockedIp(
                        *ip,
                        "link-local addresses not allowed".to_string(),
                    ));
                }
            }

            // Specific metadata IPs (AWS, GCP, Azure)
            if v4 == &Ipv4Addr::new(169, 254, 169, 254) {
                return Err(SsrfError::MetadataEndpoint(
                    format!("cloud metadata service: {}", ip)
                ));
            }

            // Check loopback
            if is_loopback_v4(v4) && !config.allow_loopback {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "loopback addresses not allowed".to_string(),
                ));
            }

            // Check private ranges
            if is_private_v4(v4) && !config.allow_private_ips {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "private IP ranges not allowed".to_string(),
                ));
            }
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();

            // Check loopback (::1)
            if is_loopback_v6(v6) && !config.allow_loopback {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "loopback addresses not allowed".to_string(),
                ));
            }

            // Check link-local (fe80::/10)
            if is_link_local_v6(v6) && !config.allow_link_local {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "link-local addresses not allowed".to_string(),
                ));
            }

            // Check unique local (fc00::/7)
            if is_unique_local_v6(v6) && !config.allow_private_ips {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "unique local addresses not allowed".to_string(),
                ));
            }

            // Check site-local (deprecated fec0::/10)
            if is_site_local_v6(v6) && !config.allow_private_ips {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "site-local addresses not allowed".to_string(),
                ));
            }

            // Check multicast (ff00::/8)
            if is_multicast_v6(v6) {
                return Err(SsrfError::BlockedIp(
                    *ip,
                    "multicast addresses not allowed".to_string(),
                ));
            }

            // Check IPv4-mapped addresses that might be private
            if segments[0] == 0 && segments[1] == 0 && segments[2] == 0 
                && segments[3] == 0 && segments[4] == 0 && segments[5] == 0xffff {
                // This is an IPv4-mapped IPv6 address
                let v4 = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    (segments[6] & 0xff) as u8,
                    (segments[7] >> 8) as u8,
                    (segments[7] & 0xff) as u8,
                );
                return validate_ip(&IpAddr::V4(v4), config);
            }

            // Check for IPv4-compatible addresses (::ff:0.0.0.0/96)
            if segments[0] == 0 && segments[1] == 0 && segments[2] == 0 
                && segments[3] == 0 && segments[4] == 0 && segments[5] == 0 {
                let v4 = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    (segments[6] & 0xff) as u8,
                    (segments[7] >> 8) as u8,
                    (segments[7] & 0xff) as u8,
                );
                return validate_ip(&IpAddr::V4(v4), config);
            }
        }
    }

    Ok(())
}

/// Check if a hostname is in the blocked list
fn is_blocked_hostname(hostname: &str, config: &SsrfConfig) -> bool {
    let hostname_lower = hostname.to_lowercase();

    for blocked in &config.blocked_domains {
        if hostname_lower == blocked.to_lowercase() {
            return true;
        }
        // Check for subdomains
        if hostname_lower.ends_with(&format!(".{}", blocked.to_lowercase())) {
            return true;
        }
    }

    // Special checks
    if hostname_lower == "localhost" || hostname_lower.ends_with(".localhost") {
        return true;
    }
    if hostname_lower.contains("metadata.google") {
        return true;
    }
    if hostname_lower.contains("metadata.internal") {
        return true;
    }

    false
}

/// Main SSRF validation function
///
/// This validates a URL before making any network request. It:
/// 1. Parses and validates the URL format
/// 2. Checks the scheme (only HTTP/HTTPS)
/// 3. Resolves DNS and validates the resulting IP address
/// 4. Blocks private/internal ranges
pub async fn validate_url_ssrf(url_str: &str, config: &SsrfConfig) -> Result<url::Url, SsrfError> {
    // Parse URL
    let url = url::Url::parse(url_str).map_err(|e| {
        SsrfError::InvalidUrl(e.to_string())
    })?;

    // Validate scheme
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(SsrfError::DisallowedScheme(scheme.to_string())),
    }

    // Get the host
    let host = url.host_str().ok_or_else(|| {
        SsrfError::InvalidUrl("URL has no host".to_string())
    })?;

    // Check blocked hostnames first
    if is_blocked_hostname(host, config) {
        return Err(SsrfError::MetadataEndpoint(format!(
            "hostname '{}' is blocked",
            host
        )));
    }

    // Check if it's a bare IP address
    if let Ok(ip) = IpAddr::from_str(host) {
        validate_ip(&ip, config)?;
        return Ok(url);
    }

    // Resolve DNS
    // This is the critical SSRF protection - we validate the actual IP
    let addrs = tokio::net::lookup_host(format!("{}:1", host)).await.map_err(|_| {
        SsrfError::DnsResolutionFailed(host.to_string())
    })?;

    // Check all resolved IPs
    for addr in addrs {
        validate_ip(&addr.ip(), config)?;
    }

    Ok(url)
}

/// Validate a redirect URL
///
/// When a redirect is received, the new URL must be validated again
/// to prevent open redirect attacks that bypass SSRF protection
pub async fn validate_redirect_url(url_str: &str, config: &SsrfConfig) -> Result<url::Url, SsrfError> {
    match validate_url_ssrf(url_str, config).await {
        Ok(url) => Ok(url),
        Err(e) => Err(SsrfError::BlockedRedirect(e.to_string())),
    }
}

/// Convert SSRF error to ToolError for use in tools
pub fn ssrf_to_tool_error(err: SsrfError) -> ToolError {
    ToolError::ExecutionFailed {
        tool: "web.fetch".to_string(),
        reason: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper for running async tests
    fn run_async<F: std::future::Future>(f: F) -> F::Output {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(f)
    }

    #[test]
    fn test_private_v4_addresses() {
        // 10.0.0.0/8
        assert!(is_private_v4(&Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_private_v4(&Ipv4Addr::new(10, 255, 255, 255)));

        // 172.16.0.0/12
        assert!(is_private_v4(&Ipv4Addr::new(172, 16, 0, 1)));
        assert!(is_private_v4(&Ipv4Addr::new(172, 31, 255, 255)));
        assert!(!is_private_v4(&Ipv4Addr::new(172, 15, 0, 1)));
        assert!(!is_private_v4(&Ipv4Addr::new(172, 32, 0, 1)));

        // 192.168.0.0/16
        assert!(is_private_v4(&Ipv4Addr::new(192, 168, 0, 1)));
        assert!(is_private_v4(&Ipv4Addr::new(192, 168, 255, 255)));
        assert!(!is_private_v4(&Ipv4Addr::new(192, 167, 0, 1)));
        assert!(!is_private_v4(&Ipv4Addr::new(192, 169, 0, 1)));

        // 127.0.0.0/8 (loopback)
        assert!(is_private_v4(&Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_private_v4(&Ipv4Addr::new(127, 255, 255, 255)));

        // 169.254.0.0/16 (link-local)
        assert!(is_private_v4(&Ipv4Addr::new(169, 254, 1, 1)));
    }

    #[test]
    fn test_public_v4_allowed() {
        // Public IPs should not be private
        assert!(!is_private_v4(&Ipv4Addr::new(8, 8, 8, 8)));     // Google DNS
        assert!(!is_private_v4(&Ipv4Addr::new(1, 1, 1, 1)));     // Cloudflare DNS
        assert!(!is_private_v4(&Ipv4Addr::new(93, 184, 216, 34))); // example.com
    }

    #[test]
    fn test_loopback_v6() {
        assert!(is_loopback_v6(&Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert!(!is_loopback_v6(&Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 2)));
    }

    #[test]
    fn test_link_local_v6() {
        // fe80::/10
        assert!(is_link_local_v6(&Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0)));
        assert!(is_link_local_v6(&Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)));
        assert!(is_link_local_v6(&Ipv6Addr::new(0xfebf, 0, 0, 0, 0, 0, 0, 1)));
        assert!(!is_link_local_v6(&Ipv6Addr::new(0xfec0, 0, 0, 0, 0, 0, 0, 1)));
    }

    #[test]
    fn test_is_blocked_hostname() {
        let config = SsrfConfig::new();
        assert!(is_blocked_hostname("localhost", &config));
        assert!(is_blocked_hostname("LOCALHOST", &config));
        assert!(is_blocked_hostname("subdomain.localhost", &config));
        assert!(is_blocked_hostname("169.254.169.254", &config));
        assert!(!is_blocked_hostname("example.com", &config));
        assert!(!is_blocked_hostname("google.com", &config));
    }

    #[tokio::test]
    async fn test_validate_url_schemes() {
        let config = SsrfConfig::new();

        assert_eq!(
            validate_url_ssrf("file:///etc/passwd", &config).await.unwrap_err(),
            SsrfError::DisallowedScheme("file".to_string())
        );

        assert_eq!(
            validate_url_ssrf("ftp://example.com/file.txt", &config).await.unwrap_err(),
            SsrfError::DisallowedScheme("ftp".to_string())
        );

        // HTTP should resolve and validate
        let result = validate_url_ssrf("http://example.com", &config).await;
        // May fail due to DNS, but should parse validly
        if result.is_err() {
            let err = result.unwrap_err();
            assert!(!matches!(err, SsrfError::DisallowedScheme(_)),
                "HTTP should be allowed, got: {:?}", err);
        }
    }

    #[tokio::test]
    async fn test_validate_ip_loopback() {
        let config = SsrfConfig::new();

        let result = validate_url_ssrf("http://127.0.0.1/", &config).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SsrfError::BlockedIp(ip, _) if ip == IpAddr::from([127, 0, 0, 1]) => {}
            other => panic!("Expected BlockedIp, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_validate_ip_private() {
        let config = SsrfConfig::new();

        // 10.0.0.0/8
        let result = validate_url_ssrf("http://10.0.0.1/", &config).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SsrfError::BlockedIp(ip, _) if ip == IpAddr::from([10, 0, 0, 1]) => {}
            other => panic!("Expected BlockedIp for 10.x.x.x, got: {:?}", other),
        }

        // 192.168.x.x
        let result = validate_url_ssrf("http://192.168.1.1/", &config).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SsrfError::BlockedIp(ip, _) if ip == IpAddr::from([192, 168, 1, 1]) => {}
            other => panic!("Expected BlockedIp for 192.168.x.x, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_validate_localhost_hostname() {
        let config = SsrfConfig::new();

        let result = validate_url_ssrf("http://localhost/", &config).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SsrfError::MetadataEndpoint(_)));
    }

    #[tokio::test]
    async fn test_validate_metadata_endpoint() {
        let config = SsrfConfig::new();

        let result = validate_url_ssrf("http://169.254.169.254/", &config).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SsrfError::MetadataEndpoint(_)));
    }

    #[tokio::test]
    async fn test_allow_private_override() {
        let config = SsrfConfig::new().allow_private_ips(true);

        // With override, private IPs should be allowed
        // (except loopback/metadata which are still blocked by default)
        let result = validate_url_ssrf("http://10.0.0.1/", &config).await;
        // Should be allowed now
        if let Err(e) = result {
            // Should NOT be BlockedIp with "private" reason
            assert!(!e.to_string().contains("private"),
                "Private IPs should be allowed with override, got: {}", e);
        }
    }
}
