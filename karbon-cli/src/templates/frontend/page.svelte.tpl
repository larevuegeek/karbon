<script>
	import { onMount } from 'svelte';

	let info = null;
	let loaded = false;
	let copied = null;

	onMount(async () => {
		try {
			const res = await fetch('/_studio/api/info', { headers: { accept: 'application/json' } });
			if (res.ok) info = await res.json();
		} catch (_) {
			/* Studio not available (prod / feature off) — page stays static. */
		}
		loaded = true;
	});

	async function copy(text, key) {
		try {
			await navigator.clipboard.writeText(text);
			copied = key;
			setTimeout(() => (copied === key ? (copied = null) : null), 1400);
		} catch (_) {}
	}

	// Onboarding steps — auto-checked from the live app state.
	$: steps = [
		{
			key: 'run',
			label: 'App running',
			desc: 'Your Rust + SvelteKit dev server is live with hot-reload.',
			done: true
		},
		{
			key: 'studio',
			label: 'Open Studio',
			desc: 'Live dev cockpit: metrics, schema, routes & an integrated terminal.',
			done: loaded && !!info,
			href: '/_studio'
		},
		{
			key: 'db',
			label: 'Connect a database',
			desc: info && info.database ? 'Connected.' : 'Set DB_NAME in .env, then run the migrations.',
			done: !!(info && info.database),
			cmd: 'karbon migrate'
		},
		{
			key: 'crud',
			label: 'Scaffold your first resource',
			desc:
				info && info.tables && info.tables.length
					? 'Entity, repository & controller wired in.'
					: 'Generate an entity, repository & controller in one command.',
			done: !!(info && info.tables && info.tables.length),
			cmd: 'karbon generate crud Post title:string body:text'
		}
	];
	$: doneCount = steps.filter((s) => s.done).length;
	$: pct = Math.round((doneCount / steps.length) * 100);
</script>

<svelte:head>
	<title>{{PROJECT_NAME_TITLE}} — Karbon</title>
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="" />
	<link
		href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap"
		rel="stylesheet"
	/>
</svelte:head>

