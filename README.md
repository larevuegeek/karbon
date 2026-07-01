# Karbon

**Karbon** is a full-stack Rust + frontend (SvelteKit or Next.js) framework with a unified CLI. Build, develop, and deploy from a single command.

## Why Karbon?

If you like the productivity of **Laravel / Symfony / Django** but want the
**performance, type-safety and single-binary deployment of Rust**, Karbon is for you.

- **Productive, not bare-metal** — controllers, an ORM, auth, validation, jobs,
  events, i18n and code generators come built-in. You write features, not plumbing.
- **One binary, one CLI** — `karbon new → dev → build → serve → deploy`. No glue scripts.
- **Familiar conventions** — Symfony-style role hierarchy, environments (`APP_ENV`
  with cascading `.env` files), `generate crud` scaffolding.
- **Deploy anywhere** — VPS (rsync + PM2/systemd), Docker, or PaaS (Fly/Render/Railway)
  out of the box. See the [deployment guide](docs/DEPLOY.md).
- **Safe by default** — Argon2, JWT rotation, CSRF, SQL-injection-safe query builder,
  upload hardening, XSS escaping.

> Karbon is young and pre-1.0 — the API may change. See the [CHANGELOG](CHANGELOG.md) for what's
> shipped and where it's heading (CMS &amp; shop skeletons, themeable admin, …).

## Features

- **Unified CLI** — `karbon dev / build / serve / migrate / deploy / generate / doctor / docs`
- **Rust backend** — Axum-based, with controllers, entities, repositories, auth, file uploads
- **SvelteKit or Next.js frontend** — SSR, TypeScript, Tailwind CSS
- **Single-port production** — Reverse proxy built into the Rust binary
- **Code generators** — `karbon generate crud Post title:string body:text` scaffolds entity +
  repo + controller + migration from **typed custom fields**, with **field-aware admin** UI
- **MySQL, PostgreSQL & SQLite** — switch with a feature flag; generators emit driver-correct DDL
- **Hot reload** — `karbon dev` recompiles & restarts the backend on save (frontend keeps Vite HMR)
- **Declarative route validation** — `#[karbon::get("/{id}", id = "int:min=1")]` → 400 before the handler
- **OpenAPI & Swagger UI** — generated `/openapi.json` (params, tags) + `/docs` explorer
- **Studio dev cockpit** — Overview/perf, schema browser, **route explorer**, integrated
  **terminal** (whitelisted CLI + scaffolding form), and **live docs**, all in the browser
- **`karbon doctor`** — offline project diagnostics (CI-friendly exit code) + `--db` checks
- **Living docs** — `karbon docs build` renders `docs/**.md` to a static site; same source in Studio
- **Realtime Channels** · **Typed Query Builder** · **Feature Flags** · **Inertia.js** · **LiveWire** · **HMR**
- **Batteries included** — JWT, Argon2, CSRF, compression, rate limiting, soft delete, background
  jobs, message bus, events, cache (memory/file/Redis), i18n, forms, image pipeline, and more

## Quick Start

