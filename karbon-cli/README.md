# karbon-cli

Unified CLI for the [Karbon](https://github.com/larevuegeek/karbon) full-stack Rust framework.

## Install

```bash
cargo install karbon-cli
```

## Commands

| Command | Description |
|---------|-------------|
| `karbon new <name>` | Create a new project (SvelteKit by default) |
| `karbon new <name> --frontend nextjs` | Create a new project with Next.js |
| `karbon dev` | Start dev servers (Rust + Vite, hot-reload) |
| `karbon build` | Build for production |
| `karbon serve` | Run production build (single port, reverse proxy) |
| `karbon generate entity <Name>` | Generate entity + migration |
| `karbon generate controller <Name>` | Generate admin controller |
| `karbon generate crud <Name>` | Generate entity + repo + controller + migration |
| `karbon g crud <Name>` | Short alias |

## Usage

```bash
# Create and start a new project
karbon new my-app
cd my-app
karbon dev

# Generate a full CRUD (entity + repo + controller + migration)
karbon g crud Post

# Build and serve in production
karbon build
karbon serve
```

## Documentation

Full documentation: [github.com/larevuegeek/karbon](https://github.com/larevuegeek/karbon)

## License

AGPL-3.0-or-later — see [LICENSE](https://github.com/larevuegeek/karbon/blob/main/LICENSE)
