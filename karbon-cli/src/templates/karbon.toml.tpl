[app]
name = "{{PROJECT_NAME_TITLE}}"

[backend]
package = "app"
port = 3005
watch = true

[frontend]
dir = "frontend"
port = 3004
dev_cmd = "npm run dev"
build_cmd = "npm run build"
serve_cmd = "node build/index.js"

[proxy]
backend_routes = ["/api", "/files", "/health"]
