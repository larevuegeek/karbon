<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{{PROJECT_NAME_TITLE}} — Karbon</title>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet" />
  <style>
    * { box-sizing: border-box; }
    :root { --vio:#8b7cff; --azu:#22d3ee; }
    body {
      margin: 0; min-height: 100vh; display: flex; flex-direction: column;
      align-items: center; justify-content: center; padding: 2rem 1rem 4rem; color: #f3f4fa;
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      letter-spacing: -.011em; position: relative;
      background:
        radial-gradient(640px 520px at 10% -8%, rgba(139,124,255,.2), transparent 60%),
        radial-gradient(720px 560px at 100% -4%, rgba(34,211,238,.14), transparent 58%),
        linear-gradient(180deg, #080a13, #06070d);
    }
    body::before {
      content:''; position:fixed; inset:0; z-index:0; pointer-events:none; opacity:.5;
      background-image:
        linear-gradient(rgba(255,255,255,.02) 1px, transparent 1px),
        linear-gradient(90deg, rgba(255,255,255,.02) 1px, transparent 1px);
      background-size:60px 60px;
      -webkit-mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%);
              mask-image: radial-gradient(circle at 50% 8%, #000, transparent 78%);
    }
    .card {
      position: relative; z-index: 1; width: 100%; max-width: 680px;
      background: rgba(255,255,255,.03); backdrop-filter: blur(18px);
      border: 1px solid rgba(255,255,255,.09); border-radius: 24px;
      padding: 2.75rem 2.5rem 2.25rem; text-align: center;
      box-shadow: 0 1px 0 rgba(255,255,255,.05) inset, 0 40px 90px -35px rgba(0,0,0,.85);
      animation: rise .55s cubic-bezier(.2,.7,.2,1) both;
    }
    @keyframes rise { from { opacity:0; transform:translateY(14px);} to { opacity:1; transform:none;} }
    .logo { filter: drop-shadow(0 12px 30px rgba(124,92,255,.6)); animation: float 4s ease-in-out infinite; }
    @keyframes float { 50% { transform: translateY(-5px);} }
    .badge {
      display: inline-flex; align-items: center; gap: .45rem; margin: .9rem 0 .25rem;
      padding: .32rem .85rem; font-size: .76rem; font-weight: 600; color: #6ee7b7;
      background: rgba(52,211,153,.1); border: 1px solid rgba(52,211,153,.22); border-radius: 999px;
    }
    .dot { width: 7px; height: 7px; border-radius: 50%; background: #34d399;
      box-shadow: 0 0 0 3px rgba(52,211,153,.2); animation: pulse 1.8s ease-in-out infinite; }
    @keyframes pulse { 50% { box-shadow: 0 0 0 6px rgba(52,211,153,0);} }
    h1 { margin: 1.1rem 0 .5rem; font-size: 2.3rem; font-weight: 800; letter-spacing: -.032em; color: #f6f7fc; }
    h1 span { background: linear-gradient(120deg,var(--vio),var(--azu)); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; }
    .lead { margin: 0 auto 1.9rem; max-width: 46ch; color: #aab0c6; line-height: 1.6; }
    .onboard { text-align: left; background: rgba(255,255,255,.022); border: 1px solid rgba(255,255,255,.08); border-radius: 18px; padding: 1.15rem 1.25rem 1.3rem; }
    .onboard-top { display: flex; align-items: baseline; justify-content: space-between; }
    .onboard-title { font-weight: 700; font-size: .95rem; }
    .onboard-count { font-family: 'JetBrains Mono', monospace; font-size: .78rem; color: #8b92ad; }
    .progress { height: 5px; border-radius: 999px; background: rgba(255,255,255,.07); margin: .6rem 0 .9rem; overflow: hidden; }
    .progress i { display: block; height: 100%; border-radius: 999px; background: linear-gradient(90deg,var(--vio),var(--azu)); transition: width .5s cubic-bezier(.2,.7,.2,1); }
    .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .3rem; }
    .step { display: flex; gap: .7rem; padding: .6rem .2rem; border-top: 1px solid rgba(255,255,255,.055); }
    .step:first-child { border-top: 0; }
    .check { flex-shrink: 0; width: 20px; height: 20px; margin-top: 1px; display: grid; place-items: center; border-radius: 999px; border: 1.5px solid rgba(255,255,255,.18); color: #06070d; }
    .step.done .check { background: linear-gradient(120deg,var(--vio),var(--azu)); border-color: transparent; }
    .step.done .check svg { display: block; }
    .check svg { display: none; }
    .step-h { display: flex; align-items: center; gap: .6rem; }
    .step-h b { font-size: .94rem; font-weight: 600; color: #eef0f8; }
    .step.done .step-h b { color: #aab0c6; }
    .step-link { margin-left: auto; font-size: .82rem; font-weight: 600; color: #b3a7ff; text-decoration: none; }
    .step-link:hover { color: var(--azu); }
    .step p { margin: .15rem 0 0; font-size: .83rem; color: #838aa3; line-height: 1.5; }
    .cmd { display: inline-flex; align-items: center; gap: .6rem; margin-top: .55rem; padding: .34rem .4rem .34rem .7rem;
      background: rgba(139,124,255,.08); border: 1px solid rgba(139,124,255,.22); border-radius: 9px; cursor: pointer;
      font-family: inherit; transition: border-color .14s, background .14s; max-width: 100%; }
    .cmd:hover { border-color: rgba(139,124,255,.5); background: rgba(139,124,255,.13); }
    .cmd code { font-family: 'JetBrains Mono', monospace; font-size: .78rem; color: #cabfff; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .copy { flex-shrink: 0; font-size: .68rem; font-weight: 600; color: #8b92ad; background: rgba(255,255,255,.06); border-radius: 6px; padding: .12rem .4rem; }
    .cmd:hover .copy { color: #e8eaf2; }
    .links { display: flex; flex-wrap: wrap; gap: 1.2rem; justify-content: center; margin-top: 1.5rem; font-size: .85rem; }
    .links a { color: #8b92ad; text-decoration: none; }
    .links a:hover { color: #f3f4fa; }
    footer { position: relative; z-index: 1; margin-top: 1.4rem; color: #5b6180; font-size: .8rem; font-family: 'JetBrains Mono', monospace; }
    footer code { color: #b3a7ff; }
    @media (max-width: 560px) { .card { padding: 2.2rem 1.4rem 1.6rem; } h1 { font-size: 2rem; } }
  </style>
</head>
<body>
  <main class="card">
    <div class="logo">
      <svg width="46" height="46" viewBox="0 0 32 32" fill="none" aria-hidden="true">
        <defs><linearGradient id="kbHex" x1="2" y1="2" x2="30" y2="30" gradientUnits="userSpaceOnUse"><stop stop-color="#8b7cff"/><stop offset="1" stop-color="#22d3ee"/></linearGradient></defs>
        <path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="url(#kbHex)"/>
        <path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="none" stroke="#fff" stroke-opacity=".18" stroke-width="1"/>
        <path d="M12.5 10v12M12.5 16l6.2-6M12.5 16l6.2 6" stroke="#fff" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </div>
    <span class="badge"><span class="dot"></span> running · backend-only</span>
    <h1>Welcome to <span>{{PROJECT_NAME_TITLE}}</span></h1>
    <p class="lead">
      Your Karbon <strong>micro</strong> backend is up — the Rust binary serves everything
      directly. Follow the steps below to build your API.
    </p>

    <div class="onboard">
      <div class="onboard-top">
        <span class="onboard-title">Getting started</span>
        <span class="onboard-count" id="count">1 / 4 done</span>
      </div>
      <div class="progress"><i id="bar" style="width:25%"></i></div>
      <ol class="steps" id="steps">
        <li class="step done" data-key="run">
          <span class="check"><svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
          <div><div class="step-h"><b>Backend running</b></div><p>The Rust binary is live and serving this page.</p></div>
        </li>
        <li class="step" data-key="studio">
          <span class="check"><svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
          <div><div class="step-h"><b>Open Studio</b><a class="step-link" href="/_studio" target="_blank" rel="noreferrer">Open →</a></div><p>Live dev cockpit: metrics, schema, routes &amp; an integrated terminal.</p></div>
        </li>
        <li class="step" data-key="db">
          <span class="check"><svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
          <div><div class="step-h"><b>Connect a database</b></div><p>Set <code>DB_NAME</code> in <code>.env</code>, then run the migrations.</p>
            <button class="cmd" data-cmd="karbon migrate"><code>karbon migrate</code><span class="copy">Copy</span></button></div>
        </li>
        <li class="step" data-key="crud">
          <span class="check"><svg width="13" height="13" viewBox="0 0 16 16" fill="none"><path d="M13 4.5 6.5 11 3 7.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
          <div><div class="step-h"><b>Scaffold your first resource</b></div><p>Entity, repository &amp; controller in one command.</p>
            <button class="cmd" data-cmd="karbon generate crud Post title:string body:text"><code>karbon generate crud Post</code><span class="copy">Copy</span></button></div>
        </li>
      </ol>
    </div>

    <div class="links">
      <a href="/_studio" target="_blank" rel="noreferrer">Studio</a>
      <a href="/docs" target="_blank" rel="noreferrer">API docs</a>
      <a href="/health" target="_blank" rel="noreferrer">/health</a>
    </div>
  </main>
  <footer>Edit <code>app/src/welcome.html</code> to change this page.</footer>

  <script>
    // Copy-to-clipboard on command chips.
    document.querySelectorAll('.cmd').forEach(function (b) {
      b.addEventListener('click', function () {
        navigator.clipboard.writeText(b.dataset.cmd).then(function () {
          var s = b.querySelector('.copy'), t = s.textContent;
          s.textContent = 'Copied ✓';
          setTimeout(function () { s.textContent = t; }, 1400);
        });
      });
    });
    // Auto-check steps from the live app state (if Studio is available).
    fetch('/_studio/api/info', { headers: { accept: 'application/json' } })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (info) {
        if (!info) return;
        var done = { run: true, studio: true, db: !!info.database, crud: !!(info.tables && info.tables.length) };
        var n = 0;
        document.querySelectorAll('.step').forEach(function (li) {
          if (done[li.dataset.key]) { li.classList.add('done'); n++; } else { li.classList.remove('done'); }
        });
        document.getElementById('count').textContent = n + ' / 4 done';
        document.getElementById('bar').style.width = (n / 4 * 100) + '%';
      })
      .catch(function () {});
  </script>
</body>
</html>
