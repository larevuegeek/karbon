# karbon-macros

Procedural macros for the [Karbon](https://github.com/larevuegeek/karbon) full-stack Rust framework.

## Macros

### Controller

```rust
#[controller(prefix = "/api/posts", role = "ROLE_ADMIN")]
impl PostController {
    #[get("/")]
    async fn list(auth: AuthGuard, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
        // auth.require_role("ROLE_ADMIN")?; ← auto-injected
    }
}
```

### Insertable

```rust
#[derive(Insertable)]
#[table_name("posts")]
#[timestamps]                  // auto-sets created_at
pub struct NewPost {
    pub title: String,
    #[slug_from("title")]      // auto-generates slug if empty
    pub slug: String,
}
// dto.insert(pool).await? → returns last_insert_id
```

### Updatable

```rust
#[derive(Updatable)]
#[table_name("posts")]
#[timestamps]                  // auto-sets updated_at
pub struct UpdatePost {
    #[primary_key] pub id: i64,
    pub title: Option<String>, // only updates Some fields
}
// dto.update(pool).await?
```

## Part of Karbon

This crate is not meant to be used standalone. It is part of the [Karbon framework](https://crates.io/crates/karbon-framework).

## License

AGPL-3.0-or-later — see [LICENSE](https://github.com/larevuegeek/karbon/blob/main/LICENSE)
