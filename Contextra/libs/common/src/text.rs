/// Truncates a string safely at char boundaries.
pub fn truncate_safe(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }

    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Normalizes whitespace by replacing multiple whitespaces with a single space,
/// and trimming the start and end.
pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("hello", 10), "hello");
        assert_eq!(truncate_safe("hello", 5), "hello");
        assert_eq!(truncate_safe("hello", 4), "hell");

        // Multi-byte character test (e.g., emojis or accented chars)
        let s = "こんにちは"; // Each character is 3 bytes
        assert_eq!(truncate_safe(s, 0), "");
        assert_eq!(truncate_safe(s, 1), "");
        assert_eq!(truncate_safe(s, 3), "こ");
        assert_eq!(truncate_safe(s, 4), "こ");
        assert_eq!(truncate_safe(s, 6), "こん");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
        assert_eq!(
            normalize_whitespace("hello\tworld\n\nfoo"),
            "hello world foo"
        );
        assert_eq!(normalize_whitespace(""), "");
        assert_eq!(normalize_whitespace("   "), "");
    }
}
