use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
    pub has_more: bool,
    pub total_count: Option<u64>,
}

impl<T> Page<T> {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
            total_count: Some(0),
        }
    }

    pub fn new(
        items: Vec<T>,
        next_cursor: Option<Cursor>,
        has_more: bool,
        total_count: Option<u64>,
    ) -> Self {
        Self {
            items,
            next_cursor,
            has_more,
            total_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_empty() {
        let page: Page<i32> = Page::empty();
        assert!(page.items.is_empty());
        assert_eq!(page.next_cursor, None);
        assert!(!page.has_more);
        assert_eq!(page.total_count, Some(0));
    }

    #[test]
    fn test_page_new() {
        let cursor = Cursor("next_token".to_string());
        let page = Page::new(vec![1, 2, 3], Some(cursor.clone()), true, Some(10));
        assert_eq!(page.items, vec![1, 2, 3]);
        assert_eq!(page.next_cursor, Some(cursor));
        assert!(page.has_more);
        assert_eq!(page.total_count, Some(10));
    }
}
