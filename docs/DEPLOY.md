# Déployer une application Karbon

Une app Karbon se compile en **un binaire Rust** (backend + reverse proxy intégré)
accompagné du **build frontend** (SvelteKit/Next.js servi en SSR par Node). Ce guide
couvre les cibles de déploiement, de la plus simple à la plus avancée.

> Pré-requis : `karbon build` produit `target/release/<app>` + le build frontend.
> En production, le binaire Rust sert l'API et proxie le reste vers le SSR Node
> quand `KARBON_FRONTEND_URL` est défini (ce que fait `karbon serve`).

---

## 1. VPS / serveur dédié (rsync + PM2 ou systemd)

Le plus direct quand tu as un accès SSH. Configure la section `[deploy]` de `karbon.toml` :

```toml
[deploy]
path = "/var/www/my-app"
user = "www-data"            # optionnel — chown après copie
manager = "pm2"              # "pm2" ou "systemd"
host = "user@server"         # optionnel — si absent, déploiement local
```

Puis :

```bash
karbon deploy publish:build   # build + rsync + restart
karbon deploy publish         # rsync + restart (sans rebuild)
```

Le binaire, le build frontend, les migrations et `.env.example` sont synchronisés,
les dépendances frontend installées (`npm install --omit=dev`), puis le process
manager redémarré.

---

## 2. Docker

```bash
karbon deploy docker          # génère Dockerfile + .dockerignore
docker build -t my-app .
docker run -p 3000:3000 --env-file .env my-app
```

Le Dockerfile généré est **multi-stage** (build frontend → build backend →
image runtime `debian-slim` avec Node pour le SSR) et inclut un `HEALTHCHECK`
sur `/health`.

> Note : une image `scratch`/`distroless` totalement statique n'est pas possible
> tant que le SSR Node est embarqué (le runtime Node est requis). Pour une app
> **API-only** (sans frontend SSR), le binaire Rust peut tourner seul dans une
> image minimale.

---

## 3. PaaS (Fly.io / Render / Railway)

```bash
karbon deploy paas            # génère fly.toml, render.yaml, Procfile (+ Dockerfile si absent)
```

- **Fly.io** : `fly launch --copy-config --no-deploy` (première fois), puis `fly deploy`.
- **Render** : connecte le repo à un Blueprint Render — `render.yaml` est détecté automatiquement.
- **Railway** : `railway up` (utilise le Dockerfile).

Tous pointent leur healthcheck sur `/health` et définissent `APP_ENV=production`.

Raccourcis (génèrent la config au besoin puis déploient via le CLI de la plateforme) :

```bash
karbon deploy fly       # → flyctl deploy
karbon deploy railway   # → railway up
```

### Multi-arch (amd64 + arm64)

Le Dockerfile généré fonctionne sous `buildx` pour les deux architectures :

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t my-app --push .
```

---

## 4. Variables d'environnement & environnements

Karbon charge les fichiers `.env` en cascade selon `APP_ENV` (style Symfony) :

```
.env.{env}.local   (priorité max)
.env.local         (ignoré en test)
.env.{env}
.env               (priorité min)
```

En production, mets `APP_ENV=production`. Les valeurs déjà présentes dans
l'environnement réel ne sont jamais écrasées par les fichiers.

Variables minimales requises : `DB_NAME`, `DB_USER`, `JWT_SECRET` (voir `.env.example`).

---

## 5. Migrations

```bash
karbon migrate                # exécute les .sql de migration/ dans l'ordre alphabétique
```

Lit `DATABASE_URL` ou les variables `DB_*`. Supporte MySQL/MariaDB (`mysql` CLI)
et PostgreSQL (`psql` CLI).

---

## 6. Hébergement mutualisé

⚠️ Un mutualisé classique ne laisse pas tourner un process Rust persistant. Le
support **FastCGI** est en réflexion et n'est pas encore disponible. Pour
l'instant, privilégie un VPS, Docker ou un PaaS.
