# Minimal Karbon example

A tiny backend-only Karbon app used as a compile-time smoke test for the public
API (`karbon::` re-exports, the `Insertable` derive and the controller/route
attribute macros). It is part of the workspace, so CI compiles it on every push.

```bash
cargo run -p karbon-example-minimal   # needs a DB configured in .env
```

Routes:
- `GET /health` → `ok`
- `GET /posts/` → `{ "posts": [] }`
