/// All templates are embedded at compile time via include_str!

pub const KARBON_TOML: &str = include_str!("karbon.toml.tpl");
pub const ENV_EXAMPLE: &str = include_str!("env.example.tpl");
pub const GITIGNORE: &str = include_str!("gitignore.tpl");

// Rust
pub const CARGO_WORKSPACE: &str = include_str!("rust/Cargo.workspace.toml.tpl");
pub const CARGO_APP: &str = include_str!("rust/Cargo.app.toml.tpl");
pub const MAIN_RS: &str = include_str!("rust/main.rs.tpl");
pub const CONTROLLER_MOD: &str = include_str!("rust/controller_mod.rs.tpl");
pub const ENTITY_MOD: &str = include_str!("rust/entity_mod.rs.tpl");
pub const HEALTH_CONTROLLER: &str = include_str!("rust/health_controller.rs.tpl");

// Frontend — SvelteKit
pub mod svelte {
    pub const PACKAGE_JSON: &str = include_str!("frontend/package.json.tpl");
    pub const SVELTE_CONFIG: &str = include_str!("frontend/svelte.config.js.tpl");
    pub const VITE_CONFIG: &str = include_str!("frontend/vite.config.ts.tpl");
    pub const APP_CSS: &str = include_str!("frontend/app.css.tpl");
    pub const LAYOUT: &str = include_str!("frontend/layout.svelte.tpl");
    pub const PAGE: &str = include_str!("frontend/page.svelte.tpl");
    pub const API_TS: &str = include_str!("frontend/api.ts.tpl");
    pub const HOOKS_SERVER: &str = include_str!("frontend/hooks.server.ts.tpl");
    pub const APP_D_TS: &str = include_str!("frontend/app.d.ts.tpl");
    pub const TSCONFIG: &str = include_str!("frontend/tsconfig.json.tpl");
}

// Frontend — Next.js
pub mod nextjs {
    pub const PACKAGE_JSON: &str = include_str!("nextjs/package.json.tpl");
    pub const NEXT_CONFIG: &str = include_str!("nextjs/next.config.ts.tpl");
    pub const GLOBALS_CSS: &str = include_str!("nextjs/globals.css.tpl");
    pub const LAYOUT: &str = include_str!("nextjs/layout.tsx.tpl");
    pub const PAGE: &str = include_str!("nextjs/page.tsx.tpl");
    pub const API_TS: &str = include_str!("nextjs/api.ts.tpl");
    pub const TSCONFIG: &str = include_str!("nextjs/tsconfig.json.tpl");
}
