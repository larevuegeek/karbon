use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};
use chrono::NaiveDate;

/// Validates that a date falls within a given range.
pub struct DateRange {
    pub min: Option<NaiveDate>,
    pub max: Option<NaiveDate>,
    pub format: String,
    pub min_message: String,
    pub max_message: String,
    pub invalid_message: String,
}

impl Default for DateRange {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            format: "%Y-%m-%d".to_string(),
            min_message: "This date should be {{ limit }} or later.".to_string(),
            max_message: "This date should be {{ limit }} or earlier.".to_string(),
            invalid_message: "This value is not a valid date.".to_string(),
        }
    }
}

impl DateRange {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min(mut self, date: NaiveDate) -> Self {
        self.min = Some(date);
        self
    }

    pub fn max(mut self, date: NaiveDate) -> Self {
        self.max = Some(date);
        self
    }

    pub fn between(min: NaiveDate, max: NaiveDate) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..Self::default()
        }
    }

    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    pub fn future_only() -> Self {
        Self {
            min: Some(chrono::Utc::now().date_naive()),
            min_message: "This date should be in the future.".to_string(),
            ..Self::default()
        }
    }

    pub fn past_only() -> Self {
        Self {
            max: Some(chrono::Utc::now().date_naive()),
            max_message: "This date should be in the past.".to_string(),
            ..Self::default()
        }
    }
}

impl Constraint for DateRange {
    fn validate(&self, value: &str) -> ConstraintResult {
        let date = NaiveDate::parse_from_str(value, &self.format).map_err(|_| {
            ConstraintViolation::new(self.name(), &self.invalid_message, value)
        })?;

        if let Some(min) = self.min {
            if date < min {
                return Err(ConstraintViolation::new(
                    self.name(),
                    self.min_message.replace("{{ limit }}", &min.to_string()),
                    value,
                ));
            }
        }

        if let Some(max) = self.max {
            if date > max {
                return Err(ConstraintViolation::new(
                    self.name(),
                    self.max_message.replace("{{ limit }}", &max.to_string()),
                    value,
                ));
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "DateRange"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_within_range() {
        let constraint = DateRange::between(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        );
        assert!(constraint.validate("2024-06-15").is_ok());
        assert!(constraint.validate("2024-01-01").is_ok());
        assert!(constraint.validate("2024-12-31").is_ok());
    }

    #[test]
    fn test_out_of_range() {
        let constraint = DateRange::between(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        );
        assert!(constraint.validate("2023-12-31").is_err());
        assert!(constraint.validate("2025-01-01").is_err());
    }

    #[test]
    fn test_min_only() {
        let constraint = DateRange::new().min(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        assert!(constraint.validate("2024-06-15").is_ok());
        assert!(constraint.validate("2023-12-31").is_err());
    }

    #[test]
    fn test_max_only() {
        let constraint = DateRange::new().max(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());
        assert!(constraint.validate("2024-06-15").is_ok());
        assert!(constraint.validate("2025-01-01").is_err());
    }

    #[test]
    fn test_invalid_date_format() {
        let constraint = DateRange::new();
        assert!(constraint.validate("not-a-date").is_err());
    }

    #[test]
    fn test_past_only() {
        let constraint = DateRange::past_only();
        assert!(constraint.validate("2020-01-01").is_ok());
        assert!(constraint.validate("2099-01-01").is_err());
    }

    #[test]
    fn test_future_only() {
        let constraint = DateRange::future_only();
        assert!(constraint.validate("2099-01-01").is_ok());
        assert!(constraint.validate("2020-01-01").is_err());
    }
}
