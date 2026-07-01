# {{PROJECT_NAME_TITLE}}

Bienvenue dans la documentation de **{{PROJECT_NAME_TITLE}}**.

Ce dossier `docs/` contient les guides Markdown du projet. Ils sont :

- **affichés en direct** dans Studio (onglet **Docs**) en développement ;
- **compilés en site statique** avec `karbon docs build` (→ `docs/_site/`).

## Démarrer

```bash
karbon dev                 # backend + frontend en hot-reload
karbon generate crud Post  # scaffolde une entité + repo + contrôleur + migration
karbon migrate             # applique les migrations
karbon doctor              # diagnostique le projet
```

## Écrire la doc

Ajoute des fichiers `.md` dans `docs/` : chacun devient une page (le titre vient du
premier `# titre`). Renomme/duplique celui-ci pour créer tes propres guides.

## API

L'API du projet est documentée automatiquement :

- `/openapi.json` — le document OpenAPI 3.0 (agrégé depuis les contrôleurs) ;
- `/docs` — l'explorateur **Swagger UI**.
