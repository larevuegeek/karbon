use super::NumberHelper;

/// Amounts in integer cents: exact arithmetic, no floating point drift between a cart, an
/// order and its invoice.
pub struct MoneyHelper;

impl MoneyHelper {
    /// `"12.50"` → `Some(1250)`, `"-3.99"` → `Some(-399)`, `"12"` → `Some(1200)`,
    /// `"12.5"` → `Some(1250)`.
    ///
    /// Strict on purpose: `.` only (no comma, no thousands separator, no exponent), at
    /// most two decimals, no trailing dot. `None` for anything else or on overflow.
    pub fn parse_cents(s: &str) -> Option<i64> {
        let s = s.trim();
        let (neg, rest) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s),
        };

        let (int_part, frac_part) = rest.split_once('.').unwrap_or((rest, "00"));
        if int_part.is_empty() || !int_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if frac_part.is_empty()
            || frac_part.len() > 2
            || !frac_part.chars().all(|c| c.is_ascii_digit())
        {
            return None;
        }

        let int_v: i64 = int_part.parse().ok()?;
        let frac_v: i64 = match frac_part.len() {
            1 => frac_part.parse::<i64>().ok()? * 10,
            _ => frac_part.parse::<i64>().ok()?,
        };

        let total = int_v.checked_mul(100)?.checked_add(frac_v)?;

        Some(if neg { -total } else { total })
    }

    /// `1250` → `"12.50"`, `-399` → `"-3.99"`: always two decimals, `.` separator — the
    /// format of a `DECIMAL(x,2)` column, and what [`parse_cents`](Self::parse_cents) reads.
    pub fn format_cents(cents: i64) -> String {
        let sign = if cents < 0 { "-" } else { "" };
        let abs = cents.unsigned_abs();

        format!("{sign}{}.{:02}", abs / 100, abs % 100)
    }

    /// French display: `1234.56` → `"1 234,56 €"`.
    pub fn format_eur(amount: f64) -> String {
        format!("{} €", NumberHelper::format_decimal(amount, 2, None))
    }

    /// A percentage (`"20"`, `"5.5"`) in hundredths of a percent (`"20"` → `2000`),
    /// between 0 and 100 %. Empty → `Some(0)`; negative, more than two decimals or above
    /// 100 → `None`. Typical use: a VAT rate.
    pub fn parse_percent_hundredths(s: &str) -> Option<i64> {
        let s = s.trim();
        if s.is_empty() {
            return Some(0);
        }
        if s.starts_with('-') {
            return None;
        }

        let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
        if int_part.is_empty() || !int_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if frac_part.len() > 2 || !frac_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        let int_v: i64 = int_part.parse().ok()?;
        let frac_v: i64 = match frac_part.len() {
            0 => 0,
            1 => frac_part.parse::<i64>().ok()? * 10,
            _ => frac_part.parse::<i64>().ok()?,
        };

        let hundredths = int_v.checked_mul(100)?.checked_add(frac_v)?;

        (hundredths <= 10_000).then_some(hundredths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cents() {
        assert_eq!(MoneyHelper::parse_cents("0"), Some(0));
        assert_eq!(MoneyHelper::parse_cents("12"), Some(1200));
        assert_eq!(MoneyHelper::parse_cents("12.5"), Some(1250));
        assert_eq!(MoneyHelper::parse_cents(" 12.50 "), Some(1250));
        assert_eq!(MoneyHelper::parse_cents("-3.99"), Some(-399));
    }

    #[test]
    fn rejects_anything_ambiguous() {
        for input in [
            "",
            "abc",
            "12,50",
            "1.234",
            "1e5",
            "1.",
            "1 000.00",
            "99999999999999999999",
        ] {
            assert_eq!(MoneyHelper::parse_cents(input), None, "{input:?}");
        }
    }

    #[test]
    fn formats_round_trip() {
        for cents in [0i64, 1, 99, 100, 1250, -399, 199_999_999] {
            assert_eq!(
                MoneyHelper::parse_cents(&MoneyHelper::format_cents(cents)),
                Some(cents)
            );
        }
        assert_eq!(MoneyHelper::format_cents(-399), "-3.99");
    }

    #[test]
    fn formats_euros_the_french_way() {
        assert_eq!(MoneyHelper::format_eur(1234.56), "1 234,56 €");
        assert_eq!(MoneyHelper::format_eur(0.5), "0,50 €");
    }

    #[test]
    fn parses_percentages() {
        assert_eq!(MoneyHelper::parse_percent_hundredths("20"), Some(2000));
        assert_eq!(MoneyHelper::parse_percent_hundredths("5.5"), Some(550));
        assert_eq!(MoneyHelper::parse_percent_hundredths(""), Some(0));
        assert_eq!(MoneyHelper::parse_percent_hundredths("100.01"), None);
        assert_eq!(MoneyHelper::parse_percent_hundredths("-1"), None);
    }
}
