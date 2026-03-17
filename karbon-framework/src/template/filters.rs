//! Custom Tera filters for common operations.

use std::collections::HashMap;
use tera::{Value, Result as TeraResult};

/// Format a date string to French locale: "17 mars 2026"
pub fn date_fr(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");

    // Try parsing ISO datetime
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Value::String(format_date_fr(&dt)));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(Value::String(format_date_fr(&dt)));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Value::String(format_date_fr(&dt)));
    }

    Ok(Value::String(s.to_string()))
}

fn format_date_fr(dt: &chrono::NaiveDateTime) -> String {
    let months = [
        "", "janvier", "février", "mars", "avril", "mai", "juin",
        "juillet", "août", "septembre", "octobre", "novembre", "décembre"
    ];
    let month = dt.format("%m").to_string().parse::<usize>().unwrap_or(0);
    format!("{} {} {}", dt.format("%d"), months.get(month).unwrap_or(&""), dt.format("%Y"))
}

/// Format datetime to French: "17 mars 2026 à 14h30"
pub fn datetime_fr(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");

    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Value::String(format!("{} à {}h{}", format_date_fr(&dt), dt.format("%H"), dt.format("%M"))));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(Value::String(format!("{} à {}h{}", format_date_fr(&dt), dt.format("%H"), dt.format("%M"))));
    }

    Ok(Value::String(s.to_string()))
}

/// Format a price with € symbol: "29,99 €"
pub fn currency(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let n = value.as_f64().unwrap_or(0.0);
    Ok(Value::String(format!("{:.2} €", n).replace('.', ",")))
}

/// Truncate text to N characters with ellipsis
pub fn truncate_text(value: &Value, args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");
    let len = args.get("length")
        .and_then(|v| v.as_u64())
        .unwrap_or(150) as usize;

    if s.len() <= len {
        return Ok(Value::String(s.to_string()));
    }

    // Truncate at word boundary
    let truncated = &s[..len];
    let end = truncated.rfind(' ').unwrap_or(len);
    Ok(Value::String(format!("{}…", &s[..end])))
}

/// Generate a slug from text
pub fn slugify(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");
    Ok(Value::String(slug::slugify(s)))
}

/// Strip HTML tags
pub fn strip_tags(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    Ok(Value::String(result))
}

/// Nl2br: convert newlines to <br>
pub fn nl2br(value: &Value, _args: &HashMap<String, Value>) -> TeraResult<Value> {
    let s = value.as_str().unwrap_or("");
    Ok(Value::String(s.replace('\n', "<br>\n")))
}

/// Register all custom filters on a Tera instance
pub fn register_all(tera: &mut tera::Tera) {
    tera.register_filter("date_fr", date_fr);
    tera.register_filter("datetime_fr", datetime_fr);
    tera.register_filter("currency", currency);
    tera.register_filter("truncate_text", truncate_text);
    tera.register_filter("slugify", slugify);
    tera.register_filter("strip_tags", strip_tags);
    tera.register_filter("nl2br", nl2br);
}
