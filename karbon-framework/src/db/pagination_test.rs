#[cfg(test)]
mod tests {
    use crate::db::PaginationParams;

    fn params(page: u32, per_page: u32, sort: Option<&str>, order: &str) -> PaginationParams {
        PaginationParams {
            page,
            per_page,
            sort: sort.map(|s| s.to_string()),
            order: order.to_string(),
            search: None,
        }
    }

    #[test]
    fn offset_is_zero_for_page_one() {
        let p = params(1, 20, None, "desc");
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn offset_for_page_two() {
        let p = params(2, 20, None, "desc");
        assert_eq!(p.offset(), 20);
    }

    #[test]
    fn offset_for_page_three_with_custom_per_page() {
        let p = params(3, 10, None, "desc");
        assert_eq!(p.offset(), 20);
    }

    #[test]
    fn offset_does_not_underflow_for_page_zero() {
        let p = params(0, 20, None, "desc");
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn order_direction_normalizes_asc() {
        let p = params(1, 20, None, "asc");
        assert_eq!(p.order_direction(), "ASC");
    }

    #[test]
    fn order_direction_normalizes_desc() {
        let p = params(1, 20, None, "DESC");
        assert_eq!(p.order_direction(), "DESC");
    }

    #[test]
    fn order_direction_defaults_to_desc() {
        let p = params(1, 20, None, "invalid");
        assert_eq!(p.order_direction(), "DESC");
    }

    #[test]
    fn sort_column_uses_allowed_value() {
        let p = params(1, 20, Some("name"), "asc");
        let col = p.sort_column(&["id", "name", "created"], "id");
        assert_eq!(col, "name");
    }

    #[test]
    fn sort_column_rejects_unknown_and_uses_default() {
        let p = params(1, 20, Some("DROP TABLE"), "asc");
        let col = p.sort_column(&["id", "name", "created"], "id");
        assert_eq!(col, "id");
    }

    #[test]
    fn sort_column_uses_default_when_none() {
        let p = params(1, 20, None, "asc");
        let col = p.sort_column(&["id", "name", "created"], "id");
        assert_eq!(col, "id");
    }
}
