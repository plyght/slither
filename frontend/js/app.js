(function () {
    'use strict';

    var API_BASE = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1'
        ? 'http://REDACTED_SERVER_IP:8080'
        : '';
    var currentMode = 'hybrid';
    var currentController = null;
    var docCount = null;

    var body = document.body;
    var searchInput = document.getElementById('searchInput');
    var searchForm = document.getElementById('searchForm');
    var searchClear = document.getElementById('searchClear');
    var resultsList = document.getElementById('resultsList');
    var resultsMeta = document.getElementById('resultsMeta');
    var resultsSection = document.getElementById('resultsSection');
    var brandTitle = document.getElementById('brandTitle');
    var brandTagline = document.getElementById('brandTagline');
    var brand = document.getElementById('brand');
    var modeButtons = document.querySelectorAll('.mode-btn');
    var paletteBtn = document.getElementById('paletteBtn');

    function setState(state) {
        body.setAttribute('data-state', state);
    }

    function formatNumber(n) {
        return n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
    }

    function extractDomain(url) {
        try {
            var u = new URL(url);
            return u.hostname.replace(/^www\./, '') + (u.pathname.length > 1 ? u.pathname : '');
        } catch (e) {
            return url;
        }
    }

    function escapeHtml(str) {
        var el = document.createElement('span');
        el.textContent = str;
        return el.innerHTML;
    }

    function highlightTerms(text, query) {
        if (!query || !text) return escapeHtml(text || '');
        var words = query.split(/\s+/).filter(function (w) { return w.length > 2; });
        if (words.length === 0) return escapeHtml(text);
        var escaped = escapeHtml(text);
        words.forEach(function (word) {
            var re = new RegExp('(' + word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
            escaped = escaped.replace(re, '<mark>$1</mark>');
        });
        return escaped;
    }

    function truncateSnippet(text, maxLen) {
        if (!text) return '';
        if (text.length <= maxLen) return text;
        return text.substring(0, maxLen).replace(/\s+\S*$/, '') + '\u2026';
    }

    function renderSkeletons(count) {
        var html = '';
        for (var i = 0; i < count; i++) {
            html += '<div class="skeleton" aria-hidden="true">' +
                '<div class="skeleton-line skeleton-title"></div>' +
                '<div class="skeleton-line skeleton-url"></div>' +
                '<div class="skeleton-line skeleton-text-1"></div>' +
                '<div class="skeleton-line skeleton-text-2"></div>' +
                '</div>';
        }
        return html;
    }

    function updateClearBtn() {
        if (searchInput.value.length > 0) {
            searchClear.classList.add('visible');
        } else {
            searchClear.classList.remove('visible');
        }
    }

    function fetchStats() {
        fetch(API_BASE + '/stats')
            .then(function (r) { return r.json(); })
            .then(function (data) {
                if (data.success && data.data) {
                    docCount = data.data.documents;
                    brandTagline.innerHTML = formatNumber(docCount) + ' pages indexed &middot; hybrid search';
                }
            })
            .catch(function () {
                brandTagline.innerHTML = 'hybrid search engine';
            });
    }

    function doSearch(query, mode, pushState) {
        if (!query.trim()) return;

        if (currentController) {
            currentController.abort();
        }
        currentController = new AbortController();

        setState('results');
        searchInput.classList.add('loading');
        resultsList.innerHTML = renderSkeletons(4);
        resultsMeta.textContent = '';

        var startTime = performance.now();
        var params = '?q=' + encodeURIComponent(query.trim()) + '&limit=20&mode=' + mode;

        if (pushState) {
            var newUrl = window.location.pathname + '?q=' + encodeURIComponent(query.trim()) + '&mode=' + mode;
            history.pushState({ q: query, mode: mode }, '', newUrl);
        }

        document.title = query.trim() + ' \u2014 slither';

        fetch(API_BASE + '/search' + params, { signal: currentController.signal })
            .then(function (r) { return r.json(); })
            .then(function (data) {
                searchInput.classList.remove('loading');
                var elapsed = ((performance.now() - startTime) / 1000).toFixed(2);

                if (!data.success) {
                    resultsMeta.textContent = '';
                    resultsList.innerHTML = '<div class="error-msg"><p>' + escapeHtml(data.error || 'Search failed.') + '</p><button class="retry-btn" onclick="window.__retry()">Try again</button></div>';
                    return;
                }

                var results = data.data || [];

                if (results.length === 0) {
                    resultsMeta.innerHTML = 'No results &middot; <span class="meta-mode">' + mode + '</span> &middot; ' + elapsed + 's';
                    resultsList.innerHTML = '<div class="no-results"><span class="no-results-ornament">&oslash;</span>No results found for \u201c' + escapeHtml(query) + '\u201d</div>';
                    return;
                }

                resultsMeta.innerHTML = results.length + ' result' + (results.length !== 1 ? 's' : '') +
                    ' &middot; <span class="meta-mode">' + mode + '</span> &middot; ' + elapsed + 's';

                var html = '';
                results.forEach(function (r, i) {
                    html += '<article class="result" style="--i:' + i + '">' +
                        '<span class="result-rank">[' + (i + 1) + ']</span>' +
                        '<div class="result-body">' +
                        '<h3><a href="' + escapeHtml(r.url) + '" class="result-title">' + escapeHtml(r.title || 'Untitled') + '</a></h3>' +
                        '<cite class="result-url">' + escapeHtml(extractDomain(r.url)) + '</cite>' +
                        '<p class="result-snippet">' + highlightTerms(truncateSnippet(r.snippet, 280), query) + '</p>' +
                        '<span class="result-score">' + r.score.toFixed(4) + '</span>' +
                        '</div></article>';
                });
                resultsList.innerHTML = html;
            })
            .catch(function (err) {
                searchInput.classList.remove('loading');
                if (err.name === 'AbortError') return;
                resultsMeta.textContent = '';
                resultsList.innerHTML = '<div class="error-msg"><p>Could not reach the search index.</p><button class="retry-btn" onclick="window.__retry()">Try again</button></div>';
            });
    }

    window.__retry = function () {
        doSearch(searchInput.value, currentMode, false);
    };

    function goHome() {
        setState('home');
        searchInput.value = '';
        updateClearBtn();
        resultsList.innerHTML = '';
        resultsMeta.textContent = '';
        searchInput.focus();
        document.title = 'slither';
        history.pushState({}, '', window.location.pathname);
    }

    searchForm.addEventListener('submit', function (e) {
        e.preventDefault();
        doSearch(searchInput.value, currentMode, true);
    });

    searchInput.addEventListener('input', updateClearBtn);

    searchClear.addEventListener('click', function () {
        searchInput.value = '';
        updateClearBtn();
        searchInput.focus();
        if (body.getAttribute('data-state') === 'results') {
            goHome();
        }
    });

    modeButtons.forEach(function (btn) {
        btn.addEventListener('click', function () {
            modeButtons.forEach(function (b) { b.classList.remove('active'); });
            btn.classList.add('active');
            currentMode = btn.getAttribute('data-mode');
            if (body.getAttribute('data-state') === 'results' && searchInput.value.trim()) {
                doSearch(searchInput.value, currentMode, true);
            }
        });
    });

    brand.addEventListener('click', function () {
        if (body.getAttribute('data-state') === 'results') {
            goHome();
        }
    });

    document.addEventListener('keydown', function (e) {
        if (e.key === '/' && document.activeElement !== searchInput) {
            e.preventDefault();
            searchInput.focus();
        }
        if (e.key === 'Escape') {
            if (body.getAttribute('data-state') === 'results') {
                goHome();
            } else {
                searchInput.blur();
            }
        }
    });

    window.addEventListener('popstate', function () {
        var params = new URLSearchParams(window.location.search);
        var q = params.get('q');
        var mode = params.get('mode') || 'hybrid';
        if (q) {
            searchInput.value = q;
            updateClearBtn();
            currentMode = mode;
            modeButtons.forEach(function (b) {
                b.classList.toggle('active', b.getAttribute('data-mode') === mode);
            });
            doSearch(q, mode, false);
        } else {
            goHome();
        }
    });

    var colorGen = {
        harmonies: ['monochromatic', 'analogous', 'complementary', 'triadic', 'split-complementary'],
        currentScheme: null,

        getLuminance: function (l, c, h) {
            var a = c * Math.cos(h * Math.PI / 180);
            var b = c * Math.sin(h * Math.PI / 180);
            var L = l + 0.3963377774 * a + 0.2158037573 * b;
            var M = l - 0.1055613458 * a - 0.0638541728 * b;
            var S = l - 0.0894841775 * a - 1.2914855480 * b;
            var l_ = L * L * L, m_ = M * M * M, s_ = S * S * S;
            var r = Math.max(0, Math.min(1, +4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_));
            var g = Math.max(0, Math.min(1, -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_));
            var bv = Math.max(0, Math.min(1, -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_));
            var toLinear = function (v) { return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
            var toS = function (v) { return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055; };
            return 0.2126 * toLinear(toS(r)) + 0.7152 * toLinear(toS(g)) + 0.0722 * toLinear(toS(bv));
        },

        getContrast: function (c1, c2) {
            var l1 = this.getLuminance(c1.l, c1.c, c1.h);
            var l2 = this.getLuminance(c2.l, c2.c, c2.h);
            return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
        },

        ensureContrast: function (bg, text, min) {
            var t = { l: text.l, c: text.c, h: text.h };
            if (this.getContrast(bg, t) >= min) return t;
            var step = bg.l > 0.5 ? -0.05 : 0.05;
            for (var i = 0; i < 20; i++) {
                t.l = Math.max(0, Math.min(1, t.l + step));
                if (this.getContrast(bg, t) >= min) break;
            }
            return t;
        },

        generate: function () {
            var baseHue = Math.floor(Math.random() * 360);
            var harmony = this.harmonies[Math.floor(Math.random() * this.harmonies.length)];
            var hues;
            switch (harmony) {
                case 'monochromatic': hues = [baseHue, baseHue, baseHue]; break;
                case 'analogous': hues = [baseHue, (baseHue + 30) % 360, (baseHue + 330) % 360]; break;
                case 'complementary': hues = [baseHue, (baseHue + 180) % 360, baseHue]; break;
                case 'triadic': hues = [baseHue, (baseHue + 120) % 360, (baseHue + 240) % 360]; break;
                default: hues = [baseHue, (baseHue + 150) % 360, (baseHue + 210) % 360]; break;
            }
            var bc = 0.03 + Math.random() * 0.06;
            return {
                light: {
                    bgPrimary: { l: 0.97, c: bc * 0.3, h: hues[0] },
                    bgSecondary: { l: 0.99, c: bc * 0.2, h: hues[0] },
                    textPrimary: this.ensureContrast({ l: 0.97, c: bc * 0.3, h: hues[0] }, { l: 0.25, c: bc * 2, h: hues[1] }, 7),
                    textSecondary: this.ensureContrast({ l: 0.97, c: bc * 0.3, h: hues[0] }, { l: 0.50, c: bc * 1.5, h: hues[1] }, 4.5),
                    borderColor: { l: 0.88, c: bc * 0.6, h: hues[0] },
                    accentColor: this.ensureContrast({ l: 0.97, c: bc * 0.3, h: hues[0] }, { l: 0.30, c: bc * 2.5, h: hues[2] }, 4.5)
                },
                dark: {
                    bgPrimary: { l: 0.18, c: bc * 0.6, h: hues[0] },
                    bgSecondary: { l: 0.23, c: bc * 0.5, h: hues[0] },
                    textPrimary: this.ensureContrast({ l: 0.18, c: bc * 0.6, h: hues[0] }, { l: 0.92, c: bc * 0.8, h: hues[1] }, 7),
                    textSecondary: this.ensureContrast({ l: 0.18, c: bc * 0.6, h: hues[0] }, { l: 0.68, c: bc * 1.2, h: hues[1] }, 4.5),
                    borderColor: { l: 0.32, c: bc * 0.8, h: hues[0] },
                    accentColor: this.ensureContrast({ l: 0.18, c: bc * 0.6, h: hues[0] }, { l: 0.82, c: bc * 1.5, h: hues[2] }, 4.5)
                }
            };
        },

        apply: function (scheme) {
            var root = document.documentElement;
            var isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            var colors = isDark ? scheme.dark : scheme.light;
            var setColor = function (prop, color) {
                root.style.setProperty(prop, 'oklch(' + color.l + ' ' + color.c + ' ' + color.h + ')');
            };
            setColor('--bg-primary', colors.bgPrimary);
            setColor('--bg-secondary', colors.bgSecondary);
            setColor('--text-primary', colors.textPrimary);
            setColor('--text-secondary', colors.textSecondary);
            setColor('--border-color', colors.borderColor);
            setColor('--accent-color', colors.accentColor);
            this.currentScheme = scheme;
        },

        reset: function () {
            var root = document.documentElement;
            ['--bg-primary', '--bg-secondary', '--text-primary', '--text-secondary', '--border-color', '--accent-color'].forEach(function (p) {
                root.style.removeProperty(p);
            });
            this.currentScheme = null;
        }
    };

    paletteBtn.addEventListener('click', function () {
        colorGen.apply(colorGen.generate());
    });

    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function () {
        if (colorGen.currentScheme) {
            colorGen.apply(colorGen.currentScheme);
        }
    });

    function init() {
        fetchStats();

        var params = new URLSearchParams(window.location.search);
        var q = params.get('q');
        var mode = params.get('mode') || 'hybrid';

        if (q) {
            searchInput.value = q;
            updateClearBtn();
            currentMode = mode;
            modeButtons.forEach(function (b) {
                b.classList.toggle('active', b.getAttribute('data-mode') === mode);
            });
            doSearch(q, mode, false);
        }
    }

    init();
})();
