use super::PaginationParams;
use super::placeholder;

/// Simple SQL query builder for common patterns
///
/// # Example
/// ```ignore
/// let (sql, count_sql) = QueryBuilder::select("article")
///     .columns("a.id, a.title, a.status, a.created")
///     .alias("a")
///     .join("LEFT JOIN user u ON a.user_id = u.id")
///     .where_clause("a.status = 'published'")
///     .search_columns(&["a.title", "a.content"])
///     .paginate(&params);
/// ```
pub struct QueryBuilder {
    table: String,
    alias: String,
    columns: String,
    joins: Vec<String>,
    conditions: Vec<String>,
    search_cols: Vec<String>,
    group_by: Option<String>,
}

impl QueryBuilder {
    pub fn select(table: &str) -> Self {
        Self {
            table: table.to_string(),
            alias: String::new(),
            columns: "*".to_string(),
            joins: Vec::new(),
            conditions: Vec::new(),
            search_cols: Vec::new(),
            group_by: None,
        }
    }

    pub fn columns(mut self, cols: &str) -> Self {
        self.columns = cols.to_string();
        self
    }

    pub fn alias(mut self, alias: &str) -> Self {
        self.alias = alias.to_string();
        self
    }

    pub fn join(mut self, join: &str) -> Self {
        self.joins.push(join.to_string());
        self
    }

    pub fn where_clause(mut self, condition: &str) -> Self {
        self.conditions.push(condition.to_string());
        self
    }

    pub fn where_if(mut self, condition: &str, apply: bool) -> Self {
        if apply {
            self.conditions.push(condition.to_string());
        }
        self
    }

    pub fn search_columns(mut self, cols: &[&str]) -> Self {
        self.search_cols = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn group_by(mut self, clause: &str) -> Self {
        self.group_by = Some(clause.to_string());
        self
    }

    /// Build SELECT + COUNT queries for pagination
    ///
    /// Returns (data_query, count_query)
    /// Search params are bound separately by the caller
    pub fn paginate(&self, params: &PaginationParams) -> (String, String) {
        let from = if self.alias.is_empty() {
            self.table.clone()
        } else {
            format!("{} {}", self.table, self.alias)
        };

        let joins = self.joins.join(" ");

        let mut where_parts = self.conditions.clone();

        // Add search condition (placeholders start at 1 since WHERE conditions are raw)
        if params.search.is_some() && !self.search_cols.is_empty() {
            let search_cond = self
                .search_cols
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{} LIKE {}", c, placeholder(i + 1)))
                .collect::<Vec<_>>()
                .join(" OR ");
            where_parts.push(format!("({})", search_cond));
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        let group = self
            .group_by
            .as_ref()
            .map(|g| format!(" GROUP BY {}", g))
            .unwrap_or_default();

        let sort = params.sort_column(&[], "id");
        let order = params.order_direction();

        let data_query = format!(
            "SELECT {} FROM {} {}{}{} ORDER BY {} {} LIMIT {} OFFSET {}",
            self.columns,
            from,
            joins,
            where_clause,
            group,
            sort,
            order,
            params.safe_per_page(),
            params.offset()
        );

        let count_query = format!(
            "SELECT COUNT(*) as count FROM {} {}{}{}",
            from, joins, where_clause, group
        );

        (data_query, count_query)
    }
}