<div class="kb-page">
	<main class="kb-card">
		<div class="kb-head">
			<div class="kb-logo">
				<svg width="46" height="46" viewBox="0 0 32 32" fill="none" aria-hidden="true">
					<defs>
						<linearGradient id="kbHex" x1="2" y1="2" x2="30" y2="30" gradientUnits="userSpaceOnUse">
							<stop stop-color="#8b7cff" />
							<stop offset="1" stop-color="#22d3ee" />
						</linearGradient>
					</defs>
					<path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="url(#kbHex)" />
					<path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="none" stroke="#fff" stroke-opacity=".18" stroke-width="1" />
					<path d="M12.5 10v12M12.5 16l6.2-6M12.5 16l6.2 6" stroke="#fff" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round" />
				</svg>
			</div>
			<span class="kb-badge"><span class="kb-dot"></span> running · {info ? info.environment : 'dev'}</span>
		</div>

		<h1>Welcome to <span>{{PROJECT_NAME_TITLE}}</span></h1>
		<p class="kb-lead">
			Your Karbon full-stack app (Rust&nbsp;+&nbsp;SvelteKit) is up and running.
			Follow the steps below to build your first feature.
		</p>

		<div class="kb-onboard">
			<div class="kb-onboard-top">
				<span class="kb-onboard-title">Getting started</span>
				<span class="kb-onboard-count">{doneCount} / {steps.length} done</span>
			</div>
			<div class="kb-progress"><i style="width:{pct}%"></i></div>

			<ol class="kb-steps">
				{#each steps as s (s.key)}
					<li class="kb-step" class:done={s.done}>
						<span class="kb-check" aria-hidden="true">
							{#if s.done}
								<svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
							{/if}
						</span>
						<div class="kb-step-body">
							<div class="kb-step-h">
								<b>{s.label}</b>
								{#if s.href}
									<a class="kb-step-link" href={s.href} target="_blank" rel="noreferrer">Open →</a>
								{/if}
							</div>
							<p>{s.desc}</p>
							{#if s.cmd}
								<button class="kb-cmd" on:click={() => copy(s.cmd, s.key)} title="Copy command">
									<code>{s.cmd}</code>
									<span class="kb-copy">{copied === s.key ? 'Copied ✓' : 'Copy'}</span>
								</button>
							{/if}
						</div>
					</li>
				{/each}
			</ol>
		</div>

		<div class="kb-links">
			<a href="/_studio" target="_blank" rel="noreferrer">Studio</a>
			<a href="/docs" target="_blank" rel="noreferrer">API docs</a>
			<a href="/_studio#docs" target="_blank" rel="noreferrer">Guides</a>
			<a href="/health" target="_blank" rel="noreferrer">/health</a>
		</div>
	</main>

	<footer class="kb-bar">
		<div class="kb-chip">
			<span class="kb-bar-brand">◆ karbon</span>
			<span class="kb-ver">{info ? 'v' + info.version : '…'}</span>
			<div class="kb-panel">
				<div class="kb-ph">◆ Karbon framework</div>
				<div class="kb-prow"><span>Version</span><b>{info ? info.version : '—'}</b></div>
				<div class="kb-prow"><span>Environment</span><b>{info ? info.environment : '—'}</b></div>
				<div class="kb-prow"><span>Database</span><b>{info ? (info.database ? 'connected' : 'none') : '—'}</b></div>
				<div class="kb-prow kb-pcol">
					<span>Features</span>
					<div class="kb-tags">
						{#if info && info.features && info.features.length}
							{#each info.features as f}<em class="kb-tag">{f}</em>{/each}
						{:else}<b>—</b>{/if}
					</div>
				</div>
				{#if info && info.tables && info.tables.length}
					<div class="kb-prow kb-pcol">
						<span>Entities</span>
						<div class="kb-tags">
							{#each info.tables as t}<em class="kb-tag kb-tag-ent">{t}</em>{/each}
						</div>
					</div>
				{/if}
				<a class="kb-popen" href="/_studio" target="_blank" rel="noreferrer">Open Studio dashboard →</a>
			</div>
		</div>
		<span class="kb-env">{info ? info.environment : 'dev'}</span>
		<span class="kb-bar-right">Rust&nbsp;Axum&nbsp;·&nbsp;SvelteKit</span>
	</footer>
</div>

<style>
	:global(body) {
		margin: 0;
		background: #06070d;
	}
	.kb-page {
		position: relative;
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 2rem 1rem 4.5rem;
		color: #f3f4fa;
		font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
		letter-spacing: -0.011em;
		background:
			radial-gradient(640px 520px at 10% -8%, rgba(139, 124, 255, 0.2), transparent 60%),
			radial-gradient(720px 560px at 100% -4%, rgba(34, 211, 238, 0.14), transparent 58%),
			linear-gradient(180deg, #080a13, #06070d);
	}
	/* faint grid overlay */
	.kb-page::before {
		content: '';
		position: fixed;
		inset: 0;
		z-index: 0;
		pointer-events: none;
		opacity: 0.5;
		background-image:
			linear-gradient(rgba(255, 255, 255, 0.02) 1px, transparent 1px),
			linear-gradient(90deg, rgba(255, 255, 255, 0.02) 1px, transparent 1px);
		background-size: 60px 60px;
		-webkit-mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%);
		mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%);
	}
	.kb-card {
		position: relative;
		z-index: 1;
		width: 100%;
		max-width: 680px;
		background: rgba(255, 255, 255, 0.03);
		backdrop-filter: blur(18px);
		border: 1px solid rgba(255, 255, 255, 0.09);
		border-radius: 24px;
		padding: 2.75rem 2.5rem 2.25rem;
		box-shadow:
			0 1px 0 rgba(255, 255, 255, 0.05) inset,
			0 40px 90px -35px rgba(0, 0, 0, 0.85);
		text-align: center;
		animation: kb-rise 0.55s cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}
	@keyframes kb-rise {
		from { opacity: 0; transform: translateY(14px); }
		to { opacity: 1; transform: translateY(0); }
	}
	.kb-head {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.85rem;
	}
	.kb-logo {
		display: inline-grid;
		place-items: center;
		filter: drop-shadow(0 12px 30px rgba(124, 92, 255, 0.6));
		animation: kb-float 4s ease-in-out infinite;
	}
	@keyframes kb-float {
		50% { transform: translateY(-5px); }
	}
	.kb-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.32rem 0.85rem;
		font-size: 0.76rem;
		font-weight: 600;
		color: #6ee7b7;
		background: rgba(52, 211, 153, 0.1);
		border: 1px solid rgba(52, 211, 153, 0.22);
		border-radius: 999px;
	}
	.kb-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: #34d399;
		box-shadow: 0 0 0 3px rgba(52, 211, 153, 0.2);
		animation: kb-pulse 1.8s ease-in-out infinite;
	}
	@keyframes kb-pulse {
		50% { box-shadow: 0 0 0 6px rgba(52, 211, 153, 0); }
	}
	h1 {
		margin: 1.1rem 0 0.5rem;
		font-size: 2.3rem;
		font-weight: 800;
		letter-spacing: -0.032em;
		color: #f6f7fc;
	}
	h1 span {
		background: linear-gradient(120deg, #8b7cff, #22d3ee);
		-webkit-background-clip: text;
		background-clip: text;
		-webkit-text-fill-color: transparent;
	}
	.kb-lead {
		margin: 0 auto 1.9rem;
		max-width: 46ch;
		color: #aab0c6;
		line-height: 1.6;
	}

	/* Onboarding */
	.kb-onboard {
		text-align: left;
		background: rgba(255, 255, 255, 0.022);
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-radius: 18px;
		padding: 1.15rem 1.25rem 1.3rem;
	}
	.kb-onboard-top {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}
	.kb-onboard-title {
		font-weight: 700;
		font-size: 0.95rem;
		letter-spacing: -0.01em;
	}
	.kb-onboard-count {
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		font-size: 0.78rem;
		color: #8b92ad;
	}
	.kb-progress {
		height: 5px;
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.07);
		margin: 0.6rem 0 0.9rem;
		overflow: hidden;
	}
	.kb-progress i {
		display: block;
		height: 100%;
		border-radius: 999px;
		background: linear-gradient(90deg, #8b7cff, #22d3ee);
		transition: width 0.5s cubic-bezier(0.2, 0.7, 0.2, 1);
	}
	.kb-steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.kb-step {
		display: flex;
		gap: 0.7rem;
		padding: 0.6rem 0.2rem;
		border-top: 1px solid rgba(255, 255, 255, 0.055);
	}
	.kb-step:first-child {
		border-top: 0;
	}
	.kb-check {
		flex-shrink: 0;
		width: 20px;
		height: 20px;
		margin-top: 1px;
		display: grid;
		place-items: center;
		border-radius: 999px;
		border: 1.5px solid rgba(255, 255, 255, 0.18);
		color: #06070d;
	}
	.kb-step.done .kb-check {
		background: linear-gradient(120deg, #8b7cff, #22d3ee);
		border-color: transparent;
	}
	.kb-step-body {
		flex: 1;
		min-width: 0;
	}
	.kb-step-h {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.kb-step-h b {
		font-size: 0.94rem;
		font-weight: 600;
		color: #eef0f8;
	}
	.kb-step.done .kb-step-h b {
		color: #aab0c6;
	}
	.kb-step-link {
		margin-left: auto;
		font-size: 0.82rem;
		font-weight: 600;
		color: #b3a7ff;
		text-decoration: none;
	}
	.kb-step-link:hover {
		color: #22d3ee;
	}
	.kb-step-body p {
		margin: 0.15rem 0 0;
		font-size: 0.83rem;
		color: #838aa3;
		line-height: 1.5;
	}
	.kb-cmd {
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
		margin-top: 0.55rem;
		padding: 0.34rem 0.4rem 0.34rem 0.7rem;
		background: rgba(139, 124, 255, 0.08);
		border: 1px solid rgba(139, 124, 255, 0.22);
		border-radius: 9px;
		cursor: pointer;
		font-family: inherit;
		transition: border-color 0.14s, background 0.14s;
		max-width: 100%;
	}
	.kb-cmd:hover {
		border-color: rgba(139, 124, 255, 0.5);
		background: rgba(139, 124, 255, 0.13);
	}
	.kb-cmd code {
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		font-size: 0.78rem;
		color: #cabfff;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.kb-copy {
		flex-shrink: 0;
		font-size: 0.68rem;
		font-weight: 600;
		color: #8b92ad;
		background: rgba(255, 255, 255, 0.06);
		border-radius: 6px;
		padding: 0.12rem 0.4rem;
	}
	.kb-cmd:hover .kb-copy {
		color: #e8eaf2;
	}

	.kb-links {
		display: flex;
		flex-wrap: wrap;
		gap: 1.2rem;
		justify-content: center;
		margin-top: 1.5rem;
		font-size: 0.85rem;
	}
	.kb-links a {
		color: #8b92ad;
		text-decoration: none;
	}
	.kb-links a:hover {
		color: #f3f4fa;
	}

	/* Bottom profiler bar */
	.kb-bar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		z-index: 9999;
		display: flex;
		align-items: center;
		gap: 1rem;
		height: 36px;
		padding: 0 0.95rem;
		font: 12px/36px 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
		background: rgba(6, 7, 13, 0.92);
		backdrop-filter: blur(10px);
		color: #c7ccdf;
		border-top: 1px solid rgba(255, 255, 255, 0.08);
	}
	.kb-bar a {
		color: #b3a7ff;
		text-decoration: none;
	}
	.kb-bar a:hover {
		color: #22d3ee;
	}
	.kb-chip {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		cursor: default;
	}
	.kb-bar-brand {
		font-weight: 700;
		background: linear-gradient(120deg, #8b7cff, #22d3ee);
		-webkit-background-clip: text;
		background-clip: text;
		-webkit-text-fill-color: transparent;
	}
	.kb-ver {
		color: #6c7392;
	}
	.kb-env {
		padding: 0 0.45rem;
		background: rgba(255, 255, 255, 0.06);
		border-radius: 5px;
		color: #aab0c6;
		text-transform: lowercase;
	}
	.kb-bar-right {
		margin-left: auto;
		color: #5b6180;
	}
	.kb-panel {
		display: none;
		position: absolute;
		bottom: 42px;
		left: 0;
		width: 370px;
		max-height: 62vh;
		overflow: auto;
		padding: 14px 16px;
		background: #0e1019;
		color: #e8eaf2;
		border: 1px solid rgba(255, 255, 255, 0.12);
		border-radius: 14px;
		box-shadow: 0 22px 60px -16px rgba(0, 0, 0, 0.75);
		line-height: 1.55;
	}
	.kb-chip:hover .kb-panel {
		display: block;
	}
	.kb-ph {
		font-weight: 700;
		margin-bottom: 8px;
		background: linear-gradient(120deg, #8b7cff, #22d3ee);
		-webkit-background-clip: text;
		background-clip: text;
		-webkit-text-fill-color: transparent;
	}
	.kb-prow {
		display: flex;
		justify-content: space-between;
		gap: 14px;
		padding: 3px 0;
	}
	.kb-prow > span {
		color: #8b92ad;
	}
	.kb-prow b {
		font-weight: 600;
		text-align: right;
		word-break: break-all;
		color: #e8eaf2;
	}
	.kb-pcol {
		flex-direction: column;
		gap: 6px;
	}
	.kb-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 5px;
	}
	.kb-tag {
		font-style: normal;
		font-size: 11px;
		padding: 2px 8px;
		background: rgba(139, 124, 255, 0.14);
		border-radius: 999px;
		color: #c7c0ff;
	}
	.kb-tag-ent {
		background: rgba(34, 211, 238, 0.14);
		color: #7dd3fc;
	}
	.kb-popen {
		display: inline-block;
		margin-top: 11px;
		color: #b3a7ff;
	}

	@media (max-width: 560px) {
		.kb-card {
			padding: 2.2rem 1.4rem 1.6rem;
		}
		h1 {
			font-size: 2rem;
		}
	}
</style>
