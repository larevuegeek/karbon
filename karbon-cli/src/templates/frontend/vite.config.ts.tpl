import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 3004,
		proxy: {
			'/api': 'http://localhost:3005',
			'/files': 'http://localhost:3005'
		}
	}
});
