use serde::{Deserialize, Serialize};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    offset: usize,
    limit: usize,
}

impl PageRequest {
    pub fn new(offset: usize, limit: usize) -> Self {
        Self {
            offset,
            limit: limit.clamp(1, MAX_LIMIT),
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::new(0, DEFAULT_LIMIT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, total: usize, request: PageRequest) -> Self {
        Self {
            items,
            total,
            offset: request.offset(),
            limit: request.limit(),
        }
    }

    pub fn slice(all: Vec<T>, request: PageRequest) -> Self {
        let total = all.len();
        let items = all
            .into_iter()
            .skip(request.offset())
            .take(request.limit())
            .collect();
        Self::new(items, total, request)
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            offset: self.offset,
            limit: self.limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped_into_a_sane_range() {
        assert_eq!(PageRequest::new(0, 0).limit(), 1);
        assert_eq!(PageRequest::new(0, 10_000).limit(), MAX_LIMIT);
        assert_eq!(PageRequest::default().limit(), DEFAULT_LIMIT);
    }

    #[test]
    fn slice_reports_the_total_before_slicing() {
        let page = Page::slice((1..=10).collect(), PageRequest::new(2, 3));
        assert_eq!(page.items, vec![3, 4, 5]);
        assert_eq!(page.total, 10, "total counts everything, not the page");
    }

    #[test]
    fn slice_past_the_end_yields_an_empty_page_not_an_error() {
        let page = Page::slice((1..=3).collect(), PageRequest::new(99, 5));
        assert!(page.items.is_empty());
        assert_eq!(page.total, 3);
    }

    #[test]
    fn map_preserves_pagination_metadata() {
        let page = Page::slice((1..=10).collect(), PageRequest::new(0, 2)).map(|n| n * 2);
        assert_eq!(page.items, vec![2, 4]);
        assert_eq!(page.total, 10);
        assert_eq!(page.limit, 2);
    }
}
