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
    let activeTab = 'requests';
    let statusFilter = 'all';
    let searchQuery = '';

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
    }, 1000);

    // ── Boot ──────────────────────────────
    loadInitialData();
    connect();
})();
