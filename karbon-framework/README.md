# karbon-framework

**Karbon** is a batteries-included Rust web framework built on [Axum](https://github.com/tokio-rs/axum). MySQL and PostgreSQL, JWT auth, CSRF, background jobs, events, i18n, and more — out of the box.

## Setup

```toml
# Cargo.toml
[dependencies]
framework = { package = "karbon-framework", version = "0.1" }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "mysql", "chrono"] }
```

```env
# .env
DB_HOST=127.0.0.1
DB_PORT=3306
DB_NAME=myapp
DB_USER=root
DB_PASSWORD=
JWT_SECRET=change-me-to-a-random-secret
```

```rust
use framework::http::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    App::new()
        .router(build_router())
        .serve()  // graceful shutdown built-in
        .await
}
```

For **PostgreSQL**, swap the feature:

```toml
framework = { package = "karbon-framework", version = "0.1", default-features = false, features = ["postgres"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "chrono"] }
```

---

## Controllers

Define routes with proc macros. The `#[require_role]` attribute auto-injects role checks.

```rust
use framework::{controller, get, post, require_role};
use framework::http::AppState;
use framework::security::AuthGuard;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

#[controller(prefix = "/api/posts")]
impl PostController {
    #[get("/")]
    async fn list(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
        let posts = Post::find_all(state.db.pool(), Some(("created_at", "DESC"))).await?;
        Ok(Json(posts))
    }

    #[post("/")]
    #[require_role("ROLE_ADMIN")]
    async fn create(
        auth: AuthGuard,
        State(state): State<AppState>,
        Json(input): Json<CreatePostInput>,
    ) -> AppResult<impl IntoResponse> {
        let id = NewPost {
            title: input.title,
            slug: String::new(), // auto-generated if #[slug_from("title")]
        }.insert(state.db.pool()).await?;
        Ok(Json(serde_json::json!({ "id": id })))
    }
}

// Mount in your router:
fn build_router() -> Router<AppState> {
    Router::new()
        .nest(PostController::prefix(), PostController::router())
}
```

## Entities & CrudRepository

Implement `CrudRepository` to get free CRUD methods. Add `SOFT_DELETE = true` for soft deletion.

```rust
use sqlx::FromRow;
use serde::Serialize;
use framework::db::CrudRepository;

#[derive(Debug, FromRow, Serialize)]
pub struct Post {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl CrudRepository for Post {
    const TABLE: &'static str = "posts";
    const ENTITY_NAME: &'static str = "Article";
    const HAS_SLUG: bool = true;
    const SOFT_DELETE: bool = true;
}

// Now you can:
let post = Post::find_by_id(pool, 42).await?;
let post = Post::find_by_slug(pool, "hello-world").await?;
let all  = Post::find_all(pool, Some(("created_at", "DESC"))).await?;
let n    = Post::count(pool).await?;
Post::delete(pool, 42).await?;         // sets deleted_at = NOW()
Post::restore(pool, 42).await?;        // sets deleted_at = NULL
Post::force_delete(pool, 42).await?;   // actual DELETE

// Dynamic WHERE:
let posts = Post::find_all_where(pool, &[("status", "published".into())], None).await?;
```

## Insertable & Updatable

Derive macros that generate type-safe INSERT/UPDATE queries.

```rust
use framework::Insertable;

#[derive(Insertable)]
#[table_name("posts")]
#[timestamps]                     // auto-adds created_at = NOW()
pub struct NewPost {
    pub title: String,
    #[slug_from("title")]         // auto-generates slug from title if empty
    pub slug: String,
    pub content: String,
    pub user_id: i64,
}

let id = NewPost {
    title: "Hello World".into(),
    slug: String::new(),          // → auto-generated: "hello-world"
    content: "...".into(),
    user_id: 1,
}.insert(pool).await?;            // returns last_insert_id
```

```rust
use framework::Updatable;

#[derive(Updatable)]
#[table_name("posts")]
#[timestamps]                     // auto-adds updated_at = NOW()
pub struct UpdatePost {
    #[primary_key]
    pub id: i64,
    pub title: Option<String>,    // only included in SET if Some
    pub content: Option<String>,
}

UpdatePost {
    id: 42,
    title: Some("New Title".into()),
    content: None,                // not included in query
}.update(pool).await?;
// → UPDATE posts SET title=?, updated_at=? WHERE id=?
```

## Transactions

```rust
let mut tx = state.db.begin().await?;
NewPost { ... }.insert(&mut *tx).await?;
NewComment { ... }.insert(&mut *tx).await?;
tx.commit().await?;
// If anything fails, the transaction is automatically rolled back on drop
```

## Authentication & Security

### JWT

```rust
use framework::security::{JwtManager, Claims};

let jwt = JwtManager::new(&config.jwt_secret, config.jwt_expiration);
let token = jwt.generate(&Claims { user_id: 1, username: "david".into(), roles: vec!["ROLE_ADMIN".into()] })?;
let claims = jwt.verify(&token)?;
```

### AuthGuard (Axum extractor)

```rust
async fn me(auth: AuthGuard) -> impl IntoResponse {
    auth.require_role("ROLE_USER")?;
    Json(serde_json::json!({ "user_id": auth.claims.user_id }))
}
```

### Role Hierarchy

```
ROLE_SUPER_ADMIN → ROLE_ADMIN → ROLE_REDACTEUR, ROLE_MODERATEUR → ROLE_USER
```

A user with `ROLE_SUPER_ADMIN` passes `auth.require_role("ROLE_ADMIN")`.

### Password Hashing

```rust
use framework::security::Password;

let hash = Password::hash("my-password")?;       // Argon2id
let ok = Password::verify("my-password", &hash)?; // also supports bcrypt legacy
```

## Middleware

All built-in and applied automatically by `App::serve()`:

| Middleware | Description |
|---|---|
| **Compression** | gzip/brotli on all responses |
| **Request ID** | `X-Request-Id` UUID on every request/response |
| **Graceful shutdown** | Clean shutdown on Ctrl+C / SIGTERM |

Available as opt-in layers:

```rust
use framework::http::middleware::{csrf_protection, RateLimitLayer};

let app = Router::new()
    .route("/api/login", post(login))
    .layer(axum::middleware::from_fn(csrf_protection))
    .layer(RateLimitLayer::per_minute(60));
```

| Middleware | Description |
|---|---|
| `csrf_protection` | Double-submit cookie (SameSite=Strict, constant-time comparison) |
| `RateLimitLayer::per_minute(n)` | Per-IP rate limiting |
| `maintenance_mode` | Returns 503 globally when enabled |
| `request_logger` | Logs method, path, status, duration |

## Background Jobs

In-process job queue with configurable workers and automatic retry.

```rust
use framework::job::{Job, JobQueue};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

struct SendEmailJob { to: String, subject: String }

impl Job for SendEmailJob {
    fn name(&self) -> &str { "send_email" }
    fn max_retries(&self) -> u32 { 3 }
    fn retry_delay(&self) -> Duration { Duration::from_secs(10) }

    fn execute(&self) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        Box::pin(async {
            // send email logic here
            Ok(())
        })
    }
}

let queue = JobQueue::new(4); // 4 concurrent workers
queue.push(SendEmailJob { to: "user@example.com".into(), subject: "Hello".into() }).await;
```

## Event System

Async pub/sub for decoupling logic.

```rust
use framework::event::{Event, EventBus};

struct UserCreated { user_id: i64, email: String }
impl Event for UserCreated {}

let bus = EventBus::new();

// Register handlers
bus.on::<UserCreated>(|event| async move {
    println!("Send welcome email to {}", event.email);
}).await;

bus.on::<UserCreated>(|event| async move {
    println!("Update stats for user {}", event.user_id);
}).await;

// Emit — all handlers run concurrently
bus.emit(UserCreated { user_id: 1, email: "david@example.com".into() }).await;
```

## WebSocket

Helpers for Axum WebSocket upgrade.

```rust
use framework::http::ws::websocket_handler;
use axum::extract::ws::{WebSocket, Message};

async fn handle_socket(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            let _ = socket.send(Message::Text(format!("Echo: {text}"))).await;
        }
    }
}

// In your router:
Router::new().route("/ws", get(|ws: WebSocketUpgrade| websocket_handler(ws, handle_socket)))
```

## i18n

Simple translation system with interpolation and locale fallback.

```rust
use framework::i18n::I18n;

let mut i18n = I18n::new("fr");

i18n.add_translations("fr", &[
    ("user.not_found", "Utilisateur introuvable"),
    ("welcome", "Bienvenue {name} !"),
]);

i18n.add_translations("en", &[
    ("user.not_found", "User not found"),
    ("welcome", "Welcome {name}!"),
]);

i18n.t("user.not_found");                            // → "Utilisateur introuvable"
i18n.t_locale("user.not_found", "en");                // → "User not found"
i18n.t_with("welcome", &[("name", "David")]);         // → "Bienvenue David !"

// Load from JSON:
i18n.load_json("fr", r#"{"bye": "Au revoir"}"#)?;
```

## Validation

25+ built-in constraints for input validation.

```rust
use framework::validation::constraints::string::*;
use framework::validation::constraints::number::*;
use framework::validation::constraints::security::*;

// String constraints
Length::new().min(3).max(100).validate("hello")?;
Email::new().validate("user@example.com")?;
Url::new().validate("https://example.com")?;
NotBlank::new().validate("hello")?;

// Password strength
PasswordStrength::strong().validate("MyP@ssw0rd123")?;

// Async validation (database checks)
use framework::validation::AsyncValidator;

AsyncValidator::new(pool)
    .unique("users", "email", &input.email, "Email already taken")
    .exists("roles", "id", input.role_id, "Role not found")
    .validate()
    .await?;
```

## Pagination

```rust
use framework::db::{PaginatedQuery, Paginated, PaginationParams};

// In a handler — params come from query string: ?page=1&per_page=20&sort=id&order=desc&search=hello
async fn list(Query(params): Query<PaginationParams>, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let (items, total) = PaginatedQuery::<Post>::new("SELECT * FROM posts")
        .allowed_sorts(&["id", "title", "created_at"])
        .search_columns(&["title", "content"])
        .default_sort("created_at")
        .execute(state.db.pool(), &params)
        .await?;

    Ok(Json(Paginated::new(items, total, &params)))
}
// Response: { "data": [...], "meta": { "page": 1, "per_page": 20, "total": 42, "total_pages": 3, "has_next": true, "has_prev": false } }
```

## Database Seeder

```rust
use framework::db::seeder::{Seeder, run_seeders};
use framework::db::DbPool;
use std::pin::Pin;
use std::future::Future;

struct UserSeeder;

impl Seeder for UserSeeder {
    fn name(&self) -> &str { "users" }

    fn seed<'a>(&'a self, pool: &'a DbPool) -> Pin<Box<dyn Future<Output = framework::AppResult<()>> + Send + 'a>> {
        Box::pin(async move {
            NewUser { username: "admin".into(), email: "admin@example.com".into(), ... }.insert(pool).await?;
            Ok(())
        })
    }
}

run_seeders(pool, &[Box::new(UserSeeder)]).await?;
```

## Testing

```rust
use framework::testing::TestApp;

#[tokio::test]
async fn test_list_posts() {
    let app = TestApp::spawn(build_router()).await;

    let res = app.get("/api/posts").await;
    assert_eq!(res.status(), 200);

    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn test_create_post_requires_auth() {
    let app = TestApp::spawn(build_router()).await;

    let res = app.post_json("/api/posts", &serde_json::json!({
        "title": "Test"
    })).await;
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_authenticated_request() {
    let app = TestApp::spawn(build_router()).await;

    let res = app.get_auth("/api/me", "valid-jwt-token").await;
    assert_eq!(res.status(), 200);
}
```

## File Uploads

```rust
use framework::storage::{UploadManager, UploadConfig};

let config = UploadConfig {
    upload_dir: "./uploads".into(),
    max_file_size: 10 * 1024 * 1024, // 10MB
    allowed_extensions: vec!["jpg", "png", "gif", "webp"],
    ..Default::default()
};

let manager = UploadManager::new(config);
let file = manager.save(field).await?;
// Magic bytes validated, SVG sanitized, path traversal prevented
// Returns: SavedFile { filename, path, size, mime_type }
```

---

## Full documentation

CLI, project scaffolding, and full docs: [github.com/larevuegeek/karbon](https://github.com/larevuegeek/karbon)

## License

AGPL-3.0-or-later — Copyright (C) 2026 LaRevueGeek
