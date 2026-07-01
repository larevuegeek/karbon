/// Number formatting and utility helpers
pub struct NumberHelper;

impl NumberHelper {
    /// Format a file size in human-readable format (French units)
    pub fn filesize_format(bytes: u64) -> String {
        const UNITS: &[&str] = &["o", "Ko", "Mo", "Go", "To"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{} {}", bytes, UNITS[0])
        } else {
            format!("{:.1} {}", size, UNITS[unit_idx])
        }
    }

    /// Format a number with thousands separator
    /// e.g., 1234567 => "1 234 567" (French style with spaces)
    pub fn format_number(value: i64, separator: Option<&str>) -> String {
        let separator = separator.unwrap_or(" ");
        let negative = value < 0;
        let s = value.unsigned_abs().to_string();
        let chars: Vec<char> = s.chars().collect();
        let mut result = String::new();

        for (i, c) in chars.iter().enumerate() {
            if i > 0 && (chars.len() - i).is_multiple_of(3) {
                result.push_str(separator);
            }
            result.push(*c);
        }

        if negative {
            format!("-{}", result)
        } else {
            result
        }
    }

    /// Format a float with specified decimal places and thousands separator
    pub fn format_decimal(value: f64, decimals: usize, separator: Option<&str>) -> String {
        let integer_part = value.trunc() as i64;
        let formatted_int = Self::format_number(integer_part, separator);

        if decimals == 0 {
            return formatted_int;
        }

        let decimal_part = (value.fract().abs() * 10_f64.powi(decimals as i32)).round() as u64;
        let decimal_str = format!("{:0>width$}", decimal_part, width = decimals);

        format!("{},{}", formatted_int, decimal_str)
    }

    /// Format as ordinal (French): 1 => "1er", 2 => "2e", etc.
    pub fn ordinal(value: u64) -> String {
        if value == 1 {
            "1er".to_string()
        } else {
            format!("{}e", value)
        }
    }

    /// Format as percentage
    /// e.g., percentage(0.756, 1) => "75.6%"
    pub fn percentage(value: f64, decimals: usize) -> String {
        format!("{:.prec$}%", value * 100.0, prec = decimals)
    }

    /// Clamp a value between min and max
    pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
        if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        }
    }

    /// Convert bytes to a human-readable duration string (French)
    /// e.g., 3661 => "1h 1min 1s"
    pub fn duration_format(total_seconds: u64) -> String {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        let mut parts = Vec::new();

        if hours > 0 {
            parts.push(format!("{}h", hours));
        }
        if minutes > 0 {
            parts.push(format!("{}min", minutes));
        }
        if seconds > 0 || parts.is_empty() {
            parts.push(format!("{}s", seconds));
        }

        parts.join(" ")
    }

    /// Round a float to n decimal places
    pub fn round(value: f64, decimals: u32) -> f64 {
        let factor = 10_f64.powi(decimals as i32);
        (value * factor).round() / factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filesize_format() {
        assert_eq!(NumberHelper::filesize_format(0), "0 o");
        assert_eq!(NumberHelper::filesize_format(512), "512 o");
        assert_eq!(NumberHelper::filesize_format(1024), "1.0 Ko");
        assert_eq!(NumberHelper::filesize_format(1536), "1.5 Ko");
        assert_eq!(NumberHelper::filesize_format(1048576), "1.0 Mo");
        assert_eq!(NumberHelper::filesize_format(1073741824), "1.0 Go");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(NumberHelper::format_number(1234567, None), "1 234 567");
        assert_eq!(NumberHelper::format_number(42, None), "42");
        assert_eq!(NumberHelper::format_number(1000, Some(",")), "1,000");
        assert_eq!(NumberHelper::format_number(-1234, None), "-1 234");
        assert_eq!(NumberHelper::format_number(0, None), "0");
    }

    #[test]
    fn test_format_decimal() {
        assert_eq!(NumberHelper::format_decimal(1234.56, 2, None), "1 234,56");
        assert_eq!(NumberHelper::format_decimal(0.5, 2, None), "0,50");
        assert_eq!(NumberHelper::format_decimal(1000.0, 0, None), "1 000");
    }

    #[test]
    fn test_ordinal() {
        assert_eq!(NumberHelper::ordinal(1), "1er");
        assert_eq!(NumberHelper::ordinal(2), "2e");
        assert_eq!(NumberHelper::ordinal(100), "100e");
    }

    #[test]
    fn test_percentage() {
        assert_eq!(NumberHelper::percentage(0.756, 1), "75.6%");
        assert_eq!(NumberHelper::percentage(1.0, 0), "100%");
        assert_eq!(NumberHelper::percentage(0.5, 0), "50%");
    }

    #[test]
    fn test_clamp() {
        assert_eq!(NumberHelper::clamp(5, 0, 10), 5);
        assert_eq!(NumberHelper::clamp(-5, 0, 10), 0);
        assert_eq!(NumberHelper::clamp(15, 0, 10), 10);
        assert_eq!(NumberHelper::clamp(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn test_duration_format() {
        assert_eq!(NumberHelper::duration_format(0), "0s");
        assert_eq!(NumberHelper::duration_format(45), "45s");
        assert_eq!(NumberHelper::duration_format(90), "1min 30s");
        assert_eq!(NumberHelper::duration_format(3600), "1h");
        assert_eq!(NumberHelper::duration_format(3661), "1h 1min 1s");
        assert_eq!(NumberHelper::duration_format(7200), "2h");
    }

    #[test]
    fn test_round() {
        assert_eq!(NumberHelper::round(5.67891, 2), 5.68);
        assert_eq!(NumberHelper::round(3.145, 2), 3.15);
        assert_eq!(NumberHelper::round(3.0, 2), 3.0);
    }
}
