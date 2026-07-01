# Publier Karbon sur crates.io

> Processus de release des 3 crates Karbon. **État : `0.2.30` publié ; `0.3.0` prêt à
> publier** (`karbon-macros`, `karbon-framework`, `karbon-cli`). Le passage à `0.3`
> (bump **mineur**) livre le durcissement sécurité + le firewall et **plusieurs
> changements de défaut cassants** (cf. `CHANGELOG.md`).

## 1. Crates & ordre de publication

Trois crates, publiées **dans cet ordre** (une dépendance doit exister sur l'index
avant la crate qui en dépend) :

1. **`karbon-macros`** — proc-macros, aucune dépendance interne.
2. **`karbon-framework`** — dépend de `karbon-macros` (`version = "0.3"`).
3. **`karbon-cli`** — indépendante (n'utilise ni le framework ni les macros), mais
   publiée en dernier par convention.

> `examples/minimal` est en `publish = false` — jamais publié.

## 2. Pré-requis (une fois)

```bash
cargo login            # token depuis https://crates.io/settings/tokens
```
Le compte doit être **owner** des 3 crates (déjà le cas pour `0.2.30`).

## 3. Couper une nouvelle version

1. **Bump la version** du workspace dans `Cargo.toml` racine :
   ```toml
   [workspace.package]
   version = "0.3.0"   # ← version courante à couper
   ```
   Les 3 crates héritent via `version.workspace = true`.

2. **Vérifier la dépendance inter-crate** : `karbon-framework/Cargo.toml` →
   `karbon-macros = { version = "0.3", path = "../karbon-macros" }`. Le caret `"0.3"`
   (`>=0.3.0, <0.4.0`) doit correspondre à la série courante — à ré-incrémenter au
   prochain bump majeur de série (`0.4`, `1.0`…).

3. **Mettre à jour le `CHANGELOG.md`** : déplacer la section `[Unreleased]` sous le
   nouveau numéro + date.

4. **Vérifs locales** (doivent toutes passer) :
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo test -p karbon-cli --test scaffold -- --ignored   # compile e2e
   ```

5. **Dry-run du packaging** (dans l'ordre) :
   ```bash
   cargo publish -p karbon-macros    --dry-run
   cargo publish -p karbon-framework --dry-run --no-verify   # macros pas encore en ligne → --no-verify
   cargo publish -p karbon-cli       --dry-run
   ```
   `--no-verify` sur le framework saute la recompilation isolée (qui exigerait la
   nouvelle version des macros déjà publiée). Vérifier la liste de fichiers avec
   `cargo package -p <crate> --list` (LICENSE + README doivent y être).

## 4. Publier

```bash
cargo publish -p karbon-macros
# attendre ~30 s que l'index se propage, puis :
cargo publish -p karbon-framework
cargo publish -p karbon-cli
```

## 5. Après publication

1. **Le template pointe sur `version = "0.3"`**
   (`karbon-cli/src/templates/rust/Cargo.app.toml.tpl`) → un `karbon new` **sans
   `--local`** récupère le framework publié à jour. À ré-incrémenter au prochain
   changement de série (`0.4`…).
2. **Tag git** : `git tag v0.3.0 && git push --tags`.
3. **Vérifier** : `cargo install karbon-cli` puis `karbon new demo` → `cd demo && cargo run`.
4. **doc.rs** se construit automatiquement (métadonnées `[package.metadata.docs.rs]`).

## 6. Points de vigilance

- **`--local` reste l'outil de dev** contre ce dépôt (path dep + feature `studio`).
  Sans `--local`, on dépend de la **dernière version publiée** — d'où l'importance
  de publier régulièrement pour que le scaffolding non-local reflète le code récent.
- **Base de données optionnelle / Studio par défaut** : si l'on veut que
  `karbon new` sans `--local` tourne *sans* base et avec Studio out-of-the-box, il
  faut que ces comportements soient **publiés** (pas seulement dans le working tree).
  Vérifier qu'ils sont dans la version publiée avant d'activer `studio` par défaut
  dans le template de base.
- **AGPL-3.0** : licence déclarée via `license-file = "LICENSE"`. crates.io l'accepte ;
  la page affichera « non-standard » — passer à `license.workspace = true`
  (SPDX `AGPL-3.0-or-later`) si l'on veut l'affichage standard (optionnel).
