# {{PROJECT_NAME_TITLE}}

Welcome to the documentation of **{{PROJECT_NAME_TITLE}}**.

This `docs/` folder holds the project's Markdown guides. They are:

- **shown live** in Studio (the **Docs** tab) during development;
- **built into a static site** with `karbon docs build` (→ `docs/_site/`).

## Getting started

```bash
karbon dev                 # backend + frontend, with hot reload
karbon generate crud Post  # scaffolds an entity + repository + controller
karbon generate auth       # sign-up / sign-in pages + JSON API
karbon migrate diff        # writes the migration from the entity ↔ database diff
karbon migrate             # applies the pending migrations
karbon user create you@example.com --admin   # your first administrator
karbon doctor              # diagnoses the project
```

## Writing docs

Add `.md` files to `docs/`: each one becomes a page (its title is the first
`# heading`). Rename or duplicate this one to start your own guides.

## API

The project's API documents itself:

- `/openapi.json` — the OpenAPI 3.0 document (aggregated from the controllers);
- `/docs` — the **Swagger UI** explorer.
