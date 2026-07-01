/* Karbon docs shell — builds chrome (nav, sidebar, TOC, pager) around each page's <article id="doc">. */
(function () {
  var LOGO = '<svg width="24" height="24" viewBox="0 0 32 32" fill="none"><defs><linearGradient id="kg" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse"><stop stop-color="#8b7cff"/><stop offset="1" stop-color="#22d3ee"/></linearGradient></defs><path d="M16 1.6 28.5 8.8v14.4L16 30.4 3.5 23.2V8.8z" fill="url(#kg)"/><path d="M12.5 10v12M12.5 16l6.2-6M12.5 16l6.2 6" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>';

  // Single source of truth for the docs nav + pager order.
  var NAV = [
    { group: 'Get started', pages: [
      ['index.html', 'Introduction'],
      ['installation.html', 'Installation'],
      ['quickstart.html', 'Quickstart'],
      ['structure.html', 'Project structure'],
    ]},
    { group: 'Core concepts', pages: [
      ['cli.html', 'CLI reference'],
      ['routing.html', 'Routing & controllers'],
      ['database.html', 'Database & ORM'],
      ['migrations.html', 'Migrations'],
      ['validation.html', 'Validation'],
    ]},
    { group: 'Build', pages: [
      ['generators.html', 'Generators & scaffolding'],
      ['security.html', 'Security & auth'],
      ['realtime.html', 'Realtime & background'],
      ['frontend.html', 'Frontend & rendering'],
    ]},
    { group: 'Operate', pages: [
      ['studio.html', 'Studio cockpit'],
      ['configuration.html', 'Configuration'],
      ['deployment.html', 'Deployment'],
    ]},
  ];

  var GH = 'https://github.com/larevuegeek/karbon';
  var file = (location.pathname.split('/').pop() || 'index.html');
  var flat = NAV.reduce(function (a, g) { return a.concat(g.pages); }, []);

  function slug(s) { return s.toLowerCase().replace(/[^\w]+/g, '-').replace(/^-|-$/g, ''); }

  document.addEventListener('DOMContentLoaded', function () {
    var doc = document.getElementById('doc');
    var title = doc ? doc.getAttribute('data-title') : 'Docs';
    var cat = doc ? (doc.getAttribute('data-cat') || 'Documentation') : '';

    // ---- top nav ----
    var nav = el('header', 'nav');
    nav.innerHTML =
      '<div class="nav-in">' +
        '<button class="menu-btn" aria-label="Menu">☰</button>' +
        '<a class="brand" href="../index.html">' + LOGO + '<span>Karbon</span></a>' +
        '<nav class="nav-links">' +
          '<a href="index.html" class="active">Docs</a>' +
          '<a href="../index.html#features">Features</a>' +
          '<a href="cli.html">CLI</a>' +
          '<a class="btn btn-ghost" href="' + GH + '">GitHub ★</a>' +
        '</nav>' +
      '</div>';
    document.body.insertBefore(nav, document.body.firstChild);

    // ---- sidebar ----
    var sideHtml = '<input class="search-box" type="search" placeholder="Search the docs…" aria-label="Search">';
    NAV.forEach(function (g) {
      sideHtml += '<div class="nav-group"><h5>' + g.group + '</h5>';
      g.pages.forEach(function (p) {
        var act = p[0] === file ? ' active' : '';
        sideHtml += '<a class="navlink' + act + '" href="' + p[0] + '">' + p[1] + '</a>';
      });
      sideHtml += '</div>';
    });
    var side = el('aside', 'sidebar');
    side.innerHTML = sideHtml;

    // ---- build TOC from headings ----
    var heads = doc ? doc.querySelectorAll('h2, h3') : [];
    var tocHtml = '<h6>On this page</h6>';
    heads.forEach(function (h) {
      if (!h.id) h.id = slug(h.textContent);
      var lvl = h.tagName === 'H3' ? ' lvl3' : '';
      tocHtml += '<a class="tlink' + lvl + '" href="#' + h.id + '">' + h.textContent + '</a>';
    });
    var toc = el('nav', 'toc');
    toc.innerHTML = tocHtml;

    // ---- pager ----
    var idx = flat.findIndex(function (p) { return p[0] === file; });
    var prev = idx > 0 ? flat[idx - 1] : null;
    var next = idx >= 0 && idx < flat.length - 1 ? flat[idx + 1] : null;
    var pager = el('div', 'pager');
    pager.innerHTML =
      (prev ? '<a href="' + prev[0] + '"><small>← Previous</small><b>' + prev[1] + '</b></a>' : '<span></span>') +
      (next ? '<a class="next" href="' + next[0] + '"><small>Next →</small><b>' + next[1] + '</b></a>' : '<span></span>');

    // ---- crumb ----
    var crumb = el('div', 'crumb');
    crumb.innerHTML = '<a href="index.html">Docs</a> / ' + cat;

    // ---- assemble ----
    var main = el('main', 'doc-main');
    var content = el('div', 'doc-content');
    if (doc) {
      doc.parentNode.removeChild(doc);
      content.appendChild(crumb);
      while (doc.firstChild) content.appendChild(doc.firstChild);
      content.appendChild(pager);
    }
    main.appendChild(content);

    var scrim = el('div', 'scrim');
    var shell = el('div', 'docs-shell');
    shell.appendChild(side);
    shell.appendChild(main);
    shell.appendChild(toc);
    document.body.appendChild(shell);
    document.body.appendChild(buildFooter());
    document.body.appendChild(scrim);
    document.title = title + ' — Karbon docs';

    // ---- mobile menu ----
    var mb = nav.querySelector('.menu-btn');
    mb.addEventListener('click', function () { document.body.classList.toggle('nav-open'); });
    scrim.addEventListener('click', function () { document.body.classList.remove('nav-open'); });
    side.addEventListener('click', function (e) { if (e.target.tagName === 'A') document.body.classList.remove('nav-open'); });

    // ---- search filter ----
    var sb = side.querySelector('.search-box');
    sb.addEventListener('input', function () {
      var q = sb.value.trim().toLowerCase();
      side.querySelectorAll('.nav-group').forEach(function (grp) {
        var any = false;
        grp.querySelectorAll('.navlink').forEach(function (a) {
          var hit = !q || a.textContent.toLowerCase().indexOf(q) > -1;
          a.classList.toggle('nav-hidden', !hit);
          if (hit) any = true;
        });
        grp.classList.toggle('nav-hidden', !any);
      });
    });

    // ---- scrollspy ----
    var tlinks = toc.querySelectorAll('.tlink');
    if (heads.length && tlinks.length) {
      var obs = new IntersectionObserver(function (entries) {
        entries.forEach(function (en) {
          if (en.isIntersecting) {
            tlinks.forEach(function (t) { t.classList.toggle('active', t.getAttribute('href') === '#' + en.target.id); });
          }
        });
      }, { rootMargin: '-70px 0px -70% 0px' });
      heads.forEach(function (h) { obs.observe(h); });
    }
  });

  function el(tag, cls) { var e = document.createElement(tag); if (cls) e.className = cls; return e; }

  function buildFooter() {
    var f = el('footer');
    f.innerHTML =
      '<div class="wrap"><div class="foot-grid">' +
        '<div><a class="brand" href="../index.html">' + LOGO + '<span>Karbon</span></a>' +
        '<p style="color:var(--mut);max-width:260px;margin-top:12px">The batteries-included Rust full-stack framework. Axum + SvelteKit/Next.js, one CLI.</p></div>' +
        '<div><h4>Docs</h4><a href="installation.html">Installation</a><a href="quickstart.html">Quickstart</a><a href="cli.html">CLI reference</a><a href="deployment.html">Deployment</a></div>' +
        '<div><h4>Concepts</h4><a href="routing.html">Routing</a><a href="database.html">Database</a><a href="migrations.html">Migrations</a><a href="security.html">Security</a></div>' +
        '<div><h4>Project</h4><a href="' + GH + '">GitHub</a><a href="https://crates.io/crates/karbon-framework">crates.io</a><a href="https://docs.rs/karbon-framework">docs.rs</a><a href="../index.html">Home</a></div>' +
      '</div><div class="foot-bottom"><span>© Karbon · AGPL-3.0</span></div></div>';
    return f;
  }
})();
