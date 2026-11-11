use chrono::{NaiveDate, NaiveDateTime, Utc};

use crate::i18n::Locale;

/// Date/time helpers
pub struct DateHelper;

impl DateHelper {
    /// Human-readable "time ago" string, in the language of [`crate::i18n::locale`]:
    /// `"3 h ago"`, or `"Il y a 3 h"` with `APP_LOCALE=fr`.
    pub fn time_ago(date: &NaiveDateTime) -> String {
        Self::time_ago_in(date, crate::i18n::locale())
    }

    /// [`time_ago`](Self::time_ago) in a given language, whatever the process locale.
    pub fn time_ago_in(date: &NaiveDateTime, locale: Locale) -> String {
        let diff = Utc::now().naive_utc().signed_duration_since(*date);

        let seconds = diff.num_seconds();
        let minutes = diff.num_minutes();
        let hours = diff.num_hours();
        let days = diff.num_days();

        if seconds < 0 {
            return locale.pick("In the future", "Dans le futur").to_string();
        }
        if seconds < 60 {
            return locale.pick("Just now", "A l'instant").to_string();
        }

        let (count, en, fr) = if minutes < 60 {
            (minutes, "min", "min")
        } else if hours < 24 {
            (hours, "h", "h")
        } else if days < 30 {
            (days, "d", "j")
        } else if days < 365 {
            (days / 30, "mo", "mois")
        } else if days / 365 == 1 {
            (1, "yr", "an")
        } else {
            (days / 365, "yrs", "ans")
        };

        match locale {
            Locale::En => format!("{count} {en} ago"),
            Locale::Fr => format!("Il y a {count} {fr}"),
        }
    }

    /// Format a NaiveDateTime with the given chrono format string
    pub fn format(date: &NaiveDateTime, fmt: &str) -> String {
        date.format(fmt).to_string()
    }

    /// Format as "dd/mm/YYYY"
    pub fn format_fr(date: &NaiveDateTime) -> String {
        date.format("%d/%m/%Y").to_string()
    }

    /// Format as "dd/mm/YYYY HH:MM"
    pub fn format_fr_time(date: &NaiveDateTime) -> String {
        date.format("%d/%m/%Y %H:%M").to_string()
    }

    /// Format as "YYYY-MM-DD"
    pub fn format_iso(date: &NaiveDateTime) -> String {
        date.format("%Y-%m-%d").to_string()
    }

