import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 3004,
		proxy: {
			'/api': 'http://localhost:3005',
			'/files': 'http://localhost:3005',
			'/health': 'http://localhost:3005',
			'/admin': 'http://localhost:3005',
			'/docs': 'http://localhost:3005',
			'/openapi.json': 'http://localhost:3005',
			'/_studio': {
				target: 'http://localhost:3005',
				ws: true,
				// Studio's WebSocket resets when the backend recompiles (karbon dev
				// watcher); swallow the benign ECONNRESET so it doesn't spam the console.
				configure: (proxy: any) => {
					proxy.on('error', (err: any) => {
						if (err && err.code !== 'ECONNRESET') {
							console.error('[studio proxy]', err.message);
						}
					});
				}
			}
		}
	}
});
