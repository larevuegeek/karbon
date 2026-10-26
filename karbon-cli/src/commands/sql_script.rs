//! MySQL scripts written for the `mysql` client: `DELIMITER //` lets a stored procedure
//! keep the `;` of its body. The server does not know that directive, so such a script is
//! cut here into statements sent one by one.

/// A line starting with `DELIMITER` (any case), as the `mysql` client reads it.
pub fn has_delimiter_directive(sql: &str) -> bool {
    sql.lines().any(|line| {
        line.get(..10)
            .is_some_and(|start| start.eq_ignore_ascii_case("DELIMITER "))
    })
}

/// Statements of a script, cut on the current terminator only outside strings,
/// backquoted identifiers and comments. `DELIMITER x` changes the terminator, like the
/// `mysql` client.
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut delimiter = ";".to_string();

    let mut rest = sql;

    while !rest.is_empty() {
        // A client directive, recognised at the start of a line only.
        if current.trim().is_empty()
            && at_line_start(sql, rest)
            && let Some(after) = strip_delimiter_keyword(rest)
        {
            let line_end = after.find('\n').map(|i| i + 1).unwrap_or(after.len());
            let value = after[..line_end].trim();

            if !value.is_empty() {
                delimiter = value.to_string();
            }

            rest = &after[line_end..];
            current.clear();
            continue;
        }

        if rest.starts_with(&delimiter) {
            push(&mut statements, &current);
            current.clear();
            rest = &rest[delimiter.len()..];
            continue;
        }

        let (token, len) = read_token(rest);
        current.push_str(token);
        rest = &rest[len..];
    }

    push(&mut statements, &current);
    statements
}

fn strip_delimiter_keyword(rest: &str) -> Option<&str> {
    rest.get(..10)
        .filter(|start| start.eq_ignore_ascii_case("DELIMITER "))
        .map(|_| &rest[10..])
}

/// One indivisible piece: a whole string, a whole comment, or one character. Returns the
/// text to keep and the bytes consumed — a comment is consumed, not kept.
fn read_token(rest: &str) -> (&str, usize) {
    let bytes = rest.as_bytes();

    match bytes[0] {
        b'\'' | b'"' | b'`' => {
            let quote = bytes[0];
            let mut i = 1;

            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if quote != b'`' => i += 2,
                    c if c == quote => {
                        // A doubled quote inside a string is an escaped quote.
                        if bytes.get(i + 1) == Some(&quote) {
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    }
                    _ => i += 1,
                }
            }

            let end = i.min(rest.len());
            (&rest[..end], end)
        }

        // MySQL opens a comment on `--` followed by whitespace only: `5--3` is arithmetic.
        b'-' if rest.starts_with("--")
            && rest[2..].chars().next().is_none_or(char::is_whitespace) =>
        {
            let end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
            ("", end)
        }

        b'#' => {
            let end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
            ("", end)
        }

        b'/' if rest.starts_with("/*") => {
            let end = rest[2..].find("*/").map(|i| i + 4).unwrap_or(rest.len());
            ("", end)
        }

        _ => {
            let end = rest.chars().next().map(char::len_utf8).unwrap_or(1);
            (&rest[..end], end)
        }
    }
}

fn at_line_start(whole: &str, rest: &str) -> bool {
    let offset = rest.as_ptr() as usize - whole.as_ptr() as usize;

    offset == 0 || whole.as_bytes()[offset - 1] == b'\n'
}

fn push(statements: &mut Vec<String>, current: &str) {
    let trimmed = current.trim();

    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_directive_at_line_start_counts() {
        assert!(has_delimiter_directive("SELECT 1;\ndelimiter //\n"));
        assert!(!has_delimiter_directive("SELECT 1;\n  -- DELIMITER //\n"));
        assert!(!has_delimiter_directive("SELECT 'DELIMITER ';"));
    }

    #[test]
    fn a_banner_does_not_hide_the_statement() {
        let sql = "-- ====\n-- Keys table\n-- ====\n\nCREATE TABLE x (id INT);";

        assert_eq!(split_statements(sql), vec!["CREATE TABLE x (id INT)"]);
    }

    #[test]
    fn a_file_of_pure_comments_yields_nothing() {
        assert!(split_statements("-- nothing\n-- but comments\n").is_empty());
    }

    #[test]
    fn a_semicolon_inside_a_string_does_not_split() {
        let sql = "SET @s := 'ALTER TABLE t ADD c INT COMMENT ''a; b'';';\nSELECT 1;";

        assert_eq!(
            split_statements(sql),
            vec![
                "SET @s := 'ALTER TABLE t ADD c INT COMMENT ''a; b'';'",
                "SELECT 1"
            ]
        );
    }

    #[test]
    fn backticks_and_double_quotes_are_respected() {
        let sql = r#"CREATE TABLE `a;b` (c VARCHAR(9) DEFAULT "x;y");"#;

        assert_eq!(split_statements(sql).len(), 1);
    }

    #[test]
    fn delimiter_keeps_a_procedure_whole() {
        let sql = "DROP PROCEDURE IF EXISTS p;\nDELIMITER //\nCREATE PROCEDURE p()\nBEGIN\n    SET @a = 1;\n    SET @b = 2;\nEND //\nDELIMITER ;\nCALL p();";

        let statements = split_statements(sql);

        assert_eq!(statements.len(), 3, "got: {statements:#?}");
        assert!(statements[1].starts_with("CREATE PROCEDURE p()"));
        assert!(statements[1].contains("SET @b = 2;"));
        assert_eq!(statements[2], "CALL p()");
    }

    #[test]
    fn an_inner_comment_is_dropped_not_the_statement() {
        let statements = split_statements("ALTER TABLE t\n    ADD COLUMN c INT -- quota\n;");

        assert_eq!(statements.len(), 1);
        assert!(statements[0].starts_with("ALTER TABLE t"));
        assert!(!statements[0].contains("quota"));
    }

    #[test]
    fn a_double_dash_without_space_is_arithmetic() {
        assert_eq!(split_statements("SELECT 5--3;"), vec!["SELECT 5--3"]);
    }

    #[test]
    fn a_trailing_statement_without_semicolon_is_kept() {
        assert_eq!(split_statements("SELECT 1"), vec!["SELECT 1"]);
    }
}
