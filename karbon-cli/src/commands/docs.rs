use colored::Colorize;
use pulldown_cmark::{Options, Parser, html};
use std::fs;
use std::path::{Path, PathBuf};

/// `karbon docs <action>` — `build` (default) renders `docs/*.md` into a static
/// HTML site under `docs/_site/`. The same Markdown sources back Studio's embedded
/// docs (`/_studio/docs`), so the in-app docs and the published site stay in sync.
pub fn run(root: &Path, action: &str) -> Result<(), String> {
    match action {
        "build" | "" => build(root),
        other => Err(format!("Unknown docs action '{other}'. Available: build")),
    }
}

struct Page {
    slug: String,
    title: String,
    path: PathBuf,
}

fn build(root: &Path) -> Result<(), String> {
    println!("\n{}  docs build\n", "▲ karbon".bold().red());

    let docs_dir = root.join("docs");
    if !docs_dir.exists() {
        return Err(
            "No docs/ directory found. Create docs/*.md guides (e.g. docs/index.md).".to_string(),
        );
    }

    let mut pages = collect_pages(&docs_dir)?;
    if pages.is_empty() {
        return Err("No .md files found in docs/".to_string());
    }
    // index first, then alphabetical.
    pages.sort_by(|a, b| match (a.slug.as_str(), b.slug.as_str()) {
        ("index", _) => std::cmp::Ordering::Less,
        (_, "index") => std::cmp::Ordering::Greater,
        _ => a.title.cmp(&b.title),
    });

    let out = docs_dir.join("_site");
    fs::create_dir_all(&out).map_err(|e| format!("Cannot create docs/_site: {e}"))?;

    for page in &pages {
        let md = fs::read_to_string(&page.path)
            .map_err(|e| format!("Cannot read {}: {e}", page.path.display()))?;
        let body = render_markdown(&md);
        let html = layout(&page.title, &page.slug, &pages, &body);
        let dest = out.join(format!("{}.html", page.slug));
        fs::write(&dest, html).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;
        println!("  {} docs/_site/{}.html", "✓".green(), page.slug);
    }

    // Ensure there is an index.html landing page.
    if !pages.iter().any(|p| p.slug == "index") {
        let listing = pages
            .iter()
            .map(|p| format!("<li><a href=\"{}.html\">{}</a></li>", p.slug, esc(&p.title)))
            .collect::<String>();
        let body = format!("<h1>Documentation</h1>\n<ul class=\"kd-index\">{listing}</ul>");
        let html = layout("Documentation", "", &pages, &body);
        fs::write(out.join("index.html"), html).map_err(|e| format!("Cannot write index: {e}"))?;
        println!("  {} docs/_site/index.html (généré)", "✓".green());
    }

    println!(
        "\n  {} {} page(s) → {}\n",
        "✓".green().bold(),
        pages.len(),
        "docs/_site/".cyan()
    );
    Ok(())
}

fn collect_pages(dir: &Path) -> Result<Vec<Page>, String> {
    let mut pages = Vec::new();
    collect_into(dir, dir, &mut pages)?;
    Ok(pages)
}

/// Recurse `docs/` (skipping the generated `_site/`), turning each `*.md` into a
/// page whose slug encodes its relative path (`guides/setup.md` → `guides-setup`).
fn collect_into(base: &Path, dir: &Path, pages: &mut Vec<Page>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
            if name.as_deref() == Some("_site") {
                continue;
            }
            collect_into(base, &path, pages)?;
        } else if path.extension().is_some_and(|x| x == "md") {
            let rel = path.strip_prefix(base).unwrap_or(&path).with_extension("");
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let slug = slugify(&rel_str.replace('/', "-"));
            let md = fs::read_to_string(&path).unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let title = first_heading(&md).unwrap_or_else(|| humanize(&stem));
            pages.push(Page { slug, title, path });
        }
    }
    Ok(())
}

