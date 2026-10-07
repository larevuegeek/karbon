# Changelog

All notable changes to Karbon are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Versioning policy

Karbon is **pre-1.0**: minor versions (`0.x.0`) may contain breaking changes,
patch versions (`0.x.y`) are backwards-compatible fixes and additions. From
`1.0.0` onward the project will follow [Semantic Versioning](https://semver.org/)
strictly. Breaking changes are always listed under **Changed** / **Removed**.

## [0.4.2] - 2026-10-07

> Security release for `ImgResizer`. Every distinct resize URL cost a full decode and encode
> plus a new cache file, with no limit on concurrency or on the number of variants, so
> anyone could exhaust CPU and disk by iterating sizes. It also served the original file
> whenever processing failed — which happened for every PNG saved under a `.jpg` name —
> and its WebP output ignored the quality setting.

### Security
- **Unbounded resize variants (CPU and disk exhaustion).** Processing now goes through a
  semaphore (`max_concurrent`, default half the CPU cores; a request waits up to 15 s, then
  gets `503 Retry-After`), each source image keeps at most `max_variants_per_file` cached
  variants (default 48; further new variants get `400`), and `allowed_specs` can restrict
  URLs to a fixed list.
- **Cache-busting through equivalent spellings.** Unknown modifiers were ignored, so
  `320x180_a`, `320x180_b`… were all new URLs for the same image. Unknown modifiers,
  non-integer blur and unsupported output formats are now rejected, and equivalent spellings
  (`_fit`, `q075`, reordered modifiers, `_grayscale`) get a `308` to the canonical URL.
- **Crop anchor missing from the cache key.** `?anchor=` changed the crop but not the cache
  file, so the first visitor decided the crop everybody got. The anchor is now part of the
  key, and unknown anchors are rejected instead of silently meaning "center".
- **Hidden files.** The `ServeDir` fallback served dotfiles (`.env`, `.git/`); any path
  segment starting with `.` now gets `404`.
- **Repeated failures.** A source that fails to process is recorded next to its variants
  and not decoded again until it changes.

### Fixed
- **Images whose extension lies about their format were never resized.** The processor
  picked the decoder from the file extension, so a PNG named `.jpg` failed and the original
  (often several MB) was served in its place. The format is now detected from the content.
- **WebP ignored the quality setting.** The `image` crate only encodes lossless WebP, which
  made `_q75.webp` variants several times heavier than JPEG. WebP is now encoded lossy
  through libwebp (`ImageProcessor::webp_quality`); `webp()` keeps lossless output.

### Changed
- **Cache layout** is now `cache_dir/<source path>/<spec>[@anchor].<ext>`. Existing cache
  entries are not reused; delete the old cache directory after upgrading.
- An invalid spec now answers `400` instead of serving the original.
- New dependency `webp` (bundles libwebp, built with the system C compiler).

## [0.4.1] - 2026-10-07

> Fix release: `#[derive(Updatable)]` did not compile against `sqlx` 0.9, so every
> application using it failed to build after upgrading to 0.4.0.

### Fixed
- **`Updatable` passed a `String` to `sqlx::query()`**, which 0.9 rejects ("dynamic SQL
  strings should be audited"). The generated query is now wrapped in `AssertSqlSafe`: the
  table and column names come from the struct at compile time and every value is bound.
  `Insertable` was unaffected (its SQL is a literal).

## [0.4.0] - 2026-10-06

> Dependency release: every major that had been held back is taken at once — `sqlx` 0.9,
> `jsonwebtoken` 11, `argon2` 0.6, `tower-http` 0.7, `tera` 2, `redis` 1.7.
>
> **This is a breaking release for applications.** `sqlx` types cross Karbon's public API
> (`DbPool`, `DbRow`, `DbArguments`, `CrudRepository: FromRow`), and a generated project
> declares `sqlx` itself — so an application must move to `sqlx` 0.9 in the same step.
> Staying on 0.8 compiles two copies of the crate and yields unrelated-looking type
> errors on `DbPool`. The same applies to `tower-http` 0.7.
>
> Stored password hashes and already-issued JWTs are unaffected; see below.

### Changed
- **`sqlx` 0.8 → 0.9.** Three consequences for application code:
  - The combined runtime+TLS features are gone: `runtime-tokio-rustls` becomes
    `runtime-tokio` + `tls-rustls`. A project that keeps the old name fails to *resolve*,
    before any compilation.
  - `query*()` now takes `impl SqlSafeStr`, accepting only `&'static str` or an explicit
    `AssertSqlSafe(…)`. Karbon's own dynamic SQL has been audited and wrapped; identifiers
    still go through `is_valid_identifier()` / `normalize_direction()` as before.
  - The `Arguments` associated type lost its lifetime parameter, so the public
    `DbArguments` alias changes shape (`SqliteArguments<'static>` → `SqliteArguments`).
- **`argon2` 0.5 → 0.6.** `SaltString` and `rand_core` left `password_hash`, `PasswordHash`
  moved under `phc::`, and `hash_password()` now generates the salt itself. **Hashes written
  by earlier releases still verify** — covered by a regression test built from a hash
  produced by argon2 0.5.
- **`tera` 1 → 2** (feature `templates`). Filters receive the already-coerced argument plus
  `Kwargs` and `&State` instead of `&Value` and a `HashMap`, and may return a plain
  `String`. Any application-defined filter must be ported the same way. Template loading is
  now `Tera::new()` + `load_from_glob()`, which requires Tera's non-default `glob_fs`
  feature. Auto-escaping is unchanged (`.html`, `.htm`, `.xml`), and no Karbon filter
  declares itself safe, so filter output stays escaped.
- **`redis` 0.27 → 1.7** (feature `redis`), **`jsonwebtoken` 10 → 11**,
  **`tower-http` 0.6 → 0.7**.
- Generated projects now pin `sqlx` 0.9 and `tower-http` 0.7 to match the framework.

### Fixed
- **`truncate_text` panicked on accented text.** The filter sliced by byte offset using a
  character count, so any multi-byte character at the cut position split a codepoint —
  reachable with ordinary French text. It now counts characters.
- **Redis `clear()` could push a scan error as if it were a key.** `next_item()` returns a
  `Result` in redis 1.x; a mid-scan failure now aborts instead of clearing a partial
  namespace.
- The PostgreSQL and SQLite branches of `InsertBuilder` and the `DbArguments` alias were
  only reachable under their own `cfg`, so a default `cargo check` never compiled them.

## [0.3.9] - 2026-09-11

> Additive release: a token can now say who is really acting when an administrator works
> inside someone else's account. Tokens issued by earlier releases keep decoding.

### Added
- **`Claims::impersonator_id`** and **`JwtManager::generate_with_impersonator`** — the
  token of a support session records the administrator behind it. Until now such a token
  was byte-for-byte the customer's own, so every action taken with it was logged under the
  customer's name and the administrator was invisible. The field is optional, omitted when
  empty, and decoded with a default: every token minted before this release is still
  accepted. `generate` and `generate_full` keep their signatures and set no impersonator.
- **`AuthGuard::impersonator_id()`** — reads it back in a handler.

### Changed
- Code that builds `Claims { .. }` with a struct literal must set `impersonator_id`
  (normally `None`). Tokens produced through `JwtManager` are unaffected.

## [0.3.8] - 2026-09-08

> Fix release: the CSRF `Origin` check rejected legitimate requests behind a reverse
> proxy — including Karbon's own. Backwards-compatible; a deployment that was working
> keeps working, and one that was returning `403 Cross-site request rejected` stops.

### Fixed
- **`403 Cross-site request rejected` behind a reverse proxy.** The CSRF middleware
  compared `Origin` against the raw `Host` header, which behind a proxy is whatever that
  proxy sent upstream, not what the browser addressed. nginx's default is
  `proxy_set_header Host $proxy_host` — the *upstream* name — so a stock nginx in front of
  Karbon turned every cookie-authenticated POST/PUT/PATCH/DELETE into a 403. Vite's dev
  proxy does the same: its string shorthand implies `changeOrigin: true`, rewriting `Host`
  to the backend while forwarding the browser's `Origin` untouched. The check now resolves
  the public host from `X-Forwarded-Host`, and only when the direct peer is one of
  `TRUSTED_PROXIES` — an untrusted client cannot forge it and walk past the same-origin
  rule. Karbon's own proxy (`http::proxy`) already set that header; the middleware simply
  was not reading it, while it did honor `X-Forwarded-Proto` a few lines above for the
  cookie's `Secure` flag.
- **`CORS_ORIGINS` had no effect on unsafe methods.** The middleware never read it, so an
  origin the deployment explicitly allowed still could not POST — the setting promised
  something it did not deliver. Declared origins now pass the `Origin` check. The
  double-submit token requirement is unchanged, so this relaxes defense-in-depth only.
  `CORS_ORIGINS=*` deliberately grants nothing here: it is a CORS wildcard (browsers
  refuse to send credentials to it anyway), not a statement that every site on the web may
  act on behalf of a logged-in user.

### Added
- **`middleware::CsrfConfig` and `middleware::csrf_protection_with`** — the middleware with
  the application's allowlist and trusted proxies wired in; `App` uses it automatically.
  `csrf_protection` is unchanged and still works standalone, now documented as the strict
  same-origin variant to avoid behind a `Host`-rewriting proxy.

## [0.3.7] - 2026-09-08

> Additive release: a validation rule the `validator` crate does not provide, plus two
> constraint enums that were unreachable. No breaking changes.

### Added
- **`#[karbon::validated]` + the `accepted` rule** — extends the `#[validate(...)]`
  vocabulary of the `validator` crate, which is not extensible on its own:
  `#[validate(accepted(message = "…"))]` rejects a `bool` that is not `true`, for
  "I accept the terms" checkboxes. `validator` has no equivalent — `required` only
  checks that an `Option` is `Some`, so an explicitly unchecked box would pass. On an
  `Option<bool>` field the macro also emits `required`, because `validator` skips custom
  validators on a `None` value. Placing the attribute below the `derive` is caught with
  an explicit compile error instead of a baffling "no method named `validate`".

### Fixed
- **`PasswordStrength` and `IpVersion` were unreachable from outside the crate**, making
  `Password::strength(...)` and `Ip::version(...)` impossible to call — the two enums are
  now re-exported alongside their constraint.

## [0.3.6] - 2026-09-06

> Fix release, in the same vein as 0.3.5: another piece of the `karbon new` scaffolding
> that could not compile. Nothing changes for an existing application.

### Fixed
- **`#[derive(Validate)]` did not compile in a generated project.**
  `karbon::validation::validate_request` takes `validator::Validate`, but the scaffolding
  never added `validator` to the project, so deriving it failed with *cannot find derive
  macro `Validate` in this scope*. It is now a workspace dependency of new projects,
  pinned to the version `karbon-framework` compiles against (re-exporting the derive from
  `karbon` is not an option — it expands to absolute `::validator::` paths, so the crate
  has to be nameable in the application).
- Dropped a dead `karbon-framework = "0.1"` line from the generated workspace
  `Cargo.toml` — unused (the app declares `karbon` directly) and stale since 0.1.

### Changed
- Dependencies refreshed to their latest compatible versions (`cargo update`, 22 crates).
  `sqlx` 0.9, `jsonwebtoken` 11, `tower-http` 0.7 and `argon2` 0.6 are deliberately held
  back: the first three leak into Karbon's public API, so they belong in a minor release.

## [0.3.5] - 2026-09-06

> Fix release: Studio was dead (404) in every project scaffolded since 0.3.0. Nothing
> changes for an existing application — only the `karbon new` scaffolding is affected.

### Fixed
- **`karbon new`: Studio returned 404 in a freshly created project.** The generated
  `app/Cargo.toml` did not enable the `studio` feature, while the welcome page and the
  SvelteKit/Next.js landing page both linked to `/_studio`. New projects now ship
  `features = ["studio"]`; the dashboard stays mounted only in debug builds, so
  `karbon build`/`serve` (release) are unchanged.
- **`karbon new` pinned a stale framework version.** The template hardcoded
  `karbon = "0.3.1"`; the pin is now derived from the CLI's own version, so the two can
  no longer drift.
- **`[404] GET /favicon.png`** on every page load of a new project — the SvelteKit
  `app.html` referenced a file the scaffolding never wrote. Both frontends now use an
  inline SVG icon (no asset to ship).

## [0.3.4] - 2026-09-04

> Additive release: a CLI companion to the maintenance middleware shipped in 0.3.3.
> No breaking changes, and nothing changes for an application that does not use it.

### Added
- **`karbon maintenance on|off|status`** — flips the flag file the middleware watches.
  The path is resolved exactly as the application resolves it (`MAINTENANCE_FLAG_FILE`
  from the environment, then `.env`, then the framework default), so the CLI and the
  running application can never disagree on which file matters — a hard-coded path here
  would silently flip a file nobody reads.

  It contacts no server and opens no database connection, so it works while the
  application is down — which is precisely when maintenance gets declared. `on` and
  `off` are idempotent and say so, because a procedure run at 3 a.m. should never fail
  for having been run twice.

  The command cannot bypass the file's permissions: when the flag lives outside the web
  tree and is owned by root, as recommended, `on`/`off` need `sudo`. Errors say which
  case they hit — missing directory, or missing privilege — and give the command to fix
  it.

### Changed
- `scraper` (optional `html` feature) updated from `0.20` to `0.27`.

## [0.3.3] - 2026-09-04

> Additive release: the maintenance middleware becomes usable. No breaking changes —
> with no configuration installed it behaves exactly as before.

### Added
- **`MaintenanceConfig` + `init_maintenance()`** — the maintenance middleware can now be
  driven by a **flag file** (`flag_file`), re-checked at most once per second, so an
  operator toggles maintenance with a `touch` — no restart, no deploy, and **the
  application never needs write access to the file**. Put it outside the web tree and
  owned by root, and a compromised app process cannot take the site down.
- **`Retry-After`** on the 503, from `retry_after_secs` (default 120). Without it a bare
  503 reads as "give up" to webhook senders, and their events are lost rather than
  replayed.
- **`exempt_prefixes`** — path prefixes that keep answering during maintenance. A status
  page and a health endpoint belong here: maintenance is exactly when they get consulted.
- **`is_exempt()`** exported, and **`is_maintenance()`** documented as the check background
  jobs should make before touching the database — no HTTP middleware ever sees them.

### Changed
- `is_maintenance()` now reports true when either the in-process flag or the configured
  flag file says so. With no `flag_file` configured — the default — the result is
  unchanged, so existing applications are unaffected.
- `set_maintenance(false)` no longer lifts a maintenance declared on disk: the two sources
  are independent, and an operator's file always wins.

## [0.3.2] - 2026-09-03

> Additive release: connection-pool tuning. No breaking changes — every new default
> reproduces the behaviour previous versions inherited from sqlx.

### Added
- **`PoolSettings`** (`karbon::PoolSettings`) — connection-pool tuning read from the
  environment: `DB_MIN_CONNECTIONS` (default `0`), `DB_ACQUIRE_TIMEOUT_SECS` (`30`),
  `DB_MAX_LIFETIME_SECS` (`1800`) and `DB_IDLE_TIMEOUT_SECS` (`600`). For the two
  durations, `0` disables the behaviour instead of meaning "immediately".
  Defaults are exactly sqlx's, so an app that sets none of these sees no change.
- **`Database::connect_with`, `connect_url_with`, `connect_lazy_with`** — the same three
  constructors taking an explicit `&PoolSettings`, for apps that would rather configure
  the pool in code than through the environment.
- Guidance on `max_lifetime` for clustered databases (Galera, Group Replication, managed
  HA): after a failover the pool still holds sockets to the previous node, and forced
  recycling is what redistributes it instead of discovering each dead connection one
  request at a time. Keep it below the server's own `wait_timeout`.

### Fixed
- **`Database::connect_url` ignored `DB_MAX_CONNECTIONS`.** The pool ceiling was
  hard-coded to `5`, so the variable had no effect on this code path (unlike
  `connect()`, which honoured it through `Config`). It is now read, and still falls back
  to `5` when unset — existing deployments are unaffected.

## [0.3.1] - 2026-07-01

> Additive release: a live debug mode plus a Studio hardening fix. No breaking changes.

### Added
- **Live debug mode (Symfony `app_dev.php` equivalent)**. On a deployed binary, append
  `?__karbon_dev=<KEY>` to any URL to open a **per-request** debug session — verbose errors,
  debug toolbar, and Studio access — while all other traffic stays in production mode.
  Enabled only when `KARBON_DEBUG_KEY` is set. Gated by an IP allowlist (`KARBON_DEBUG_IPS`)
  and a signed, **IP-bound**, short-lived HttpOnly cookie (HMAC-SHA256); the real client IP
  is resolved through `TRUSTED_PROXIES`. Deactivate with `?__karbon_dev=off`.

### Security
- **Studio loopback bypass closed in release builds.** The `is_loopback()` shortcut in the
  Studio guard now applies only to local debug builds. On a same-host reverse-proxy deploy the
  peer is loopback, which previously exposed `/_studio` — release builds now require a valid
  token or an active debug session.
- Error masking is now decided **per request**: a live debug session sees full detail, all
  other traffic keeps the production mask (unchanged default).

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
