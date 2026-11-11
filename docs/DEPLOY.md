# Deploying a Karbon application

A Karbon app compiles to **one Rust binary** (backend + built-in reverse proxy) shipped
with the **frontend build** (SvelteKit/Next.js, server-rendered by Node). This guide covers
the deployment targets, from the simplest to the most advanced.

> Prerequisite: `karbon build` produces `target/release/<app>` and the frontend build. In a
> Cargo workspace, `target/` is the one at the workspace root (or `CARGO_TARGET_DIR`):
> Karbon asks cargo where it is.
> In production the Rust binary serves the API and proxies everything else to the Node SSR
> server when `KARBON_FRONTEND_URL` is set (which `karbon serve` does).

---

## 1. VPS / dedicated server (rsync + PM2 or systemd)

The most direct route when you have SSH access. Fill in the `[deploy]` section of
`karbon.toml`:

```toml
[deploy]
path = "/var/www/my-app"
user = "www-data"            # optional — chown after the copy
manager = "pm2"              # "pm2" or "systemd"
host = "user@server"         # optional — without it, the deploy is local
```

Then:

```bash
karbon deploy publish:build   # build + rsync + restart
karbon deploy publish         # rsync + restart (no rebuild)
```

The binary, the frontend build, the migrations and `.env.example` are synchronized, the
frontend dependencies installed (`npm install --omit=dev`), then the process manager
restarted.

### Several servers behind a load balancer

With two instances or more, **build once** and publish the same result everywhere: two
builds of the same code do not produce the same frontend file names (SvelteKit stamps a
version into them), and a page served half by one build and half by another breaks.

```toml
[deploy]
path = "/var/www/my-app"
user = "www-data"
manager = "pm2"
# Published in this order, one after the other; "local" = the machine running the command
hosts = ["root@10.0.0.12", "local"]
# Optional: wait for this one to answer before moving on (checked ON the target)
health_url = "http://127.0.0.1:8080/api/health"
health_timeout = 120          # seconds (default: 120)
```

`karbon deploy publish:build` builds **once**, then publishes to and restarts each target
in turn. With `health_url`, the next target is only touched once the previous one answers:
the servers are never all restarted at the same time. At the first failure the command
stops and says which targets were not published. `host` (a single target) is still
accepted; `host` and `hosts` together are refused.

---

## 2. Docker

```bash
karbon deploy docker          # generates Dockerfile + .dockerignore
docker build -t my-app .
docker run -p 3000:3000 --env-file .env -e APP_ENV=production my-app
```

The generated Dockerfile is **multi-stage** (frontend build → backend build → `debian-slim`
runtime image with Node for SSR) and includes a `HEALTHCHECK` on `/health`.

> Note: a fully static `scratch`/`distroless` image is not possible while Node SSR is
> embedded (the Node runtime is required). An **API-only** app (no SSR frontend) can run
> the Rust binary alone in a minimal image.

---

## 3. PaaS (Fly.io / Render / Railway)

```bash
karbon deploy paas            # generates fly.toml, render.yaml, Procfile (+ Dockerfile if missing)
```

- **Fly.io**: `fly launch --copy-config --no-deploy` (first time), then `fly deploy`.
- **Render**: connect the repository to a Render Blueprint — `render.yaml` is detected.
- **Railway**: `railway up` (uses the Dockerfile).

All of them point their health check at `/health` and set `APP_ENV=production`.

Shortcuts (they generate the config when needed, then deploy through the platform's CLI):

```bash
karbon deploy fly       # → flyctl deploy
karbon deploy railway   # → railway up
```

### Multi-arch (amd64 + arm64)

The generated Dockerfile works under `buildx` for both architectures:

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t my-app --push .
```

---

## 4. Environment variables & environments

Karbon loads the `.env` files in cascade according to `APP_ENV` (Symfony style):

```
.env.{env}.local   (highest priority)
.env.local         (ignored in test)
.env.{env}
.env               (lowest priority)
```

In production, set `APP_ENV=production`. Values already present in the real environment are
never overwritten by the files.

What an app needs depends on what it uses: `JWT_SECRET` for authentication (generate it
with `karbon secret` — the app refuses to start with the placeholder of `.env.example`),
and `DB_NAME` / `DB_USER` / `DB_PASSWORD` for a database. See `.env.example`.

---

## 5. Migrations

```bash
karbon migrate                # applies the pending migrations of migration/
karbon migrate status         # applied / pending
karbon migrate rollback       # undoes the last one (its `-- migrate:down` section)
```

Migrations are versioned: each file runs once, tracked in the `_karbon_migrations` table.
They run natively — no `mysql` or `psql` client is needed on the server. The connection
comes from `DATABASE_URL` or the `DB_*` variables.

---

## 6. Shared hosting

⚠️ Classic shared hosting does not let a long-lived Rust process run. The `cgi` feature is
a best-effort adapter that has not been validated on a real shared host. For now, prefer a
VPS, Docker or a PaaS.
