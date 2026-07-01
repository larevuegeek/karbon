PORT=3005
APP_ENV=development
LOG_LEVEL=info

# Database — OPTIONAL. Leave DB_NAME empty to run WITHOUT a database.
# Set DB_NAME (e.g. {{PROJECT_NAME_SNAKE}}) when you add entities / use the DB.
# MySQL default port: 3306 | PostgreSQL default port: 5432
DB_HOST=127.0.0.1
DB_PORT=3306
DB_NAME=
DB_USER=root
DB_PASSWORD=

# Auth — required if you use JWT/AuthGuard. Empty disables auth (all tokens rejected).
# MUST be a strong random value of at least 32 chars in production (e.g. `openssl rand -base64 48`).
# The app refuses to start in production with an empty, weak, or placeholder secret.
JWT_SECRET=change-me-to-a-random-secret
JWT_EXPIRATION=900
REFRESH_TOKEN_EXPIRATION=2592000

# CSRF protection (double-submit + same-site Origin check). Set to false only for pure APIs.
CSRF_ENABLED=true

# CORS — empty = deny cross-origin (same-origin only). Set a comma list or "*" to allow.
CORS_ORIGINS=

# Trusted reverse proxies. When Karbon runs behind nginx/Cloudflare (TLS terminated
# upstream), list the proxy IPs so X-Forwarded-For/Proto/Host are trusted for the real
# client IP, rate-limiting and SSR canonical URLs. Empty = Karbon is the edge (headers
# ignored, safe). Use "*" only if you are ALWAYS behind a trusted proxy.
# TRUSTED_PROXIES=127.0.0.1,10.0.0.1

# Live debug mode (Symfony `app_dev.php` equivalent). Disabled unless KARBON_DEBUG_KEY is set.
# On a live deployment, append ?__karbon_dev=<KEY> to any URL from an allowed IP to open a
# per-request debug session (verbose errors + debug toolbar + Studio) bound to your IP; the
# rest of the traffic stays in production mode. Deactivate with ?__karbon_dev=off.
# Requires TRUSTED_PROXIES set correctly (real client IP is resolved from X-Forwarded-For).
# KARBON_DEBUG_KEY=change-me-to-a-long-random-secret
# KARBON_DEBUG_IPS=203.0.113.7

CORS_ORIGINS=http://localhost:3004,http://localhost:3005

UPLOAD_DIR=./uploads
UPLOAD_MAX_SIZE=10485760

SITE_NAME={{PROJECT_NAME_TITLE}}
SITE_URL=http://localhost:3005
BASE_URL=http://localhost:3005/

MAIL_FROM=noreply@localhost
SMTP_HOST=
SMTP_PORT=587
SMTP_USER=
SMTP_PASSWORD=
