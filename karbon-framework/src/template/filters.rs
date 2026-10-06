//! Custom Tera filters for common operations.
//!
//! Tera 2 passes the already-coerced argument (`&str`, `f64`, …) plus the keyword
//! arguments and the render state, instead of a `&Value` and a `HashMap`. Returning
//! a plain `String` is enough; `TeraResult<String>` is for filters that can fail.
//!
//! None of these mark themselves safe, so their output is still auto-escaped in
//! `.html`/`.htm`/`.xml` templates unless the template applies `| safe`.

use tera::{Kwargs, State, Tera, TeraResult};

/// Format a date string to French locale: "17 mars 2026"
pub fn date_fr(value: &str, _kwargs: Kwargs, _state: &State) -> String {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return format_date_fr(&dt);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return format_date_fr(&dt);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return format_date_fr(&dt);
    }

    value.to_string()
}

fn format_date_fr(dt: &chrono::NaiveDateTime) -> String {
    let months = [
        "",
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    let month = dt.format("%m").to_string().parse::<usize>().unwrap_or(0);
    format!(
        "{} {} {}",
        dt.format("%d"),
        months.get(month).unwrap_or(&""),
        dt.format("%Y")
    )
}

/// Format datetime to French: "17 mars 2026 à 14h30"
pub fn datetime_fr(value: &str, _kwargs: Kwargs, _state: &State) -> String {
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, fmt) {
            return format!(
                "{} à {}h{}",
                format_date_fr(&dt),
                dt.format("%H"),
                dt.format("%M")
            );
        }
    }

    value.to_string()
}

/// Format a price with € symbol: "29,99 €"
pub fn currency(value: f64, _kwargs: Kwargs, _state: &State) -> String {
    format!("{value:.2} €").replace('.', ",")
}

/// Truncate text to N characters with ellipsis, cutting on a word boundary.
pub fn truncate_text(value: &str, kwargs: Kwargs, _state: &State) -> TeraResult<String> {
    let len = kwargs.get::<usize>("length")?.unwrap_or(150);

    if value.chars().count() <= len {
        return Ok(value.to_string());
    }

    // Count characters, not bytes: slicing mid-codepoint panics, and accented text
    // (the whole point of the French filters above) reaches that case immediately.
    let cut = value
        .char_indices()
        .nth(len)
        .map(|(i, _)| i)
        .unwrap_or(value.len());
    let end = value[..cut].rfind(' ').unwrap_or(cut);
    Ok(format!("{}…", &value[..end]))
}

/// Generate a slug from text
pub fn slugify(value: &str, _kwargs: Kwargs, _state: &State) -> String {
    slug::slugify(value)
}

/// Strip HTML tags
pub fn strip_tags(value: &str, _kwargs: Kwargs, _state: &State) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
}

/// Nl2br: convert newlines to <br>
pub fn nl2br(value: &str, _kwargs: Kwargs, _state: &State) -> String {
    value.replace('\n', "<br>\n")
}

/// Register all custom filters on a Tera instance
pub fn register_all(tera: &mut Tera) {
    tera.register_filter("date_fr", date_fr);
    tera.register_filter("datetime_fr", datetime_fr);
    tera.register_filter("currency", currency);
    tera.register_filter("truncate_text", truncate_text);
    tera.register_filter("slugify", slugify);
    tera.register_filter("strip_tags", strip_tags);
    tera.register_filter("nl2br", nl2br);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tera::Context;

    /// Render `source` through a Tera instance carrying the custom filters.
    /// Goes through `register_all` on purpose: it is the registration that the
    /// Tera 2 port had to change, so the tests must exercise it.
    fn render(source: &str, ctx: &Context) -> String {
        let mut tera = Tera::new();
        register_all(&mut tera);
        tera.add_raw_template("t.txt", source).unwrap();
        tera.render("t.txt", ctx).unwrap()
    }

    // Tera 2's `Context::insert` takes `Into<Cow<'static, str>>` for the key.
    fn ctx_with(key: &'static str, value: &str) -> Context {
        let mut c = Context::new();
        c.insert(key, value);
        c
    }

    #[test]
    fn dates_render_in_french() {
        let c = ctx_with("d", "2026-03-17");
        assert_eq!(render("{{ d | date_fr }}", &c), "17 mars 2026");

        let c = ctx_with("d", "2026-03-17T14:30:00");
        assert_eq!(render("{{ d | datetime_fr }}", &c), "17 mars 2026 à 14h30");
    }

    #[test]
    fn unparseable_date_passes_through() {
        let c = ctx_with("d", "pas une date");
        assert_eq!(render("{{ d | date_fr }}", &c), "pas une date");
    }

    #[test]
    fn currency_uses_a_comma() {
        let mut c = Context::new();
        c.insert("p", &29.99);
        assert_eq!(render("{{ p | currency }}", &c), "29,99 €");
    }

    #[test]
    fn truncate_cuts_on_a_word_boundary() {
        // The window is backed up to the last space inside it, so a word is never cut
        // in half — at length=16 the window is "un deux trois qu" and "qu" is dropped.
        let c = ctx_with("t", "un deux trois quatre cinq");
        assert_eq!(
            render("{{ t | truncate_text(length=16) }}", &c),
            "un deux trois…"
        );
        // At length=13 the window ends exactly on a space, which is itself the last
        // one, so the whole trailing word goes with it.
        assert_eq!(render("{{ t | truncate_text(length=13) }}", &c), "un deux…");
    }

    #[test]
    fn truncate_does_not_split_a_multibyte_char() {
        // Every char here is 2 bytes: byte-slicing at `length` lands mid-codepoint
        // and panics, which is what the Tera 1 version did.
        let c = ctx_with("t", "éééééééééé");
        assert_eq!(render("{{ t | truncate_text(length=4) }}", &c), "éééé…");
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        let c = ctx_with("t", "court");
        assert_eq!(render("{{ t | truncate_text(length=50) }}", &c), "court");
    }

    #[test]
    fn text_helpers() {
        let c = ctx_with("t", "Hello World & Co");
        assert_eq!(render("{{ t | slugify }}", &c), "hello-world-co");

        let c = ctx_with("t", "<p>bonjour</p>");
        assert_eq!(render("{{ t | strip_tags }}", &c), "bonjour");

        let c = ctx_with("t", "a\nb");
        assert_eq!(render("{{ t | nl2br }}", &c), "a<br>\nb");
    }

    #[test]
    fn html_templates_still_autoescape_filter_output() {
        let mut tera = Tera::new();
        register_all(&mut tera);
        tera.add_raw_template("t.html", "{{ t | nl2br }}").unwrap();
        let out = tera.render("t.html", &ctx_with("t", "a\nb")).unwrap();
        assert!(
            out.contains("&lt;br&gt;"),
            "filter output must stay escaped in .html templates, got: {out}"
        );
    }
}