fn render_markdown(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// Full HTML page: sidebar nav (current page highlighted) + rendered content.
fn layout(title: &str, current: &str, pages: &[Page], body: &str) -> String {
    let nav = pages
        .iter()
        .map(|p| {
            let active = if p.slug == current {
                " class=\"active\""
            } else {
                ""
            };
            format!("<a href=\"{}.html\"{active}>{}</a>", p.slug, esc(&p.title))
        })
        .collect::<String>();

    format!(
        r#"<!doctype html><html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — Karbon docs</title>
<style>
:root{{--vio:#8b7cff;--azu:#22d3ee;--bg:#0b0d18;--bg2:#11142a;--bd:#242a44;--tx:#e8eaf2;--mut:#a6acc4}}
*{{box-sizing:border-box}}body{{margin:0;font:15px/1.65 -apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;color:var(--tx);background:radial-gradient(900px 500px at 10% -10%,rgba(139,124,255,.14),transparent 60%),linear-gradient(180deg,#0a0c16,var(--bg))}}
.kd{{display:flex;max-width:1100px;margin:0 auto;min-height:100vh}}
.kd-side{{width:250px;flex-shrink:0;padding:26px 18px;border-right:1px solid var(--bd);position:sticky;top:0;height:100vh;overflow:auto}}
.kd-brand{{display:flex;align-items:center;gap:9px;font-weight:700;font-size:17px;margin-bottom:20px}}
.kd-brand b{{background:linear-gradient(120deg,var(--vio),var(--azu));-webkit-background-clip:text;background-clip:text;-webkit-text-fill-color:transparent}}
.kd-side a{{display:block;padding:7px 11px;border-radius:8px;color:var(--mut);text-decoration:none;font-size:14px}}
.kd-side a:hover{{background:#1a1f33;color:var(--tx)}}
.kd-side a.active{{background:linear-gradient(135deg,rgba(139,124,255,.18),rgba(34,211,238,.1));color:#c7c0ff}}
.kd-main{{flex:1;min-width:0;padding:40px 44px}}
.kd-main h1,.kd-main h2,.kd-main h3{{letter-spacing:-.01em;line-height:1.25}}
.kd-main h1{{font-size:2rem;margin:.2em 0 .6em}}
.kd-main h2{{font-size:1.4rem;margin-top:1.6em;border-bottom:1px solid var(--bd);padding-bottom:.3em}}
.kd-main a{{color:#a99bff}}
.kd-main code{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.88em;background:rgba(139,124,255,.12);border:1px solid rgba(139,124,255,.2);border-radius:6px;padding:.1em .4em;color:#c7c0ff}}
.kd-main pre{{background:#08090f;border:1px solid var(--bd);border-radius:12px;padding:16px;overflow:auto}}
.kd-main pre code{{background:none;border:0;padding:0;color:#d7dbe0}}
.kd-main table{{border-collapse:collapse;width:100%;margin:1em 0}}
.kd-main th,.kd-main td{{border:1px solid var(--bd);padding:7px 11px;text-align:left}}
.kd-main blockquote{{border-left:3px solid var(--vio);margin:1em 0;padding:.2em 1em;color:var(--mut);background:#11142a;border-radius:0 8px 8px 0}}
.kd-index a{{color:#a99bff}}
@media(max-width:760px){{.kd{{flex-direction:column}}.kd-side{{width:auto;height:auto;position:static;border-right:0;border-bottom:1px solid var(--bd)}}}}
</style></head><body><div class="kd">
<nav class="kd-side"><div class="kd-brand">◆ <b>Karbon</b> docs</div>{nav}</nav>
<main class="kd-main">{body}</main>
</div></body></html>"#,
        title = esc(title),
    )
}

// ── Helpers ──

fn first_heading(md: &str) -> Option<String> {
    md.lines().find_map(|l| {
        let t = l.trim();
        t.strip_prefix("# ").map(|h| h.trim().to_string())
    })
}

fn humanize(stem: &str) -> String {
    let s = stem.replace(['-', '_'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

fn slugify(stem: &str) -> String {
    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_heading() {
        assert_eq!(first_heading("# Hello\n\ntext").as_deref(), Some("Hello"));
        assert_eq!(first_heading("no heading"), None);
    }

    #[test]
    fn slugify_and_humanize() {
        assert_eq!(slugify("Getting Started"), "getting-started");
        assert_eq!(humanize("getting-started"), "Getting started");
    }

    #[test]
    fn renders_markdown_to_html() {
        let html = render_markdown("# Title\n\n- a\n- b");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<li>a</li>"));
    }
}
