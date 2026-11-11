# Karbon — reference

A quick reference of the framework, rendered live in Studio. It evolves with the code.

## CLI

| Command | Purpose |
|---------|---------|
| `karbon new <app>` | New project (`--frontend svelte\|nextjs`, `--skeleton full\|micro`, `--template blog`, `--local <repo>`) |
| `karbon dev` | Development (backend + frontend, hot reload) |
| `karbon build` / `karbon serve` | Production build / run it (built-in reverse proxy) |
| `karbon generate <kind> <Name>` | Generates `entity` / `controller` / `crud` / `admin` (`--dry-run`, `--force`) |
| `karbon generate auth` | Accounts: sign-up, sign-in, session renewal, password reset — browser pages + JSON API |
| `karbon user create <email> [--admin]` | Creates an account (`user promote <email>` makes one an administrator) |
| `karbon migrate [status\|rollback\|diff]` | Versioned migrations; `diff` = entities ↔ database |
| `karbon doctor` | Diagnoses common pitfalls (offline) |
| `karbon secret [--write]` | Generates a random `JWT_SECRET` (printed, or written to `.env`) |
| `karbon docs build` | Renders `docs/*.md` → static site in `docs/_site/` |
| `karbon deploy <docker\|paas\|fly\|railway\|publish>` | Deployment |

## Controllers

```rust
#[karbon::controller(prefix = "/admin/posts", role = "ROLE_ADMIN")]
impl PostController {
    #[karbon::get("/")]
    async fn list(auth: AuthGuard, State(state): State<AppState>) -> AppResult<impl IntoResponse> { /* … */ }
}
```

The macro generates `router()`, `prefix()` and `openapi_paths()`. Generated routes are
mounted automatically in `main.rs` (markers `// karbon:routes` / `// karbon:api-routes`).

**Per-route protection**: `role = "…"` (controller) or `#[require_role("…")]` (route, takes
precedence). The check uses the handler's `auth: AuthGuard` parameter; a route that carries
a role **without** `auth: AuthGuard` **does not compile** (no silently unprotected route).

**Global firewall**: `App::firewall(AccessControl::new().public("^/login").rule("^/admin", ["ROLE_ADMIN"]))`
— URL pattern → roles rules, enforced centrally (the first matching rule wins).

**Route parameter validation** (declarative, answered with a 400 before the handler runs):

```rust
#[karbon::get("/{id}", id = "int:min=1")]                 // integer ≥ 1
#[karbon::get("/{slug}", slug = "slug")]                   // ^[a-z0-9-]+$
#[karbon::get("/{status}", status = "enum:draft|published")]
#[karbon::get("/{code}", code = "regex:^[A-Z]{2}[0-9]{2}$")]
```
Rules: `int[:min=N][:max=N]`, `string[:minlen=N][:maxlen=N]`, `slug`, `uuid`,
`enum:a|b|c`, `regex:^…$`. The binding name must match the path parameter name.

## Database

- Multi-driver: **MySQL** (default) / **PostgreSQL** / **SQLite** (feature flags).
- `CrudRepository`: `find_by_id`, `find_all`, `find_where`, `delete`, soft delete, slug…
- `SelectBuilder`: typed, parameterized query builder (injection-safe).
- `Insertable` / `Updatable` (derive): INSERT/UPDATE with `#[timestamps]`, `#[slug_from]`.
- Relations: `has_many`, belongs-to through `find_by_id`, `load_grouped_by` (no N+1).
- **Optional database**: empty `DB_NAME` → the app starts without connecting.
- **Migrations by diff**: entities are the source of truth; `karbon migrate diff` compares
  entities ↔ database and writes the migration (`CREATE` / `ALTER ADD`, additive only).

## Security

Argon2id, JWT (15 min access token + 30 day refresh token, stored as a SHA-256 hash), CSRF
(double submit), AuthGuard (Bearer header, or the `karbon_auth` session cookie for browsers)
+ role hierarchy, `LOGIN_PATH` (browsers without a session are sent to the login page), hardened uploads (magic bytes, path traversal
protection, SVG sanitization), CORS, compression, request ID, graceful shutdown.

## Components

- **Forms** (`form::Form` / `Field`): binding, validation, HTML rendering, CSRF.
- Composable **Validator**: collects every violation, nested, with groups.
- **Messages** (`MessageBus`): events and background work, sync or async, retry with
  backoff, dead-letter log. `EventBus` and `JobQueue` are deprecated.
- **Persistent jobs** (`PersistentQueue`): stored in the database, survive a restart.
- **Cache**: memory / file / Redis, tags, HTTP cache (ETag).
- **Templates**: Tera (the supported engine); MiniJinja and the in-house engine
  (`native-templates`) are experimental.
- **i18n**, **mail** (lettre), **scraping**, **images** (`ImageProcessor`).
- **Realtime**: `ChannelRegistry` (WebSocket rooms), HMR, LiveWire, Inertia.

## Language

Karbon's own messages (masked errors, route and form validation) are in English.
`APP_LOCALE=fr` switches them to French. Your application's messages are yours: translate
them with `i18n::I18n`.

## Studio (dev)

Real-time dashboard (feature `studio`, development only): requests, events, jobs, mails,
**Overview** (performance), **Database** (schema), **Terminal** (allowlisted CLI commands +
Maker), **Docs** (this page). Open from a browser on the same machine, or with the token
printed at startup. A toolbar is injected at the bottom of HTML pages.

## Extensibility

- `http::Module` — contributes routes.
- `http::Bundle` — routes + a `boot(&AppState)` hook at startup (compile-time plugin).
