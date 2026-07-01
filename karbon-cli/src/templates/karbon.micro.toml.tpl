[app]
name = "{{PROJECT_NAME_TITLE}}"

[backend]
package = "app"
port = 3005
watch = true

# Backend-only (micro) project — no [frontend] section.
# The Rust binary serves everything directly (API + welcome page).

[proxy]
backend_routes = ["/api", "/files", "/health"]
