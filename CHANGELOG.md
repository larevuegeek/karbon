# Changelog

All notable changes to Karbon are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Versioning policy

Karbon is **pre-1.0**: minor versions (`0.x.0`) may contain breaking changes,
patch versions (`0.x.y`) are backwards-compatible fixes and additions. From
`1.0.0` onward the project will follow [Semantic Versioning](https://semver.org/)
strictly. Breaking changes are always listed under **Changed** / **Removed**.

## [0.3.0] - 2026-07-01

> Notable release: a security-hardening pass with several **breaking default changes**
> (see *Changed*). Per the pre-1.0 policy, breaking changes bump the **minor** version.

### Security (hardening pass)
- **Auth fails closed**: an empty/weak/placeholder `JWT_SECRET` no longer boots silently —
  `verify()` rejects all tokens when the secret is empty, and the app **refuses to start in
  production** with an empty/weak/placeholder secret (`is_weak_secret`, min 32 bytes).
- **CSRF is wired by default** (`App::serve`) — double-submit cookie **plus** a same-site
  `Origin`/`Referer` check; Bearer-token API requests are exempt. Opt out with `CSRF_ENABLED=false`.
- **Baseline security headers** on every response: `X-Content-Type-Options: nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy`, and HSTS over HTTPS.
- **SQL identifier validation**: `CrudRepository` (`find_all`/`find_where`/`has_many`/
  `load_grouped_by`, `ORDER BY` whitelisted to ASC/DESC) and `PaginatedQuery` now reject
  non-identifier columns — closing SQL-injection via dynamic sort/filter/search columns.
- **Error responses are secure-by-default**: internal detail (incl. raw `sqlx` errors) is
  masked unless `APP_ENV` is explicitly `development`/`test`.
- **Reverse proxy hardened**: strips hop-by-hop headers, rewrites `Host` to the upstream, and
  re-sets `X-Forwarded-Host/Proto/For` so the SSR frontend sees the public host (canonical URLs).
- **Rate limiter**: bounded map with eviction (memory-DoS fix) and keys on the socket peer IP
  (unspoofable), honoring `X-Forwarded-For` only from a configured `TRUSTED_PROXIES`.
- **WebSocket**: `origin_allowed` + `websocket_handler_checked` (anti Cross-Site WebSocket
  Hijacking) and per-connection message-size / room caps.
- **Uploads**: much stronger SVG sanitization (all `on*=` handlers, active elements/schemes,
  XXE), `ImgResizer` path-traversal fixed on both branches, `original_name` sanitized.
- **Deploy**: shell/ssh values are allowlist-validated (rejects spaces/quotes/`$`/leading `-`/`..`)
  across the publish path — closes command injection via `karbon.toml`.
- **Studio**: reachable only from loopback **or** with a valid (constant-time compared) token,
  even in dev; cookie `Secure` over HTTPS.
- **Macro enforcement**: a route carrying a role (`#[require_role]` or a controller-level
  `role`) **without an `auth: AuthGuard` parameter is now a compile error** (was silently
  unprotected).
- Misc: `Crypto::hash_token_keyed` (HMAC) + `tokens_match` (constant-time), bcrypt corrupted-hash
  vs wrong-password distinction + `needs_rehash`, `Regex::try_new` (no panic), bounded
  `X-Request-Id`, `AuthGuard::from_claims_with(hierarchy)`.

### Changed (breaking defaults — pre-1.0)
- **CORS defaults to deny** (was `*`). Same-origin apps are unaffected; set `CORS_ORIGINS` to
  allow cross-origin clients.
- **CSRF is on by default** — cookie-authenticated unsafe requests need a valid token or a
  same-site Origin (Bearer APIs unaffected).
- **Production refuses to start** with an empty/weak `JWT_SECRET`.
- **Error bodies are masked** unless `APP_ENV` is explicitly `development`/`test`.
- **`cargo build --all-features` no longer compiles**: DB drivers (`mysql`/`postgres`/`sqlite`)
  are mutually exclusive, now with a clear `compile_error!`. Build with exactly one driver.
- `App::serve` now takes `mut self` (source-compatible for the usual `App::new()…serve()`).

### Added
- **Firewall (Symfony-style `access_control`)**: `App::firewall(AccessControl::new()
  .public("^/login").rule("^/admin", ["ROLE_ADMIN"]).default_deny(false))` — declarative
  URL-pattern → roles, enforced centrally before handlers (first match wins), composing with the
  per-controller / per-route role guards and respecting the role hierarchy. Invalid patterns
  fail fast at startup.
- **Reverse-proxy production support**: `TRUSTED_PROXIES` (Symfony/Laravel style) — behind
  nginx/Cloudflare, `X-Forwarded-For/Proto/Host` are trusted for client IP, rate-limiting and
  SSR canonical URLs; ignored (safe) when Karbon is the edge.
- **Premium welcome/home page**: the generated home is now an onboarding checklist (steps
  auto-checked from the live app state) with copy-to-clipboard commands, restyled to the Karbon
  design system — for the Svelte, Next.js **and** micro (backend-only) skeletons.
- **Schema-diff migrations** (`karbon migrate diff [name]`, Doctrine-style): the **entities are
  the source of truth**. `migrate diff` introspects the live database *and* the entity files,
  compares them, and writes a migration for the difference — new tables → `CREATE TABLE`, new
  fields → `ALTER TABLE ADD COLUMN`. Additive and safe: it never drops or alters existing
  tables/columns (review the file before applying). Multi-driver (MySQL/Postgres/SQLite); driver
  detected from the connection URL. Re-running with no changes writes nothing ("schema up to date").
- **Interactive field assistant** (Symfony `make:entity`-style): `karbon generate entity/crud
  <Name>` with no fields (and an interactive stdin), or `-i`/`--interactive`, prompts for each
  field (name → type → nullable) and scaffolds from the answers. The Studio **Make** form
  gained a matching **field builder** (add/remove rows of name + type + nullable) that builds
  the same `generate` command — entities can be designed field-by-field from the browser too.
- **Declarative route validation** (Roadmap V2): path parameters can carry validation rules
  directly on the route — `#[karbon::get("/{id}", id = "int:min=1")]`. The `#[controller]`
  macro injects the check (right after the role guard), so invalid input is rejected with
  **HTTP 400** *before* the handler body runs. Rule grammar: `int[:min=N][:max=N]`,
  `string[:minlen=N][:maxlen=N]`, `slug`, `uuid`, `enum:a|b|c`, `regex:^…$` (compiled regexes
  are cached) — via `karbon::validation::route::validate`.
  Generated controllers and admin now use `id = "int:min=1"` on their `{id}` routes.
  (Typed extractors already validate the parameter *type*, and query builders parameterize
  values — this adds the business-rule layer.)
- **Studio Routes tab** (Roadmap V2, Vague 1.5): lists every available route by reading the
  app's `/openapi.json` (controller routes) plus the built-in system routes, **classified by
  what the route serves** — System (Axum/framework: `/health`, `/docs`, `/openapi.json`,
  `/_studio`), API (`/api/*`, JSON) and Web (server-rendered HTML, incl. admin) — with a
  colored kind badge, kind filters, a search box, and the **path parameters with their type
  and constraints** (e.g. `id:integer ≥1`).
- **OpenAPI path-parameter constraints**: path params now carry sensible default constraints
  in the spec — integer ids get `format: int64, minimum: 1`, string params `minLength: 1`.
- **Docs reachable + recursive build** (Roadmap V2, Vague 1.5): the generated welcome page
  links to **API (Swagger `/docs`)**, the **Docs** tab (`/_studio#docs`) and **Studio**; and
  `karbon docs build` now recurses sub-directories (`docs/**`, skipping `_site/`), so nested
  guides become pages (`guides/setup.md` → `guides-setup.html`).
- **Richer OpenAPI** (Roadmap V2, Vague 1.5): `openapi::spec` now emits **path parameters**
  (`/posts/{id}` → an `id` path param typed `integer`), **tags** derived from the path
  (Swagger groups operations by resource — `posts`, `admin`…), plus `summary` and
  `operationId`. Makes the generated `/docs` (Swagger UI) actually usable.
- **Field-aware `generate admin`** (Roadmap V2, Vague 1.5): the admin generator now reads the
  entity's actual fields and drives the list table, the form (`build_form`) and the
  `New`/`Update` construction from them, with per-type coercion of submitted form values
  (int/bigint/float `parse`, bool checkbox, datetime/date/json, nullable → `Option`). It no
  longer assumes `title`/`slug`, so admins work for any entity. Covered by the compile e2e.
- **`karbon dev` hot-reloads the backend** (Roadmap V2, Vague 1.5): a file watcher on
  `app/src` recompiles and restarts the backend binary on save (the frontend keeps its Vite
  HMR). Previously `dev` compiled once and ran the binary, so code generated via the Maker /
  terminal / `generate` needed a manual restart to take effect. A failed rebuild keeps the
  session alive and retries on the next save.
- **Custom entity fields** (Roadmap V2, Vague 1.5): `karbon generate entity/crud Post
  title:string body:text views:int published:bool summary:string?` now generates the entity
  struct, `New`/`Update` DTOs and migration from the given fields (types: `string` text int
  bigint float bool datetime date json, `?` = nullable) instead of a fixed `title`/`slug`.
  No fields → the previous `title`/`slug` default (backwards-compatible).
- **Multi-driver generators**: generated **migrations** use driver-correct DDL (MySQL
  backticks/`AUTO_INCREMENT`/`ON UPDATE`/`ENGINE`, Postgres `"…"`/`BIGSERIAL`/`TIMESTAMP`,
  SQLite `AUTOINCREMENT`), and generated **repositories** use the abstract
  `karbon::db::DbPool` instead of a hardcoded `MySqlPool` — so `generate crud` now compiles
  on Postgres/SQLite projects, not just MySQL.
- **Swagger UI & auto `/openapi.json`** (Roadmap V2, Pilier C): generated projects now serve
  `/openapi.json` (aggregated from the controllers — generators append each controller's
  `openapi_paths()` at `// karbon:openapi-api` / `// karbon:openapi-root` markers, prefixing
  API controllers with `/api/v1` and mounting admin paths at the root) and `/docs`
  (Swagger UI via the new `karbon::openapi::swagger_ui_html`). Studio's Docs tab links to
  both; the Vite dev proxy forwards `/docs` and `/openapi.json`.
- **Studio embedded docs** (Roadmap V2, Pilier C): a Docs tab renders the bundled **Karbon
  reference** plus the project's own `docs/*.md` (same Markdown sources as `karbon docs
  build`), live in dev. Served by `GET /_studio/api/docs` (Markdown → HTML via
  `pulldown-cmark`, behind the `studio` feature).
- **`karbon docs build`** (Roadmap V2, Pilier C): renders a project's `docs/*.md` into a
  self-contained static HTML site under `docs/_site/` (sidebar nav, dark azure/violet theme,
  tables/code/task-lists via `pulldown-cmark`). The page title comes from each file's first
  `# heading`; `index.md` becomes the landing page (auto-generated listing otherwise). The
  same Markdown sources will back Studio's embedded docs.
- **Studio terminal** (Roadmap V2, Pilier B): a Terminal tab in the dashboard runs a
  **whitelisted** set of `karbon` sub-commands (`generate`/`g`, `migrate`, `doctor`) and
  shows their output, with quick-action chips. Dev-only, token-protected, no shell (args
  passed literally; `dev`/`build`/`serve` are not allowed). Backed by
  `POST /_studio/api/terminal`; the binary is resolved from `KARBON_BIN` (set by
  `karbon dev`) with a `karbon`-on-PATH fallback. The debug toolbar drop-up gained a
  `Terminal →` deep-link (`/_studio#terminal`). The tab also includes a **Maker** form
  (kind + name + dry-run/force) that scaffolds an entity/crud/controller/admin from the
  browser without typing a command, a **command catalog** (every runnable command with a
  one-line description — click to use) and **autocomplete** on the input. `docs` is
  whitelisted too.
- **Starter `docs/index.md`**: `karbon new` now scaffolds a `docs/` guide, so `karbon docs
  build` and Studio's Docs tab work out of the box.
- **`karbon doctor`** (Roadmap V2, Pilier A): offline project diagnostics that catch the
  confusing failures before they bite — `karbon` resolved from crates.io instead of a
  local path dep in dev, route auto-wiring markers missing, controllers generated but not
  mounted in `main.rs`, `generate admin` without its entity, database not configured,
  duplicate migration numbers, and an incomplete Vite dev proxy. Each finding prints an
  actionable fix. Never connects to the database.
- **Publishing guide** (`docs/PUBLISHING.md`): the crates.io release process (order,
  version bump, dry-run, post-publish). All three crates are already published at `0.2.30`.
- **CLI end-to-end tests** (Roadmap V2, Pilier A): `karbon-cli/tests/scaffold.rs` scaffolds
  projects and asserts their structure (auto-wiring markers, `--local` path dep, SvelteKit
  `app.html`, complete Vite proxy), runs `doctor`, and exercises the generators (migration
  numbering, `--dry-run`). A heavier `#[ignore]`d test scaffolds a project and **compiles it
  against the local framework** (macros + ORM + route auto-wiring end-to-end), run by a new
  `cli-e2e` CI job.
- **Robust generators** (Roadmap V2, Pilier A): `karbon generate` gained `--dry-run`
  (print the plan, write nothing) and `--force` (overwrite existing files instead of
  skipping them — by default an existing file is now left untouched rather than silently
  clobbered). Migration numbering is now collision-proof (highest existing `NNNN_` prefix
  + 1, robust to deleted files) and idempotent (a `create_<table>` migration is not emitted
  twice).
- **Route auto-wiring**: `generate controller`/`crud`/`admin` now mount the generated
  controller in `main.rs` automatically (via `// karbon:routes` / `// karbon:api-routes`
  markers in the skeleton), instead of only printing a hint. API controllers land under
  `/api/v1`, admin/server-rendered controllers at the root. The Vite dev proxy also
  forwards `/admin` (and `/health`, `/_studio`) so generated routes are reachable in dev.
- **Studio toolbar & dashboard, profiler-style**: the debug toolbar now shows the Karbon
  version and a hover **drop-up** with framework info (version, environment, enabled
  features) + the current request; its "Studio" link now carries the auth token (fixes
  the 403). The Studio dashboard gains an **App** tab listing version/env/features and the
  **detected database tables (entities)**.
- **Frontend welcome page is now a live mini-profiler**: the generated SvelteKit home page
  fetches `/_studio/api/info` and shows the framework version, environment, enabled features
  and detected entities in a hover drop-up, with working `/health` and `/_studio` links
  (relative, proxied by Vite — added `/_studio` and `/health` to the dev proxy). Falls back
  to a static bar when Studio is off (production).
- **Visual overhaul (azure/violet dark theme)**: the welcome page and Studio now share a
  modern dark UI with an azure→violet gradient identity and a new **hexagon "K" logo**
  (carbon-ring motif, faithful to the *Karbon* name). Studio is laid out in a **centered
  container** with glassy cards, a workspace panel, version/environment pills in the topbar,
  and refined tables/stat cards. The backend debug toolbar matches the same palette.
- **Studio Overview tab** (profiler home, à la Symfony Web Profiler): framework summary
  (version, environment, DB driver, uptime), live **performance metrics** computed
  client-side (total/error-rate, avg/min/max latency, **status & method distribution**,
  **slowest endpoints**), enabled features and detected entities, plus quick links.
- **Studio Database tab** (schema browser, à la Telescope): lists tables with **row counts**
  and, per table, **columns and their types** — multi-driver introspection (MySQL/Postgres
  via `information_schema`, SQLite via `PRAGMA`), exposed through `GET /_studio/api/database`.
- **Native template engine** (`native-templates` feature): `template::NativeEngine`, a
  dependency-free Jinja/Twig-subset engine (lexer → parser → AST → render) implementing
  `Renderer` — variables/paths, `if/elif/else`, `for` with `loop.*`, filters, auto-escaping
  with `safe`/`raw`, comments, **template inheritance** (`extends`/`block`) and `include`.
- **OpenAPI generation**: `#[controller]` now also generates `openapi_paths()`, and
  `openapi::spec(title, version, &routes)` builds a minimal OpenAPI 3.0 document.
- **`karbon new --template blog`**: scaffolds a Post CRUD + admin on top of the skeleton.
- **Bundle system**: `http::Bundle` trait + `App::bundle()` — compile-time plugins that
  contribute routes and a `boot(&AppState)` startup hook.
- **Persistent jobs**: `job::PersistentQueue` — DB-backed (`_karbon_jobs`) job queue with
  serializable jobs (`PersistentJob`), polling worker, retry and dead-letter logging.
- **Cache**: Redis backend (`RedisStore`, `redis` feature) + `Cache::redis()`; tag-based
  invalidation (`set_tagged` / `invalidate_tag`); `http_cache` middleware (ETag + 304).
- **HTML sanitization**: `util::Html::sanitize` / `strip_tags` (via `ammonia`).
- **CGI adapter** (`cgi` feature): `cgi::serve_cgi` runs a router under classic CGI
  (best-effort shared-hosting support; unproven on a real host).
- **Scraping toolkit** (`scraping` feature): `Scraper` (HTTP client with throttling
  and user-agent) + `Document` (CSS-selector parsing via `scraper`), a `robots.txt`
  check, and a `scrape()` helper that keeps the non-`Send` document off `.await` points.
- **SQLite driver** (`sqlite` feature): third database driver alongside MySQL/Postgres
  (file path or `:memory:`). `karbon migrate` also handles SQLite.
- **EXIF auto-orientation** in `storage::ImageProcessor`: image orientation is applied
  on load by default (phone-camera photos display upright); `keep_orientation()` opts out.
- **`karbon generate admin <Entity>`**: scaffolds an auto-admin CRUD controller
  (list + create/edit forms + delete) built on the form system, with `ROLE_ADMIN`
  auth and CSRF. Generators now also wire `entity`/`repository`/`controller` modules
  into `main.rs`.
- **Form builder** (`form::Form` / `form::Field`, à la Symfony Forms): typed field
  kinds, data binding, validation (via the constraint traits), HTML rendering with
  escaping and a CSRF field.
- **ORM relations** on `CrudRepository`: `has_many`, belongs-to via `find_by_id`, and
  `load_grouped_by` for N+1-free batch loading of related rows.
- **Template abstraction** (`template::Renderer`): backend-agnostic rendering trait
  implemented by the Tera engine (`templates` feature) and a new minijinja engine
  (`template::MinijinjaEngine`, `minijinja` feature). Lets app code depend on
  `Arc<dyn Renderer>` and swap backends; foundation for a future in-house engine.
- **Composable validator** (`validation::Validator` + `ValidationErrors`): collects
  every field violation (not fail-fast), with nested-object validation, ad-hoc
  `check`s and validation groups, built on the existing constraint traits.
- **Pluggable cache** (`cache::CacheStore`): `MemoryStore` and `FileStore` backends
  behind a typed `Cache` facade, with TTL and a `remember()` cache-aside helper.
- **Message bus** (`message::MessageBus`, à la Symfony Messenger): fallible handlers,
  synchronous (`dispatch`) and background (`dispatch_async`) transports, retry/backoff
  (`RetryPolicy`) and dead-letter logging. Generalizes `EventBus` and `JobQueue`
  (both kept for backwards compatibility).
- **Versioned, native migrations**: `karbon migrate` now tracks applied migrations
  in a `_karbon_migrations` table (each runs once), with `karbon migrate status`
  and `karbon migrate rollback`. Migrations support `-- migrate:up` / `-- migrate:down`
  sections. Executed **natively via sqlx** (the `Any` driver) — no more dependency on
  the external `mysql`/`psql` CLIs. `generate entity/crud` now emits up/down migrations.
- **Kernel modules**: new `http::Module` trait + `App::module()` to register
  self-contained units of routes (nested via `prefix()` or merged at the root).
  Backwards-compatible extension seam — the foundation for the future bundle system.
- **Debug toolbar** (Web Profiler): with the `studio` feature in dev, a toolbar is
  injected at the bottom of HTML responses (method, path, status, duration, request-id,
  link to Studio) via `studio_toolbar_middleware`.
- **Environment system** (`APP_ENV`) with Symfony-style cascading `.env` loading
  (`.env` → `.env.{env}` → `.env.local` → `.env.{env}.local`), exposed via
  `config::load_env()` and `config::Environment`. New `Config::is_development()`
  / `Config::is_test()` / `Config::environment()` helpers.
- **`karbon deploy paas`** — generates Fly.io (`fly.toml`), Render (`render.yaml`)
  and `Procfile` configuration (plus a `Dockerfile` if missing).
- **`karbon deploy fly` / `karbon deploy railway`** — convenience wrappers that
  ensure the PaaS config exists, then shell out to `flyctl deploy` / `railway up`
  (with a helpful message if the platform CLI is not installed).
- Generated Dockerfiles now print a **multi-arch buildx** command (amd64 + arm64).
- **Deployment guide** (`docs/DEPLOY.md`) covering VPS, Docker, PaaS and environments.
- **CI** (GitHub Actions): build + test matrix over the `mysql` and `postgres`
  drivers, plus a **strict lint gate** (`cargo fmt --check` + `clippy -D warnings`
  on mysql and postgres/studio/templates).
- **docs.rs metadata** and crate-level documentation for `karbon-framework`.
- `HEALTHCHECK` and `APP_ENV=production` in the generated Dockerfile; richer
  `.dockerignore`.

### Changed
- **`generate entity`/`crud` no longer writes a migration**: the entity is the schema source of
  truth — run **`karbon migrate diff`** to generate the migration from the entity ↔ DB diff.
  Migrations are no longer coupled to entity creation.
- **Studio terminal** asks for confirmation before running a destructive `migrate rollback`.
- **`karbon doctor` now exits non-zero on failure** (so it's usable in CI / the Studio
  terminal) and gained **`--db`**: an opt-in online check that connects to the database
  (sqlx `Any` driver) and reports connectivity + pending migrations.
- Generated **stub controllers** (`generate controller`) no longer take a `State` extractor
  (a stub doesn't use it); a comment shows how to add it. Avoids unused-variable warnings.
- **Generated projects now pin `karbon = "0.2"`** (was `"0.1"`, which resolved to the
  ancient `0.1.2` on crates.io). A `karbon new` without `--local` now pulls the current
  published framework. Also bumped the `karbon-macros` dependency requirement in
  `karbon-framework` from `"0.2.5"` to `"0.2"` (any `0.2.x`).
- **Database is now optional**: if `DB_NAME` is empty the app starts without connecting
  (a lazy pool is created but never used), so projects that don't need a database just run.
  `DB_USER` and `JWT_SECRET` are no longer required either (empty `JWT_SECRET` disables
  auth with a startup warning). `Config::has_database()` reports the mode.
- `validation::Constraint` / `NumericConstraint` / `CollectionConstraint` are now
  `Send + Sync`, so boxed constraints (e.g. inside a `Form`) can be held across
  `.await` in handlers.
- Generated projects and macros now consistently use the `karbon::` path. The
  CLI templates and `karbon generate` output import the crate as
  `karbon = { package = "karbon-framework" }` (was the `framework` alias), matching
  the code emitted by the macros. **This fixes generated projects that previously
  failed to compile.**

### Fixed
- **Studio terminal hung on `generate entity/crud` without fields**: a TTY inherited through
  `karbon dev → backend → karbon` made the new field assistant prompt for input that never
  came. The terminal now runs commands with **stdin closed**, so generators stay
  non-interactive there (no fields → `title`/`slug` default); use the **Make** field builder
  or inline `name:type` specs to set fields from the browser.
- **Noisy `[vite] ws proxy socket error: ECONNRESET`**: the Vite dev proxy now swallows the
  benign `ECONNRESET` that occurs when Studio's WebSocket drops as the backend recompiles
  (the `karbon dev` watcher) — the Studio client reconnects automatically. Other proxy errors
  are still logged.
- **Studio "Disconnected" when opened via the frontend port**: the Vite dev proxy now
  forwards WebSockets for `/_studio` (`ws: true`), so the live dashboard connects when Studio
  is reached through the frontend (e.g. the welcome page / toolbar links) and not only on the
  backend port.
- **Studio terminal**: `docs` is now whitelisted, so `karbon docs build` runs from the
  terminal (with a quick-action chip).
- **Studio 403 from non-tokened links** (debug toolbar, welcome page): in development the
  dashboard skips the random token (it is already dev-only and localhost-bound). Token
  enforcement stays on as soon as `APP_ENV` is production-like.
- Generated SvelteKit skeleton was missing `src/app.html` (required by SvelteKit) — added,
  so `vite dev` / `svelte-kit sync` work out of the box.
- Generated `.env` no longer pre-fills `DB_NAME`, so a new project runs without a database
  by default; removed an unused `ServeDir` import from the generated `main.rs`.
- Broken doctests in `storage::img_resizer` / `storage::thumbnail` (missing imports)
  are now marked `ignore`, so `cargo test` passes cleanly.
- Compile error when building `karbon-framework` with the `studio` feature
  (immutable `router` reassignment) — would also have broken the docs.rs build.
- Whole codebase formatted with `rustfmt` and all `clippy` warnings resolved.

## [0.2.30]

### Changed
- Updated dependencies to their latest compatible versions (`cargo update`):
  axum, hyper, reqwest, jsonwebtoken, lettre, rustls and ~80 transitive crates.
