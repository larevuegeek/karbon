// Karbon Studio — Real-time dashboard
(() => {
    'use strict';

    const TOKEN = new URLSearchParams(window.location.search).get('token');
    const WS_URL = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/_studio/ws?token=${TOKEN}`;
    const API_URL = `/_studio/api/data?token=${TOKEN}`;

    // State
    let ws = null;
    let reconnectTimer = null;
    let requests = [];
    let events = [];
    let jobs = [];
    let mails = [];
    let stats = { total_requests: 0, total_events: 0, total_jobs: 0, total_mails: 0, avg_response_ms: 0, error_rate: 0, uptime_secs: 0 };
    let activeTab = 'overview';
    let statusFilter = 'all';
    let searchQuery = '';
    let appInfo = null;
    let dbLoaded = false;

    // DOM refs
    const $ = id => document.getElementById(id);
    const connectionStatus = $('connectionStatus');
    const statsEls = {
        requests: $('statRequests'),
        avgMs: $('statAvgMs'),
        errors: $('statErrors'),
        events: $('statEvents'),
        jobs: $('statJobs'),
        mails: $('statMails'),
    };

    // ── WebSocket ──────────────────────────────
    function connect() {
        ws = new WebSocket(WS_URL);

        ws.onopen = () => {
            setConnectionStatus('connected', 'Live');
            if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
        };

        ws.onclose = () => {
            setConnectionStatus('disconnected', 'Disconnected');
            reconnectTimer = setTimeout(connect, 2000);
        };

        ws.onerror = () => ws.close();

        ws.onmessage = (e) => {
            try {
                const msg = JSON.parse(e.data);
                handleMessage(msg);
            } catch {}
        };
    }

    function setConnectionStatus(cls, text) {
        connectionStatus.className = 'connection-status ' + cls;
        connectionStatus.querySelector('.status-text').textContent = text;
    }

    // ── Message handling ──────────────────────────────
    function handleMessage(msg) {
        switch (msg.type) {
            case 'request':
                requests.unshift(msg.data);
                if (requests.length > 500) requests.pop();
                stats.total_requests++;
                stats.total_events = stats.total_events; // keep
                renderRequests(true);
                break;
            case 'event':
                events.unshift(msg.data);
                if (events.length > 200) events.pop();
                stats.total_events++;
                renderEvents(true);
                break;
            case 'job':
                jobs.unshift(msg.data);
                if (jobs.length > 200) jobs.pop();
                stats.total_jobs++;
                renderJobs(true);
                break;
            case 'mail':
                mails.unshift(msg.data);
                if (mails.length > 100) mails.pop();
                stats.total_mails++;
                renderMails(true);
                break;
            case 'stats':
                stats = msg.data;
                break;
        }
        if (activeTab === 'overview') renderOverview();
        updateStats();
        updateTabCounts();
    }

    // ── Initial data load ──────────────────────────────
    async function loadInitialData() {
        try {
            const res = await fetch(API_URL);
            if (!res.ok) return;
            const data = await res.json();
            requests = data.requests || [];
            events = data.events || [];
            jobs = data.jobs || [];
            mails = data.mails || [];
            stats = data.stats || stats;
            renderAll();
        } catch {}
    }

    // ── Rendering ──────────────────────────────
    function renderAll() {
        renderRequests();
        renderEvents();
        renderJobs();
        renderMails();
        renderOverview();
        updateStats();
        updateTabCounts();
    }

    function updateStats() {
        statsEls.requests.textContent = formatNumber(stats.total_requests);
        statsEls.avgMs.textContent = stats.avg_response_ms + 'ms';
        statsEls.errors.textContent = stats.error_rate + '%';
        statsEls.events.textContent = formatNumber(stats.total_events);
        statsEls.jobs.textContent = formatNumber(stats.total_jobs);
        statsEls.mails.textContent = formatNumber(stats.total_mails);
        $('uptime').textContent = formatUptime(stats.uptime_secs);
    }

    function updateTabCounts() {
        $('tabRequestCount').textContent = requests.length;
        $('tabEventCount').textContent = events.length;
        $('tabJobCount').textContent = jobs.length;
        $('tabMailCount').textContent = mails.length;
    }

    function renderRequests(isNew = false) {
        const body = $('requestsBody');
        const empty = $('emptyRequests');
        const filtered = filterRequests();

        if (filtered.length === 0) {
            body.innerHTML = '';
            empty.style.display = 'flex';
            return;
        }
        empty.style.display = 'none';

        const html = filtered.map((r, i) => {
            const statusClass = statusBadgeClass(r.status);
            const methodClass = 'method-' + r.method;
            const durationClass = r.duration_ms > 500 ? 'very-slow' : r.duration_ms > 200 ? 'slow' : '';
            const newClass = isNew && i === 0 ? ' new-row' : '';

            return `<tr class="${newClass}" onclick="showRequestDetail(${r.id})">
                <td class="col-status"><span class="badge ${statusClass}">${r.status}</span></td>
                <td class="col-method"><span class="method-badge ${methodClass}">${r.method}</span></td>
                <td class="col-path" title="${esc(r.path)}">${esc(r.path)}</td>
                <td class="col-duration"><span class="duration ${durationClass}">${r.duration_ms}ms</span></td>
                <td class="col-time"><span class="time-ago">${timeAgo(r.timestamp)}</span></td>
            </tr>`;
        }).join('');

        body.innerHTML = html;
    }

    function renderEvents(isNew = false) {
        const body = $('eventsBody');
        const empty = $('emptyEvents');

        if (events.length === 0) {
            body.innerHTML = '';
            empty.style.display = 'flex';
            return;
        }
        empty.style.display = 'none';

        body.innerHTML = events.map((e, i) => {
            const newClass = isNew && i === 0 ? ' new-row' : '';
            return `<tr class="${newClass}">
                <td><span style="color:var(--accent);font-family:var(--font-mono)">${esc(e.event_type)}</span></td>
                <td><span class="badge badge-queued">${e.handler_count} handler${e.handler_count !== 1 ? 's' : ''}</span></td>
                <td><span class="time-ago">${timeAgo(e.timestamp)}</span></td>
            </tr>`;
        }).join('');
    }

    function renderJobs(isNew = false) {
        const body = $('jobsBody');
        const empty = $('emptyJobs');

        if (jobs.length === 0) {
            body.innerHTML = '';
            empty.style.display = 'flex';
            return;
        }
        empty.style.display = 'none';

        body.innerHTML = jobs.map((j, i) => {
            const newClass = isNew && i === 0 ? ' new-row' : '';
            const badge = jobStatusBadge(j.status);
            const dur = j.duration_ms != null ? j.duration_ms + 'ms' : '—';
            return `<tr class="${newClass}" ${j.error ? `title="${esc(j.error)}"` : ''}>
                <td>${badge}</td>
                <td><span style="font-family:var(--font-mono)">${esc(j.name)}</span></td>
                <td><span class="duration">${dur}</span></td>
                <td><span class="badge badge-queued">#${j.attempt}</span></td>
                <td><span class="time-ago">${timeAgo(j.timestamp)}</span></td>
            </tr>`;
        }).join('');
    }

    function renderMails(isNew = false) {
        const body = $('mailsBody');
        const empty = $('emptyMails');

        if (mails.length === 0) {
            body.innerHTML = '';
            empty.style.display = 'flex';
            return;
        }
        empty.style.display = 'none';

        body.innerHTML = mails.map((m, i) => {
            const newClass = isNew && i === 0 ? ' new-row' : '';
            const badge = m.status === 'sent'
                ? '<span class="badge badge-success">Sent</span>'
                : `<span class="badge badge-error" title="${esc(m.error || '')}">Failed</span>`;
            return `<tr class="${newClass}">
                <td>${badge}</td>
                <td><span style="font-family:var(--font-mono)">${esc(m.to.join(', '))}</span></td>
                <td>${esc(m.subject)}</td>
                <td><span class="time-ago">${timeAgo(m.timestamp)}</span></td>
            </tr>`;
        }).join('');
    }

    // ── Filters ──────────────────────────────
    function filterRequests() {
        return requests.filter(r => {
            if (statusFilter !== 'all') {
                const prefix = statusFilter.charAt(0);
                if (String(r.status).charAt(0) !== prefix) return false;
            }
            if (searchQuery) {
                const q = searchQuery.toLowerCase();
                const match = r.path.toLowerCase().includes(q)
                    || r.method.toLowerCase().includes(q)
                    || String(r.status).includes(q);
                if (!match) return false;
            }
            return true;
        });
    }

    // ── Detail slide-over ──────────────────────────────
    window.showRequestDetail = function(id) {
        const r = requests.find(x => x.id === id);
        if (!r) return;

        $('detailTitle').textContent = `${r.method} ${r.path}`;

        let html = `
            <div class="detail-section">
                <div class="detail-section-title">Overview</div>
                <div class="detail-row"><span class="detail-key">Status</span><span class="detail-value"><span class="badge ${statusBadgeClass(r.status)}">${r.status}</span></span></div>
                <div class="detail-row"><span class="detail-key">Method</span><span class="detail-value">${r.method}</span></div>
                <div class="detail-row"><span class="detail-key">Path</span><span class="detail-value">${esc(r.path)}</span></div>
                <div class="detail-row"><span class="detail-key">Duration</span><span class="detail-value">${r.duration_ms}ms</span></div>
                ${r.request_id ? `<div class="detail-row"><span class="detail-key">Request ID</span><span class="detail-value">${esc(r.request_id)}</span></div>` : ''}
                ${r.remote_addr ? `<div class="detail-row"><span class="detail-key">Remote</span><span class="detail-value">${esc(r.remote_addr)}</span></div>` : ''}
                <div class="detail-row"><span class="detail-key">Time</span><span class="detail-value">${new Date(r.timestamp).toLocaleTimeString()}</span></div>
            </div>`;

        if (r.request_headers && r.request_headers.length) {
            html += `<div class="detail-section">
                <div class="detail-section-title">Request Headers</div>
                <div class="headers-list">
                    ${r.request_headers.map(([k, v]) => `<div class="header-item"><span class="header-name">${esc(k)}</span><span class="header-value">${esc(v)}</span></div>`).join('')}
                </div>
            </div>`;
        }

        if (r.response_headers && r.response_headers.length) {
            html += `<div class="detail-section">
                <div class="detail-section-title">Response Headers</div>
                <div class="headers-list">
                    ${r.response_headers.map(([k, v]) => `<div class="header-item"><span class="header-name">${esc(k)}</span><span class="header-value">${esc(v)}</span></div>`).join('')}
                </div>
            </div>`;
        }

        $('detailBody').innerHTML = html;
        $('slideover').classList.add('open');
    };

    window.closeDetail = function() {
        $('slideover').classList.remove('open');
    };

    // ── Clear ──────────────────────────────
    window.clearData = async function() {
        try {
            await fetch(`/_studio/api/clear?token=${TOKEN}`, { method: 'POST' });
            requests = []; events = []; jobs = []; mails = [];
            stats = { total_requests: 0, total_events: 0, total_jobs: 0, total_mails: 0, avg_response_ms: 0, error_rate: 0, uptime_secs: 0 };
            renderAll();
        } catch {}
    };

    // ── Helpers ──────────────────────────────
    function statusBadgeClass(s) {
        if (s < 300) return 'badge-2xx';
        if (s < 400) return 'badge-3xx';
        if (s < 500) return 'badge-4xx';
        return 'badge-5xx';
    }

    function jobStatusBadge(s) {
        const map = {
            completed: '<span class="badge badge-success">Done</span>',
            failed: '<span class="badge badge-error">Failed</span>',
            running: '<span class="badge badge-running">Running</span>',
            queued: '<span class="badge badge-queued">Queued</span>',
        };
        return map[s] || `<span class="badge badge-queued">${s}</span>`;
    }

    function timeAgo(ts) {
        const diff = Math.floor((Date.now() - ts) / 1000);
        if (diff < 1) return 'now';
        if (diff < 60) return diff + 's ago';
        if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
        if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
        return Math.floor(diff / 86400) + 'd ago';
    }

    function formatUptime(secs) {
        if (!secs) return '--';
        const h = Math.floor(secs / 3600);
        const m = Math.floor((secs % 3600) / 60);
        const s = secs % 60;
        if (h > 0) return `${h}h ${m}m`;
        if (m > 0) return `${m}m ${s}s`;
        return `${s}s`;
    }

    function formatNumber(n) {
        if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
        if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
        return String(n);
    }

    function esc(s) {
        if (!s) return '';
        const el = document.createElement('span');
        el.textContent = s;
        return el.innerHTML;
    }

    // ── Event listeners ──────────────────────────────

    // Tabs
    document.querySelectorAll('.tab').forEach(tab => {
        tab.addEventListener('click', () => {
            document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
            document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
            tab.classList.add('active');
            activeTab = tab.dataset.tab;
            $('panel-' + activeTab).classList.add('active');
            if (activeTab === 'overview') renderOverview();
            if (activeTab === 'database') loadDatabase();
            if (activeTab === 'routes') loadRoutes();
            if (activeTab === 'terminal') $('termCmd').focus();
            if (activeTab === 'docs') loadDocs();
        });
    });

    // Status filter buttons
    document.querySelectorAll('.filter-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            statusFilter = btn.dataset.filter;
            renderRequests();
        });
    });

    // Search
    $('searchRequests').addEventListener('input', (e) => {
        searchQuery = e.target.value;
        renderRequests();
    });

    // Close detail on Escape
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') closeDetail();
    });

    // Periodic stats refresh
    setInterval(() => {
        if (stats.uptime_secs) stats.uptime_secs++;
        $('uptime').textContent = formatUptime(stats.uptime_secs);
        if (activeTab === 'overview') $('ovUptime').textContent = formatUptime(stats.uptime_secs);
    }, 1000);

    // ── App / framework info ──────────────────────────────
    async function loadAppInfo() {
        try {
            const res = await fetch(`/_studio/api/info?token=${TOKEN}`);
            if (!res.ok) return;
            const info = await res.json();
            appInfo = info;
            $('topVersion').textContent = 'v' + info.version;
            $('topEnv').textContent = info.environment;
            $('ovVersion').textContent = 'v' + info.version;
            $('ovEnv').textContent = info.environment;
            $('ovDriver').textContent = info.driver || '—';
            $('ovDb').textContent = info.database ? 'connected' : 'none';
            const features = info.features || [];
            $('ovFeatCount').textContent = features.length;
            $('ovFeatures').innerHTML = features
                .map(f => `<span class="chip">${esc(f)}</span>`).join('');
            const tables = info.tables || [];
            $('ovTablesCount').textContent = tables.length;
            $('ovTables').innerHTML = tables
                .map(t => `<span class="chip chip-entity">${esc(t)}</span>`).join('');
            $('ovTablesEmpty').style.display = tables.length ? 'none' : 'block';
        } catch {}
    }

    // ── Overview (computed client-side from requests) ──────────────────────────────
    function renderOverview() {
        $('ovTotal').textContent = formatNumber(stats.total_requests);
        $('ovErr').textContent = (stats.error_rate || 0) + '%';
        $('ovUptime').textContent = formatUptime(stats.uptime_secs);

        const durs = requests.map(r => r.duration_ms).filter(d => typeof d === 'number');
        if (durs.length) {
            const min = Math.min(...durs);
            const max = Math.max(...durs);
            const avg = Math.round(durs.reduce((a, b) => a + b, 0) / durs.length);
            $('ovLatency').textContent = `${avg}ms / ${min}ms / ${max}ms`;
        } else {
            $('ovLatency').textContent = '— / — / —';
        }

        // Status distribution
        const buckets = { '2xx': 0, '3xx': 0, '4xx': 0, '5xx': 0 };
        requests.forEach(r => {
            const k = Math.floor(r.status / 100) + 'xx';
            if (buckets[k] != null) buckets[k]++;
        });
        const total = requests.length || 1;
        const colors = { '2xx': 'var(--success)', '3xx': 'var(--info)', '4xx': 'var(--warning)', '5xx': 'var(--error)' };
        $('ovStatusDist').innerHTML = Object.keys(buckets).map(k => {
            const pct = ((buckets[k] / total) * 100).toFixed(1);
            return buckets[k]
                ? `<span class="dist-seg" style="width:${pct}%;background:${colors[k]}" title="${k}: ${buckets[k]} (${pct}%)"></span>`
                : '';
        }).join('') || '<span class="dist-empty"></span>';

        // Methods
        const methods = {};
        requests.forEach(r => { methods[r.method] = (methods[r.method] || 0) + 1; });
        $('ovMethods').innerHTML = Object.keys(methods).length
            ? Object.entries(methods).sort((a, b) => b[1] - a[1])
                .map(([m, c]) => `<span class="chip"><b class="method-${m}">${m}</b> ${c}</span>`).join('')
            : '<span class="empty-inline">No traffic yet.</span>';

        // Slowest endpoints
        const slow = [...requests].sort((a, b) => b.duration_ms - a.duration_ms).slice(0, 6);
        const body = $('ovSlowBody');
        const empty = $('ovSlowEmpty');
        if (!slow.length) {
            body.innerHTML = '';
            empty.style.display = 'block';
        } else {
            empty.style.display = 'none';
            body.innerHTML = slow.map(r => {
                const durationClass = r.duration_ms > 500 ? 'very-slow' : r.duration_ms > 200 ? 'slow' : '';
                return `<tr onclick="showRequestDetail(${r.id})">
                    <td class="col-method"><span class="method-badge method-${r.method}">${r.method}</span></td>
                    <td class="col-path" title="${esc(r.path)}">${esc(r.path)}</td>
                    <td class="col-status"><span class="badge ${statusBadgeClass(r.status)}">${r.status}</span></td>
                    <td class="col-duration"><span class="duration ${durationClass}">${r.duration_ms}ms</span></td>
                </tr>`;
            }).join('');
        }
    }

    // ── Database schema browser ──────────────────────────────
    window.loadDatabase = async function(force) {
        if (dbLoaded && !force) return;
        try {
            const res = await fetch(`/_studio/api/database?token=${TOKEN}`);
            if (!res.ok) return;
            const data = await res.json();
            dbLoaded = true;
            $('dbDriver').textContent = data.driver || '—';
            const tables = data.tables || [];

            if (!data.connected || !tables.length) {
                $('dbLayout').style.display = 'none';
                $('dbEmpty').style.display = 'flex';
                return;
            }
            $('dbLayout').style.display = 'flex';
            $('dbEmpty').style.display = 'none';

            window.__dbTables = tables;
            $('dbTables').innerHTML = tables.map((t, i) => `
                <button class="db-table-item${i === 0 ? ' active' : ''}" onclick="showTable(${i})">
                    <span class="db-table-name">${esc(t.name)}</span>
                    <span class="db-table-rows">${t.rows < 0 ? '?' : formatNumber(t.rows)}</span>
                </button>`).join('');
            showTable(0);
        } catch {}
    };

    window.showTable = function(idx) {
        const tables = window.__dbTables || [];
        const t = tables[idx];
        if (!t) return;
        document.querySelectorAll('.db-table-item').forEach((el, i) => el.classList.toggle('active', i === idx));
        const cols = t.columns || [];
        $('dbDetail').innerHTML = `
            <div class="db-detail-head">
                <h3>${esc(t.name)}</h3>
                <span class="db-detail-meta">${t.rows < 0 ? '—' : formatNumber(t.rows)} rows · ${cols.length} columns</span>
            </div>
            <div class="table-wrapper">
                <table class="data-table">
                    <thead><tr><th>Column</th><th>Type</th></tr></thead>
                    <tbody>${cols.map(c => `<tr><td><span style="font-family:var(--font-mono)">${esc(c.name)}</span></td><td><span style="color:var(--text-secondary);font-family:var(--font-mono)">${esc(c.ty)}</span></td></tr>`).join('')}</tbody>
                </table>
            </div>`;
    };

    // ── Terminal ──────────────────────────────
    async function runTerminal(cmd) {
        cmd = (cmd || '').trim();
        if (!cmd) return;
        // Destructive commands ask for confirmation first.
        if (/\brollback\b/.test(cmd) && !confirm(`Run a destructive command?\n\n  karbon ${cmd}\n\nThis rolls back the last migration.`)) {
            return;
        }
        const out = $('termOut');
        const block = document.createElement('div');
        block.className = 'term-block';
        block.innerHTML = `<div class="term-cmd"><span class="term-prompt">karbon</span> ${esc(cmd)} <span class="term-spin">running…</span></div>`;
        out.appendChild(block);
        out.scrollTop = out.scrollHeight;

        try {
            const res = await fetch(`/_studio/api/terminal?token=${TOKEN}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ command: cmd }),
            });
            const data = await res.json();
            const ok = data.code === 0;
            let body = '';
            if (data.stdout) body += `<pre class="term-stdout">${esc(data.stdout.trimEnd())}</pre>`;
            if (data.stderr) body += `<pre class="term-stderr">${esc(data.stderr.trimEnd())}</pre>`;
            block.innerHTML = `<div class="term-cmd"><span class="term-prompt">karbon</span> ${esc(cmd)} <span class="term-code ${ok ? 'ok' : 'err'}">exit ${data.code == null ? '—' : data.code}</span></div>${body}`;
        } catch (e) {
            block.innerHTML += `<pre class="term-stderr">request failed: ${esc(String(e))}</pre>`;
        }
        out.scrollTop = out.scrollHeight;
    }

    // Every command runnable from the Studio terminal (whitelisted, dev-only).
    const COMMANDS = [
        { cmd: 'doctor', desc: 'Diagnose common project issues (offline)' },
        { cmd: 'docs build', desc: 'Render docs/*.md → docs/_site/ static site' },
        { cmd: 'migrate', desc: 'Apply pending migrations' },
        { cmd: 'migrate diff', desc: 'Generate a migration from the entity ↔ DB diff' },
        { cmd: 'migrate status', desc: 'Show applied / pending migrations' },
        { cmd: 'migrate rollback', desc: 'Roll back the last migration' },
        { cmd: 'generate entity <Name>', desc: 'Entity + migration', tmpl: 'generate entity ' },
        { cmd: 'generate crud <Name>', desc: 'Entity + repository + controller + migration', tmpl: 'generate crud ' },
        { cmd: 'generate controller <Name>', desc: 'API controller stub', tmpl: 'generate controller ' },
        { cmd: 'generate admin <Name>', desc: 'Admin CRUD UI (run after crud)', tmpl: 'generate admin ' },
    ];

    function renderCatalog() {
        $('termCatalog').innerHTML =
            '<div class="term-cat-title">Available commands <span>· click to use · type for autocomplete</span></div>' +
            COMMANDS.map(c =>
                `<button class="term-cat-row" data-fill="${esc(c.tmpl || c.cmd)}" data-run="${c.tmpl ? '' : '1'}">
                    <span class="term-cat-cmd"><span class="term-prompt">karbon</span> ${esc(c.cmd)}</span>
                    <span class="term-cat-desc">${esc(c.desc)}</span>
                </button>`).join('');
        $('termCommands').innerHTML = COMMANDS
            .map(c => `<option value="${esc(c.tmpl ? c.tmpl.trim() + ' Post' : c.cmd)}"></option>`).join('');
        document.querySelectorAll('.term-cat-row').forEach(row => {
            row.addEventListener('click', () => {
                const fill = row.dataset.fill;
                if (row.dataset.run) {
                    runTerminal(fill);
                } else {
                    const input = $('termCmd');
                    input.value = fill;
                    input.focus();
                }
            });
        });
    }

    function initTerminal() {
        renderCatalog();
        $('termForm').addEventListener('submit', (e) => {
            e.preventDefault();
            const input = $('termCmd');
            runTerminal(input.value);
            input.value = '';
        });
        document.querySelectorAll('.term-chip').forEach(chip => {
            chip.addEventListener('click', () => runTerminal(chip.dataset.cmd));
        });
        const FIELD_TYPES = ['string', 'text', 'int', 'bigint', 'float', 'bool', 'datetime', 'date', 'json'];
        function addMakerField(name = '', type = 'string', nullable = false, focus = true) {
            const row = document.createElement('div');
            row.className = 'maker-frow';
            row.innerHTML = `
                <input class="maker-fname" placeholder="field name" spellcheck="false" autocapitalize="off" value="${esc(name)}">
                <select class="maker-ftype">${FIELD_TYPES.map(t => `<option ${t === type ? 'selected' : ''}>${t}</option>`).join('')}</select>
                <label class="maker-opt"><input type="checkbox" class="maker-fnull" ${nullable ? 'checked' : ''}> nullable</label>
                <button type="button" class="maker-frm" title="remove">×</button>`;
            row.querySelector('.maker-frm').addEventListener('click', () => row.remove());
            $('makerFields').appendChild(row);
            if (focus) row.querySelector('.maker-fname').focus();
        }
        function fieldsSupported() { return ['entity', 'crud'].includes($('makerKind').value); }
        function syncMakerFields() {
            const on = fieldsSupported();
            $('makerFields').style.display = on ? 'flex' : 'none';
            $('makerAddField').style.display = on ? '' : 'none';
            // Show at least one field row so the builder is immediately visible.
            if (on && $('makerFields').children.length === 0) addMakerField('', 'string', false, false);
        }
        $('makerAddField').addEventListener('click', () => addMakerField());
        $('makerKind').addEventListener('change', syncMakerFields);
        syncMakerFields();

        $('makerForm').addEventListener('submit', (e) => {
            e.preventDefault();
            const kind = $('makerKind').value;
            const name = $('makerName').value.trim();
            if (!name) { $('makerName').focus(); return; }
            let cmd = `generate ${kind} ${name}`;
            if (fieldsSupported()) {
                document.querySelectorAll('#makerFields .maker-frow').forEach(row => {
                    const fname = row.querySelector('.maker-fname').value.trim();
                    if (!fname) return;
                    const ftype = row.querySelector('.maker-ftype').value;
                    const fnull = row.querySelector('.maker-fnull').checked ? '?' : '';
                    cmd += ` ${fname}:${ftype}${fnull}`;
                });
            }
            if ($('makerDry').checked) cmd += ' --dry-run';
            if ($('makerForce').checked) cmd += ' --force';
            runTerminal(cmd);
        });
    }

    // ── Docs ──────────────────────────────
    let docsLoaded = false;
    window.loadDocs = async function(force) {
        if (docsLoaded && !force) return;
        try {
            const res = await fetch(`/_studio/api/docs?token=${TOKEN}`);
            if (!res.ok) return;
            const data = await res.json();
            docsLoaded = true;
            const docs = data.docs || [];
            window.__docs = docs;
            $('docsNav').innerHTML = docs.map((d, i) => `
                <button class="docs-link${i === 0 ? ' active' : ''}" onclick="showDoc(${i})">
                    ${esc(d.title)}${d.source === 'framework' ? '<span class="docs-badge">framework</span>' : ''}
                </button>`).join('');
            showDoc(0);
        } catch {}
    };
    window.showDoc = function(idx) {
        const docs = window.__docs || [];
        const d = docs[idx];
        if (!d) return;
        document.querySelectorAll('.docs-link').forEach((el, i) => el.classList.toggle('active', i === idx));
        $('docsContent').innerHTML = d.html;
        $('docsContent').scrollTop = 0;
    };

    // ── Routes ──────────────────────────────
    let routesLoaded = false;
    let allRoutes = [];
    let routeKind = 'all';

    // Classify by what the route serves: JSON API, framework/Axum built-ins, or
    // server-rendered HTML (admin pages, public pages…).
    function classifyRoute(path) {
        if (/^\/(api)(\/|$)/.test(path)) return 'api';
        if (/^\/(_studio|_hmr|docs|openapi\.json|health|files)(\/|$)/.test(path)) return 'system';
        return 'web';
    }

    window.loadRoutes = async function(force) {
        if (routesLoaded && !force) return;
        const builtin = [
            { method: 'GET', path: '/health', tag: 'health' },
            { method: 'GET', path: '/docs', tag: 'swagger-ui' },
            { method: 'GET', path: '/openapi.json', tag: 'openapi' },
            { method: 'GET', path: '/_studio', tag: 'studio' },
        ];
        const ops = [];
        try {
            const res = await fetch('/openapi.json', { headers: { accept: 'application/json' } });
            if (res.ok) {
                const spec = await res.json();
                for (const [path, methods] of Object.entries(spec.paths || {})) {
                    for (const [method, op] of Object.entries(methods)) {
                        const params = (op.parameters || []).map(p => {
                            const s = p.schema || {};
                            const c = s.minimum != null ? `≥${s.minimum}` : (s.minLength != null ? `≥${s.minLength} chars` : '');
                            return { name: p.name, type: s.type || 'string', c };
                        });
                        ops.push({ method: method.toUpperCase(), path, tag: (op.tags && op.tags[0]) || '', params });
                    }
                }
            }
        } catch {}
        routesLoaded = true;
        allRoutes = ops.concat(builtin)
            .map(r => ({ ...r, kind: classifyRoute(r.path) }))
            .sort((a, b) => (a.kind + a.path).localeCompare(b.kind + b.path));
        renderRoutes();
    };

    function renderRoutes() {
        const q = ($('searchRoutes').value || '').toLowerCase();
        const rows = allRoutes.filter(r =>
            (routeKind === 'all' || r.kind === routeKind) &&
            (!q || r.path.toLowerCase().includes(q) || r.method.toLowerCase().includes(q) || (r.tag || '').toLowerCase().includes(q)));
        const body = $('routesBody');
        if (!rows.length) {
            body.innerHTML = '';
            $('routesEmpty').style.display = 'flex';
            return;
        }
        $('routesEmpty').style.display = 'none';
        body.innerHTML = rows.map(r => {
            const params = (r.params || []).map(p =>
                `<span class="route-param" title="path parameter, validated as ${esc(p.type)}">${esc(p.name)}:${esc(p.type)}${p.c ? ' ' + esc(p.c) : ''}</span>`).join(' ');
            return `<tr>
            <td class="col-status"><span class="route-kind kind-${r.kind}">${r.kind}</span></td>
            <td class="col-method"><span class="method-badge method-${r.method}">${r.method}</span></td>
            <td class="col-path" title="${esc(r.path)}">${esc(r.path)} ${params}</td>
            <td>${r.tag ? `<span class="chip">${esc(r.tag)}</span>` : ''}</td>
        </tr>`;
        }).join('');
    }

    $('searchRoutes').addEventListener('input', renderRoutes);
    document.querySelectorAll('#routeKindFilter .filter-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('#routeKindFilter .filter-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            routeKind = btn.dataset.kind;
            renderRoutes();
        });
    });

    // ── Boot ──────────────────────────────
    loadInitialData();
    loadAppInfo();
    initTerminal();
    connect();

    // Deep-link: /_studio#terminal opens the terminal tab (used by the toolbar).
    if (location.hash === '#terminal') {
        const tab = document.querySelector('.tab[data-tab="terminal"]');
        if (tab) tab.click();
    }
})();
