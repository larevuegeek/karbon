const API_URL = import.meta.env.SSR
	? (process.env.API_URL || 'http://localhost:3005')
	: '';

export async function api<T>(endpoint: string, options?: RequestInit): Promise<T> {
	const url = `${API_URL}/api/v1${endpoint}`;
	const res = await fetch(url, {
		headers: { 'Content-Type': 'application/json', ...options?.headers },
		...options
	});
	if (!res.ok) throw new Error(await res.text());
	return res.json();
}
