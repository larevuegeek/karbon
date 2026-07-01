# Karbon website

The standalone marketing site **and** full documentation for Karbon. Pure static
HTML/CSS/JS — no build step, no dependencies. Open `index.html` directly or deploy the
`site/` folder as-is to any static host (GitHub Pages, Netlify, Cloudflare Pages, …).

## Structure

```
site/
├── index.html          # landing page (hero, features, code, editions, CTA)
├── assets/
│   ├── site.css        # design system (landing + docs)
│   └── docs.js         # docs shell — injects nav, sidebar, TOC, pager, search
└── docs/
    ├── index.html      # Introduction
    ├── installation.html · quickstart.html · structure.html
    ├── cli.html · routing.html · database.html · migrations.html · validation.html
    ├── generators.html · security.html · realtime.html · frontend.html
    └── studio.html · configuration.html · deployment.html
```

## How the docs pages work

Each docs page contains only its content inside
`<article id="doc" data-title="…" data-cat="…">`. On load, `assets/docs.js` wraps it with
the shared chrome (top nav, left sidebar, right table-of-contents, prev/next pager, footer)
and wires search + scrollspy. The navigation order lives in the `NAV` array at the top of
`docs.js` — **add a page there** and create the matching `docs/<slug>.html`.

To add a page:
1. Copy any `docs/*.html`, change `data-title`, `data-cat`, the `<h1>` and the content.
2. Add `['<slug>.html', 'Title']` to the right group in the `NAV` array in `docs.js`.

## Preview locally

Just open the file — everything is relative:

```bash
# Windows
start site/index.html
# or serve it
python -m http.server -d site 8080   # then http://localhost:8080
```

## Deploy

Point your static host at the `site/` directory. A ready-to-use GitHub Pages workflow is
provided at `.github/workflows/pages.yml` — enable Pages (Settings → Pages → Source:
GitHub Actions) and it publishes `site/` on every push to `main`.

## Editing content

- External links assume the repo lives at `github.com/larevuegeek/karbon` and the crates are
  `karbon-framework` / `karbon-cli`. Update them if the public URLs differ.
- The brand colors and components are defined once in `assets/site.css` (`:root` variables).