```bash
# Create a new project (SvelteKit by default)
karbon new my-app
karbon new my-app --frontend nextjs  # or with Next.js

cd my-app

# Start development (backend + frontend with hot-reload)
karbon dev

# Build for production
karbon build

# Run in production (single port)
karbon serve
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `karbon new <name>` | Create a new project (SvelteKit by default) |
| `karbon new <name> --frontend nextjs` | Create a new project with Next.js |
| `karbon dev` | Start dev servers (Rust + Vite, hot-reload) |
| `karbon build` | Build for production |
| `karbon serve` | Run production build (single port, reverse proxy) |
| `karbon generate entity <Name> [field:type…]` | Generate entity + migration (typed fields) |
| `karbon generate controller <Name>` | Generate a controller (validated `{id}` routes) |
| `karbon generate crud <Name> [field:type…]` | Generate entity + repo + controller + migration |
| `karbon generate admin <Name>` | Generate a field-aware CRUD back-office |
| `karbon g crud <Name> --dry-run` | Short alias; `--dry-run` (plan) / `--force` (overwrite) |
| `karbon migrate` | Apply pending (versioned) migrations |
| `karbon migrate status` / `rollback` | Show migration status / roll back the last one |
| `karbon migrate diff [name]` | Generate a migration from the entity ↔ database diff |
| `karbon doctor` / `doctor --db` | Diagnose the project (offline; `--db` checks connectivity) |
| `karbon docs build` | Render `docs/**.md` to a static site (`docs/_site/`) |
| `karbon deploy docker` | Generate optimized multi-stage Dockerfile |
| `karbon deploy paas` | Generate Fly.io / Render / Procfile config |
| `karbon deploy fly` / `railway` | Deploy via the platform CLI |
| `karbon deploy publish` | Publish artifacts (rsync, local or SSH) |
| `karbon deploy publish:build` | Build + publish |

## Database

MySQL is the default. To use PostgreSQL, change the feature flag:

```toml
# Cargo.toml of your project
karbon = { package = "karbon-framework", version = "0.3", default-features = false, features = ["postgres"] }
```

Set the matching port in `.env`:
```
DB_PORT=5432  # PostgreSQL (default: 3306 for MySQL)
```

## Project Structure

```
my-app/
├── karbon.toml              # Project configuration
├── Cargo.toml               # Rust workspace
├── app/                     # Rust backend
│   └── src/
│       ├── main.rs
│       ├── controller/
│       ├── entity/
│       ├── repository/
│       └── service/
├── frontend/                # SvelteKit or Next.js frontend
│   └── src/
│       ├── routes/
│       ├── lib/
│       └── app.css
├── migration/               # SQL migration files
└── .env                     # Environment variables
```

## Configuration

All configuration is in `karbon.toml`:

```toml
[app]
name = "My App"

[backend]
package = "app"
port = 3005

[frontend]
dir = "frontend"
port = 3004
```

Environment variables go in `.env` (copy from `.env.example`).

## Framework Features

### Controllers

```rust
#[karbon::controller(prefix = "/admin/posts", role = "ROLE_ADMIN")]
impl PostController {
    #[karbon::get("/")]
    async fn list(auth: AuthGuard, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
        // auth.require_role("ROLE_ADMIN")?; ← auto-injected by the macro
    }
}
```

### Entities & Repositories

```rust
impl CrudRepository for Post {
    const TABLE: &'static str = "posts";
    const ENTITY_NAME: &'static str = "Article";
    const HAS_SLUG: bool = true;
    const SOFT_DELETE: bool = true; // delete() sets deleted_at instead of removing
}
// find_by_id, find_by_slug, find_all, count, delete, restore, force_delete, exists, find_where, find_all_where
```

### Insertable / Updatable

```rust
#[derive(karbon::Insertable)]
#[table_name("posts")]
#[timestamps]                     // auto-sets created_at
pub struct NewPost {
    pub title: String,
    #[slug_from("title")]         // auto-generates slug if empty
    pub slug: String,
}

#[derive(karbon::Updatable)]
#[table_name("posts")]
#[timestamps]                     // auto-sets updated_at
pub struct UpdatePost {
    #[primary_key] pub id: i64,
    pub title: Option<String>,    // only updates Some fields
}
```

### Typed Query Builder (SelectBuilder)

```rust
use karbon::db::{SelectBuilder, Order};

// Fluent API with parameterized conditions
let users: Vec<User> = SelectBuilder::table("users")
    .columns("id, name, email")
    .where_eq("active", true)
    .where_like("name", "%alice%")
    .where_gt("age", 18)
    .where_in("role", &["admin", "editor"])
    .where_null("deleted_at")
    .order_by("created_at", Order::Desc)
    .limit(20)
    .fetch_all(&pool)
    .await?;

// Count with same conditions
let count = SelectBuilder::table("users")
    .where_eq("active", true)
    .count(&pool)
    .await?;

// Single row
let user: Option<User> = SelectBuilder::table("users")
    .where_eq("email", "alice@example.com")
    .fetch_one(&pool)
    .await?;
```

### Transactions

```rust
let mut tx = state.db.begin().await?;
new_post.insert(&mut *tx).await?;
tx.commit().await?;
```

### Security

- **Password hashing**: Argon2id (+ bcrypt legacy migration)
- **JWT**: HS256, access token 15min, refresh token 30 days with rotation
- **CSRF**: Double-submit cookie pattern with constant-time comparison
- **Role hierarchy**: Symfony-style (ROLE_SUPER_ADMIN > ROLE_ADMIN > ROLE_USER)
- **File uploads**: Magic byte validation, SVG sanitization, path traversal protection
- **CORS**: Configurable with restrictive fallback
- **SQL injection protection**: SelectBuilder validates all identifiers (table, column, join, order_by)
- **XSS protection**: Full HTML escaping in Inertia props, `html_escape()` helper for LiveWire
- **Shell injection protection**: Config values validated before Dockerfile/script generation
- **Credential safety**: DB passwords passed via env vars, not CLI args

### Middleware

Built-in middleware applied automatically or available as layers:

- **Compression** — gzip/brotli on all responses
- **Request ID** — unique `X-Request-Id` per request (UUID)
- **CSRF protection** — cookie-based, auto on unsafe methods
- **Rate limiting** — `RateLimitLayer::per_minute(60)` per IP
- **Maintenance mode** — `set_maintenance(true)` → 503 globally
- **Graceful shutdown** — Ctrl+C / SIGTERM shuts down cleanly

### Realtime Channels (WebSocket)

```rust
use karbon::channel::ChannelRegistry;

let channels = ChannelRegistry::new();

// In a WebSocket handler — auto-manages join/leave/broadcast
Router::new().route("/ws/channels", get(|ws: WebSocketUpgrade, State(channels): State<ChannelRegistry>| {
    ws.on_upgrade(move |socket| channels.handle_socket(socket))
}));

// Broadcast from anywhere (controller, job, event handler)
channels.broadcast("chat/room-1", "new_message", &message).await;

// Client-side JSON protocol:
// Join:    {"channel": "chat/room-1", "event": "join", "payload": {}}
// Leave:   {"channel": "chat/room-1", "event": "leave", "payload": {}}
// Message: {"channel": "chat/room-1", "event": "message", "payload": {"text": "hello"}}
```

### Feature Flags

```rust
use karbon::feature::FeatureFlags;

let flags = FeatureFlags::new();
flags.register("dark_mode", true, "Enable dark mode UI").await;
flags.register("beta_search", false, "New search algorithm").await;

// Check in handlers
if flags.is_enabled("dark_mode").await {
    // show dark mode
}

// Toggle at runtime (e.g., from admin endpoint)
flags.toggle("beta_search").await;
flags.enable("dark_mode").await;
flags.disable("dark_mode").await;
```

### Inertia.js Adapter

Controllers return "pages" instead of JSON. On first visit → full HTML. On navigation → JSON only (the client swaps components without full reload).

```rust
use karbon::inertia::Inertia;

#[karbon::get("/dashboard")]
async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    Inertia::render("Dashboard", serde_json::json!({
        "user": current_user,
        "stats": stats,
    }))
}

// After POST/PUT/DELETE — redirect back (Inertia protocol)
Inertia::location("/dashboard")
```

Setup:
```rust
use karbon::inertia::{InertiaConfig, inertia_middleware};

let config = InertiaConfig::new(include_str!("templates/app.html"))
    .version("1.0.0");

let app = Router::new()
    .layer(Extension(config))
    .layer(middleware::from_fn(inertia_middleware));
```

### LiveWire Components

Server-rendered interactive components — no React, no Svelte, no JS framework needed.

```rust
use karbon::livewire::{LiveComponent, live_render, live_socket};

struct Counter { count: i32 }

impl LiveComponent for Counter {
    fn render(&self) -> String {
        format!(r#"
            <span>{}</span>
            <button lw-click="increment">+</button>
            <button lw-click="decrement">-</button>
        "#, self.count)
    }

    fn handle_event<'a>(&'a mut self, event: &'a str, _params: &'a HashMap<String, String>)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            match event {
                "increment" => self.count += 1,
                "decrement" => self.count -= 1,
                _ => {}
            }
        })
    }
}

// Serve the page
#[karbon::get("/counter")]
async fn counter_page() -> impl IntoResponse {
    live_render(Counter { count: 0 }, "/ws/counter")
}

// WebSocket for live updates
Router::new()
    .route("/ws/counter", get(|ws: WebSocketUpgrade| {
        ws.on_upgrade(|socket| live_socket(socket, Counter { count: 0 }))
    }))
```

Client-side directives: `lw-click`, `lw-submit`, `lw-input`, `lw-param-*`.

### HMR (Hot Module Replacement)

Auto-reload during development. CSS changes are hot-swapped without page reload.

```rust
use karbon::hmr::HmrServer;

let hmr = HmrServer::new()
    .watch("templates/")
    .watch("static/");

Router::new()
    .route("/_hmr/ws", get({
        let hmr = hmr.clone();
        move |ws: WebSocketUpgrade| hmr.ws_handler(ws)
    }));

hmr.start(); // Spawns background file watcher
```

Inject the client script in your HTML layout:
```rust
if cfg!(debug_assertions) {
    html.push_str(&hmr.client_script());
}
```

### Background Jobs

```rust
let queue = JobQueue::new(4); // 4 concurrent workers
queue.push(SendEmailJob { to: "user@example.com".into() }).await;
// Runs in background with automatic retry on failure
```

### Event System

```rust
let bus = EventBus::new();
bus.on::<UserCreated>(|event| async move {
    // send welcome email, update stats, etc.
}).await;
bus.emit(UserCreated { user_id: 1 }).await;
```

### i18n

```rust
let mut i18n = I18n::new("fr");
i18n.add_translations("fr", &[("user.not_found", "Utilisateur introuvable")]);
i18n.t("user.not_found")                          // → "Utilisateur introuvable"
i18n.t_with("welcome", &[("name", "David")])       // → "Bienvenue David"
```

### Studio (Dev Dashboard)

Real-time monitoring dashboard for development. Zero dependencies, in-memory only.

```toml
# Enable in Cargo.toml
karbon = { package = "karbon-framework", features = ["studio"] }
```

At startup, the terminal shows:
```
⚡ Studio → http://localhost:3000/_studio?token=a7f3b9c2...
```

Monitors in real-time: HTTP requests, EventBus events, background jobs, sent emails.
Protected by a random token, only active in debug mode by default.

### Database Seeder

```rust
impl Seeder for UserSeeder {
    fn name(&self) -> &str { "users" }
    fn seed<'a>(&'a self, pool: &'a DbPool) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>> {
        Box::pin(async move { /* insert test data */ Ok(()) })
    }
}
run_seeders(pool, &[Box::new(UserSeeder)]).await?;
```

### Testing

```rust
#[tokio::test]
async fn test_health() {
    let app = TestApp::spawn(build_router()).await;
    let res = app.get("/health").await;
    assert_eq!(res.status(), 200);
}
```

### Migrations

```bash
# Run all SQL files from migration/ directory
karbon migrate
```

Reads database connection from `DATABASE_URL` or individual `DB_*` variables in `.env`.

### Deploy

```bash
# Generate optimized multi-stage Dockerfile
karbon deploy docker

# Publish artifacts (local or SSH)
karbon deploy publish

# Build + publish
karbon deploy publish:build
```

Configure in `karbon.toml`:

```toml
[deploy]
path = "/var/www/my-app"
user = "www-data"                    # optional — owner for chown
group = "www-data"                   # optional — group for chown (defaults to user)
manager = "pm2"                      # "pm2" or "systemd"
pm2_config = "ecosystem.config.cjs"  # PM2 config file (default)
service = "my-app"                   # systemd service name (default: app.name)
host = "user@server"                 # optional — if absent, deploys locally
```

## Requirements

- Rust 1.85+ (edition 2024)
- Node.js 20+
- MySQL/MariaDB or PostgreSQL

## License

AGPL-3.0 — see [LICENSE](LICENSE) for details.