    /// Format as "YYYY-MM-DD HH:MM:SS"
    pub fn format_iso_time(date: &NaiveDateTime) -> String {
        date.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Check if a datetime is in the past
    pub fn is_past(date: &NaiveDateTime) -> bool {
        *date < Utc::now().naive_utc()
    }

    /// Check if a datetime is in the future
    pub fn is_future(date: &NaiveDateTime) -> bool {
        *date > Utc::now().naive_utc()
    }

    /// Check if a date is today
    pub fn is_today(date: &NaiveDate) -> bool {
        *date == Utc::now().date_naive()
    }

    /// Difference in days between two dates
    pub fn diff_in_days(from: &NaiveDateTime, to: &NaiveDateTime) -> i64 {
        to.signed_duration_since(*from).num_days()
    }

    /// Difference in hours between two datetimes
    pub fn diff_in_hours(from: &NaiveDateTime, to: &NaiveDateTime) -> i64 {
        to.signed_duration_since(*from).num_hours()
    }

    /// Difference in minutes between two datetimes
    pub fn diff_in_minutes(from: &NaiveDateTime, to: &NaiveDateTime) -> i64 {
        to.signed_duration_since(*from).num_minutes()
    }

    /// Get the current UTC datetime
    pub fn now() -> NaiveDateTime {
        Utc::now().naive_utc()
    }

    /// Get today's date
    pub fn today() -> NaiveDate {
        Utc::now().date_naive()
    }

    /// Get the start of the day (00:00:00)
    pub fn start_of_day(date: &NaiveDate) -> NaiveDateTime {
        date.and_hms_opt(0, 0, 0).unwrap()
    }

    /// Get the end of the day (23:59:59)
    pub fn end_of_day(date: &NaiveDate) -> NaiveDateTime {
        date.and_hms_opt(23, 59, 59).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dt(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, mi, s).unwrap()
    }

    #[test]
    fn test_format_fr() {
        let dt = make_dt(2024, 3, 15, 14, 30, 0);
        assert_eq!(DateHelper::format_fr(&dt), "15/03/2024");
    }

    #[test]
    fn test_format_fr_time() {
        let dt = make_dt(2024, 3, 15, 14, 30, 0);
        assert_eq!(DateHelper::format_fr_time(&dt), "15/03/2024 14:30");
    }

    #[test]
    fn test_format_iso() {
        let dt = make_dt(2024, 3, 15, 14, 30, 0);
        assert_eq!(DateHelper::format_iso(&dt), "2024-03-15");
    }

    #[test]
    fn test_format_iso_time() {
        let dt = make_dt(2024, 3, 15, 14, 30, 0);
        assert_eq!(DateHelper::format_iso_time(&dt), "2024-03-15 14:30:00");
    }

    #[test]
    fn test_format_custom() {
        let dt = make_dt(2024, 3, 15, 14, 30, 0);
        assert_eq!(DateHelper::format(&dt, "%H:%M"), "14:30");
    }

    #[test]
    fn test_is_past() {
        let past = make_dt(2020, 1, 1, 0, 0, 0);
        assert!(DateHelper::is_past(&past));
    }

    #[test]
    fn test_is_future() {
        let future = make_dt(2099, 1, 1, 0, 0, 0);
        assert!(DateHelper::is_future(&future));
    }

    #[test]
    fn test_diff_in_days() {
        let from = make_dt(2024, 1, 1, 0, 0, 0);
        let to = make_dt(2024, 1, 11, 0, 0, 0);
        assert_eq!(DateHelper::diff_in_days(&from, &to), 10);
    }

    #[test]
    fn test_diff_in_hours() {
        let from = make_dt(2024, 1, 1, 0, 0, 0);
        let to = make_dt(2024, 1, 1, 5, 0, 0);
        assert_eq!(DateHelper::diff_in_hours(&from, &to), 5);
    }

    #[test]
    fn test_diff_in_minutes() {
        let from = make_dt(2024, 1, 1, 0, 0, 0);
        let to = make_dt(2024, 1, 1, 1, 30, 0);
        assert_eq!(DateHelper::diff_in_minutes(&from, &to), 90);
    }

    #[test]
    fn test_start_of_day() {
        let date = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let start = DateHelper::start_of_day(&date);
        assert_eq!(start, make_dt(2024, 3, 15, 0, 0, 0));
    }

    #[test]
    fn test_end_of_day() {
        let date = NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let end = DateHelper::end_of_day(&date);
        assert_eq!(end, make_dt(2024, 3, 15, 23, 59, 59));
    }

    #[test]
    fn test_time_ago_future() {
        let future = make_dt(2099, 1, 1, 0, 0, 0);
        assert_eq!(DateHelper::time_ago_in(&future, Locale::En), "In the future");
        assert_eq!(DateHelper::time_ago_in(&future, Locale::Fr), "Dans le futur");
    }

    #[test]
    fn time_ago_speaks_both_languages() {
        let ago = |d: chrono::Duration, locale| DateHelper::time_ago_in(&(Utc::now().naive_utc() - d), locale);
        let cases = [
            (chrono::Duration::seconds(5), "Just now", "A l'instant"),
            (chrono::Duration::minutes(5), "5 min ago", "Il y a 5 min"),
            (chrono::Duration::hours(3), "3 h ago", "Il y a 3 h"),
            (chrono::Duration::days(2), "2 d ago", "Il y a 2 j"),
            (chrono::Duration::days(95), "3 mo ago", "Il y a 3 mois"),
            (chrono::Duration::days(400), "1 yr ago", "Il y a 1 an"),
            (chrono::Duration::days(800), "2 yrs ago", "Il y a 2 ans"),
        ];
        for (age, en, fr) in cases {
            assert_eq!(ago(age, Locale::En), en);
            assert_eq!(ago(age, Locale::Fr), fr);
        }
    }
}
