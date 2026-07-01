'use client';

import { useEffect, useState } from 'react';

type Info = {
  version?: string;
  environment?: string;
  database?: boolean;
  features?: string[];
  tables?: string[];
};

export default function Home() {
  const [info, setInfo] = useState<Info | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    fetch('/_studio/api/info', { headers: { accept: 'application/json' } })
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => setInfo(d))
      .catch(() => {})
      .finally(() => setLoaded(true));
  }, []);

  const copy = (text: string, key: string) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(key);
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 1400);
    });
  };

  const steps = [
    { key: 'run', label: 'App running', desc: 'Your Rust + Next.js dev server is live with hot-reload.', done: true },
    { key: 'studio', label: 'Open Studio', desc: 'Live dev cockpit: metrics, schema, routes & an integrated terminal.', done: loaded && !!info, href: '/_studio' },
    { key: 'db', label: 'Connect a database', desc: info?.database ? 'Connected.' : 'Set DB_NAME in .env, then run the migrations.', done: !!info?.database, cmd: 'karbon migrate' },
    { key: 'crud', label: 'Scaffold your first resource', desc: info?.tables?.length ? 'Entity, repository & controller wired in.' : 'Generate an entity, repository & controller in one command.', done: !!info?.tables?.length, cmd: 'karbon generate crud Post title:string body:text' },
  ];
  const doneCount = steps.filter((s) => s.done).length;
  const pct = Math.round((doneCount / steps.length) * 100);

  return (
    <div className="kb-page">
      <style>{CSS}</style>
      <main className="kb-card">
        <div className="kb-head">
          <div className="kb-logo">
            <svg width="46" height="46" viewBox="0 0 32 32" fill="none" aria-hidden="true">
              <defs>
                <linearGradient id="kbHex" x1="2" y1="2" x2="30" y2="30" gradientUnits="userSpaceOnUse">
                  <stop stopColor="#8b7cff" />
                  <stop offset="1" stopColor="#22d3ee" />
                </linearGradient>
              </defs>
              <path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="url(#kbHex)" />
              <path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="none" stroke="#fff" strokeOpacity=".18" strokeWidth="1" />
              <path d="M12.5 10v12M12.5 16l6.2-6M12.5 16l6.2 6" stroke="#fff" strokeWidth="2.3" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </div>
          <span className="kb-badge"><span className="kb-dot" /> running · {info ? info.environment : 'dev'}</span>
        </div>

        <h1>Welcome to <span>{{PROJECT_NAME_TITLE}}</span></h1>
        <p className="kb-lead">
          Your Karbon full-stack app (Rust&nbsp;+&nbsp;Next.js) is up and running. Follow the
          steps below to build your first feature.
        </p>

        <div className="kb-onboard">
          <div className="kb-onboard-top">
            <span className="kb-onboard-title">Getting started</span>
            <span className="kb-onboard-count">{doneCount} / {steps.length} done</span>
          </div>
          <div className="kb-progress"><i style={{ width: pct + '%' }} /></div>
          <ol className="kb-steps">
            {steps.map((s) => (
              <li key={s.key} className={'kb-step' + (s.done ? ' done' : '')}>
                <span className="kb-check" aria-hidden="true">
                  {s.done && (
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
                  )}
                </span>
                <div className="kb-step-body">
                  <div className="kb-step-h">
                    <b>{s.label}</b>
                    {s.href && <a className="kb-step-link" href={s.href} target="_blank" rel="noreferrer">Open →</a>}
                  </div>
                  <p>{s.desc}</p>
                  {s.cmd && (
                    <button className="kb-cmd" onClick={() => copy(s.cmd!, s.key)} title="Copy command">
                      <code>{s.cmd}</code>
                      <span className="kb-copy">{copied === s.key ? 'Copied ✓' : 'Copy'}</span>
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ol>
        </div>

        <div className="kb-links">
          <a href="/_studio" target="_blank" rel="noreferrer">Studio</a>
          <a href="/docs" target="_blank" rel="noreferrer">API docs</a>
          <a href="/_studio#docs" target="_blank" rel="noreferrer">Guides</a>
          <a href="/health" target="_blank" rel="noreferrer">/health</a>
        </div>
      </main>
    </div>
  );
}

const CSS = `
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap');
body { margin: 0; background: #06070d; }
.kb-page { position: relative; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 2rem 1rem 3rem; color: #f3f4fa; font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif; letter-spacing: -.011em; background: radial-gradient(640px 520px at 10% -8%, rgba(139,124,255,.2), transparent 60%), radial-gradient(720px 560px at 100% -4%, rgba(34,211,238,.14), transparent 58%), linear-gradient(180deg,#080a13,#06070d); }
.kb-page::before { content:''; position:fixed; inset:0; z-index:0; pointer-events:none; opacity:.5; background-image: linear-gradient(rgba(255,255,255,.02) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,.02) 1px, transparent 1px); background-size:60px 60px; -webkit-mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%); mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%); }
.kb-card { position: relative; z-index: 1; width: 100%; max-width: 680px; background: rgba(255,255,255,.03); backdrop-filter: blur(18px); border: 1px solid rgba(255,255,255,.09); border-radius: 24px; padding: 2.75rem 2.5rem 2.25rem; text-align: center; box-shadow: 0 1px 0 rgba(255,255,255,.05) inset, 0 40px 90px -35px rgba(0,0,0,.85); }
.kb-head { display: flex; flex-direction: column; align-items: center; gap: .85rem; }
.kb-logo { display: inline-grid; place-items: center; filter: drop-shadow(0 12px 30px rgba(124,92,255,.6)); }
.kb-badge { display: inline-flex; align-items: center; gap: .45rem; padding: .32rem .85rem; font-size: .76rem; font-weight: 600; color: #6ee7b7; background: rgba(52,211,153,.1); border: 1px solid rgba(52,211,153,.22); border-radius: 999px; }
.kb-dot { width: 7px; height: 7px; border-radius: 50%; background: #34d399; box-shadow: 0 0 0 3px rgba(52,211,153,.2); }
h1 { margin: 1.1rem 0 .5rem; font-size: 2.3rem; font-weight: 800; letter-spacing: -.032em; color: #f6f7fc; }
h1 span { background: linear-gradient(120deg,#8b7cff,#22d3ee); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; }
.kb-lead { margin: 0 auto 1.9rem; max-width: 46ch; color: #aab0c6; line-height: 1.6; }
.kb-onboard { text-align: left; background: rgba(255,255,255,.022); border: 1px solid rgba(255,255,255,.08); border-radius: 18px; padding: 1.15rem 1.25rem 1.3rem; }
.kb-onboard-top { display: flex; align-items: baseline; justify-content: space-between; }
.kb-onboard-title { font-weight: 700; font-size: .95rem; }
.kb-onboard-count { font-family: 'JetBrains Mono', monospace; font-size: .78rem; color: #8b92ad; }
.kb-progress { height: 5px; border-radius: 999px; background: rgba(255,255,255,.07); margin: .6rem 0 .9rem; overflow: hidden; }
.kb-progress i { display: block; height: 100%; border-radius: 999px; background: linear-gradient(90deg,#8b7cff,#22d3ee); transition: width .5s cubic-bezier(.2,.7,.2,1); }
.kb-steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .3rem; }
.kb-step { display: flex; gap: .7rem; padding: .6rem .2rem; border-top: 1px solid rgba(255,255,255,.055); }
.kb-step:first-child { border-top: 0; }
.kb-check { flex-shrink: 0; width: 20px; height: 20px; margin-top: 1px; display: grid; place-items: center; border-radius: 999px; border: 1.5px solid rgba(255,255,255,.18); color: #06070d; }
.kb-step.done .kb-check { background: linear-gradient(120deg,#8b7cff,#22d3ee); border-color: transparent; }
.kb-step-body { flex: 1; min-width: 0; }
.kb-step-h { display: flex; align-items: center; gap: .6rem; }
.kb-step-h b { font-size: .94rem; font-weight: 600; color: #eef0f8; }
.kb-step.done .kb-step-h b { color: #aab0c6; }
.kb-step-link { margin-left: auto; font-size: .82rem; font-weight: 600; color: #b3a7ff; text-decoration: none; }
.kb-step-link:hover { color: #22d3ee; }
.kb-step-body p { margin: .15rem 0 0; font-size: .83rem; color: #838aa3; line-height: 1.5; }
.kb-cmd { display: inline-flex; align-items: center; gap: .6rem; margin-top: .55rem; padding: .34rem .4rem .34rem .7rem; background: rgba(139,124,255,.08); border: 1px solid rgba(139,124,255,.22); border-radius: 9px; cursor: pointer; font-family: inherit; transition: border-color .14s, background .14s; max-width: 100%; }
.kb-cmd:hover { border-color: rgba(139,124,255,.5); background: rgba(139,124,255,.13); }
.kb-cmd code { font-family: 'JetBrains Mono', monospace; font-size: .78rem; color: #cabfff; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.kb-copy { flex-shrink: 0; font-size: .68rem; font-weight: 600; color: #8b92ad; background: rgba(255,255,255,.06); border-radius: 6px; padding: .12rem .4rem; }
.kb-cmd:hover .kb-copy { color: #e8eaf2; }
.kb-links { display: flex; flex-wrap: wrap; gap: 1.2rem; justify-content: center; margin-top: 1.5rem; font-size: .85rem; }
.kb-links a { color: #8b92ad; text-decoration: none; }
.kb-links a:hover { color: #f3f4fa; }
@media (max-width: 560px) { .kb-card { padding: 2.2rem 1.4rem 1.6rem; } h1 { font-size: 2rem; } }
`;
