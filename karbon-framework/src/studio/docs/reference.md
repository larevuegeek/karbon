# Karbon — référence

Référence rapide du framework, rendue en direct dans Studio. Elle évolue avec le code.

## CLI

| Commande | Rôle |
|----------|------|
| `karbon new <app>` | Nouveau projet (`--frontend svelte\|nextjs`, `--skeleton full\|micro`, `--template blog`, `--local <repo>`) |
| `karbon dev` | Dev (backend + frontend, hot-reload) |
| `karbon build` / `karbon serve` | Build prod / lancer la prod (reverse proxy intégré) |
| `karbon generate <kind> <Name>` | Génère `entity` / `controller` / `crud` / `admin` (`--dry-run`, `--force`) |
| `karbon migrate [status\|rollback\|diff]` | Migrations versionnées ; `diff` = entités ↔ base |
| `karbon doctor` | Diagnostic des pièges courants (offline) |
| `karbon docs build` | Rend `docs/*.md` → site statique `docs/_site/` |
| `karbon deploy <docker\|paas\|fly\|railway\|publish>` | Déploiement |

## Controllers

```rust
#[karbon::controller(prefix = "/admin/posts", role = "ROLE_ADMIN")]
impl PostController {
    #[karbon::get("/")]
    async fn list(auth: AuthGuard, State(state): State<AppState>) -> AppResult<impl IntoResponse> { /* … */ }
}
```

La macro génère `router()`, `prefix()` et `openapi_paths()`. Les routes générées sont
montées automatiquement dans `main.rs` (marqueurs `// karbon:routes` / `// karbon:api-routes`).

**Protection par route** : `role = "…"` (contrôleur) ou `#[require_role("…")]` (route, prioritaire).
Le contrôle utilise le paramètre `auth: AuthGuard` du handler ; si une route porte un rôle **sans**
`auth: AuthGuard`, **le macro refuse de compiler** (pas de route non protégée en silence).

**Firewall global** : `App::firewall(AccessControl::new().public("^/login").rule("^/admin", ["ROLE_ADMIN"]))`
— règles pattern d'URL → rôles, appliquées centralement (première règle qui matche gagne).

**Validation des paramètres de route** (déclarative, appliquée en 400 avant le handler) :

```rust
#[karbon::get("/{id}", id = "int:min=1")]                 // entier ≥ 1
#[karbon::get("/{slug}", slug = "slug")]                   // ^[a-z0-9-]+$
#[karbon::get("/{status}", status = "enum:draft|published")]
#[karbon::get("/{code}", code = "regex:^[A-Z]{2}[0-9]{2}$")]
```
Règles : `int[:min=N][:max=N]`, `string[:minlen=N][:maxlen=N]`, `slug`, `uuid`,
`enum:a|b|c`, `regex:^…$`. Le nom du binding doit correspondre au nom du paramètre de chemin.

## Base de données

- Multi-driver **MySQL** (défaut) / **PostgreSQL** / **SQLite** (feature flags).
- `CrudRepository` : `find_by_id`, `find_all`, `find_where`, `delete`, soft-delete, slug…
- `SelectBuilder` : query builder typé et paramétré (anti-injection).
- `Insertable` / `Updatable` (derive) : INSERT/UPDATE avec `#[timestamps]`, `#[slug_from]`.
- Relations : `has_many`, belongs-to via `find_by_id`, `load_grouped_by` (anti N+1).
- **Base optionnelle** : `DB_NAME` vide → l'app démarre sans se connecter.
- **Migrations par diff** : les entités sont la source de vérité ; `karbon migrate diff`
  compare entités ↔ base et génère la migration (`CREATE`/`ALTER ADD`, additif).

## Sécurité

Argon2id, JWT (access 15 min + refresh 30 j hashé SHA-256), CSRF (double-submit),
AuthGuard + hiérarchie de rôles, upload sécurisé (magic bytes, anti path-traversal,
sanitization SVG), CORS, compression, request-id, graceful shutdown.

## Composants

- **Forms** (`form::Form`/`Field`) : binding, validation, rendu HTML, CSRF.
- **Validator** composable : agrège toutes les violations, imbriqué, groupes.
- **Message bus** (`MessageBus`) : sync + async, retry/backoff, dead-letter.
- **Events** (`EventBus`), **Jobs** (`JobQueue`, `PersistentQueue`).
- **Cache** : mémoire / fichier / Redis, tags, cache HTTP (ETag).
- **Templates** : Tera, MiniJinja, ou moteur maison (`native-templates`).
- **i18n**, **mail** (lettre), **scraping**, **images** (`ImageProcessor`).
- **Realtime** : `ChannelRegistry` (rooms WebSocket), HMR, LiveWire, Inertia.

## Studio (dev)

Dashboard temps réel (feature `studio`, dev only) : requêtes, events, jobs, mails,
**Overview** (perf), **Database** (schéma), **Terminal** (commandes CLI whitelistées + Maker),
**Docs** (cette page). Token + localhost. Toolbar injectée en bas des pages HTML.

## Extensibilité

- `http::Module` — contribue des routes.
- `http::Bundle` — routes + hook `boot(&AppState)` au démarrage (plugin compile-time).
