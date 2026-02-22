(function () {
    'use strict';

    var API_BASE = (function() {
        if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
            var backendPort = new URLSearchParams(window.location.search).get('api_port') || '8080';
            return 'http://' + window.location.hostname + ':' + backendPort;
        }
        return '';
    })();
    var currentMode = 'hybrid';
    var currentOffset = 0;
    var currentQuery = '';
    var limit = 20;
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
    var autocomplete = document.getElementById('autocomplete');
    var autocompleteList = document.getElementById('autocompleteList');
    var acActiveIndex = -1;
    var acResults = [];
    var acDebounceTimer = null;
    var acController = null;
    var acLastQuery = '';

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

    function extractHostname(url) {
        try {
            return new URL(url).hostname.replace(/^www\./, '');
        } catch (e) {
            return '';
        }
    }

    function formatUrl(url) {
        try {
            var u = new URL(url);
            var host = u.hostname.replace(/^www\./, '');
            var path = u.pathname;
            if (path === '/' || path === '') return host;
            var segments = path.replace(/\/$/, '').split('/').filter(Boolean);
            if (segments.length === 0) return host;
            return host + ' \u203A ' + segments.join(' \u203A ');
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

    function renderSkeletons(groupCount) {
        var html = '';
        var resultsPerGroup = [2, 1, 1];
        for (var g = 0; g < groupCount; g++) {
            var count = resultsPerGroup[g] || 1;
            html += '<div class="skeleton-group" style="--i:' + g + '" aria-hidden="true">';
            html += '<div class="skeleton-group-header">';
            html += '<div class="skeleton-line skeleton-favicon"></div>';
            html += '<div class="skeleton-line skeleton-domain"></div>';
            html += '</div>';
            for (var i = 0; i < count; i++) {
                html += '<div class="skeleton">' +
                    '<div class="skeleton-line skeleton-url"></div>' +
                    '<div class="skeleton-line skeleton-title"></div>' +
                    '<div class="skeleton-line skeleton-text-1"></div>' +
                    '<div class="skeleton-line skeleton-text-2"></div>' +
                    '</div>';
            }
            html += '</div>';
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

    var statsInterval = null;

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

    function startStatsPolling() {
        if (statsInterval) return;
        statsInterval = setInterval(fetchStats, 30000);
    }

    function buildResultHtml(r) {
        var scorePercent = typeof r.score === 'number' ? Math.min(100, Math.round(r.score * 100)) : null;
        var externalIcon = '<svg class="external-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3H3v10h10v-3"/><path d="M9 2h5v5"/><path d="M14 2L7 9"/></svg>';
        return '<article class="result">' +
            '<cite class="result-url">' + escapeHtml(formatUrl(r.url)) + '</cite>' +
            '<h3><a href="' + escapeHtml(r.url) + '" class="result-title" target="_blank" rel="noopener">' + escapeHtml(r.title || 'Untitled') + externalIcon + '</a></h3>' +
            '<p class="result-snippet">' + highlightTerms(truncateSnippet(r.snippet, 280), currentQuery) + '</p>' +
            (scorePercent !== null ? '<div class="result-score"><div class="result-score-bar"><div class="result-score-fill" style="width:' + scorePercent + '%"></div></div>' + (scorePercent / 100).toFixed(2) + '</div>' : '') +
            '</article>';
    }

    function doSearch(query, mode, pushState) {
        if (!query.trim()) return;

        currentOffset = 0;
        currentQuery = query.trim();

        if (currentController) {
            currentController.abort();
        }
        currentController = new AbortController();

        setState('results');
        searchInput.classList.add('loading');
        resultsList.innerHTML = renderSkeletons(3);
        resultsMeta.textContent = '';

        var startTime = performance.now();
        var params = '?q=' + encodeURIComponent(query.trim()) + '&limit=' + limit + '&mode=' + mode;

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
                    resultsList.innerHTML = '<div class="error-msg">' +
                        '<span class="error-msg-icon">&times;</span>' +
                        '<p>' + escapeHtml(data.error || 'Something went wrong with the search.') + '</p>' +
                        '<button class="retry-btn" onclick="window.__retry()">Try again</button>' +
                        '</div>';
                    return;
                }

                var results = data.data || [];

                if (results.length === 0) {
                    resultsMeta.innerHTML = '<span class="meta-query">\u201c' + escapeHtml(query) + '\u201d</span>' +
                        '<span class="meta-sep">&middot;</span> no results' +
                        '<span class="meta-sep">&middot;</span> <span class="meta-mode">' + mode + '</span>' +
                        '<span class="meta-sep">&middot;</span> ' + elapsed + 's';
                    resultsList.innerHTML = '<div class="no-results">' +
                        '<span class="no-results-ornament">&oslash;</span>' +
                        '<div class="no-results-title">No results found</div>' +
                        '<div class="no-results-text">Nothing matched \u201c' + escapeHtml(query) + '\u201d in the current index.</div>' +
                        '<div class="no-results-suggestions">' +
                        '<span>Try different keywords or a broader query</span>' +
                        '<span>Try switching to <em>' + (mode === 'semantic' ? 'hybrid' : 'semantic') + '</em> mode</span>' +
                        '</div>' +
                        '</div>';
                    return;
                }

                resultsMeta.innerHTML = '<span class="meta-query">\u201c' + escapeHtml(query) + '\u201d</span>' +
                    '<span class="meta-sep">&middot;</span> ' + results.length + ' result' + (results.length !== 1 ? 's' : '') +
                    '<span class="meta-sep">&middot;</span> <span class="meta-mode">' + mode + '</span>' +
                    '<span class="meta-sep">&middot;</span> ' + elapsed + 's';

                var grouped = [];
                var domainMap = {};
                results.forEach(function (r) {
                    var hostname = extractHostname(r.url);
                    if (!domainMap[hostname]) {
                        domainMap[hostname] = { domain: hostname, results: [] };
                        grouped.push(domainMap[hostname]);
                    }
                    domainMap[hostname].results.push(r);
                });

                var html = '';
                var globalIdx = 0;
                grouped.forEach(function (group) {
                    var faviconSrc = API_BASE + '/favicon?domain=' + encodeURIComponent(group.domain);
                    html += '<div class="domain-group" style="--i:' + globalIdx + '">' +
                        '<div class="domain-header">' +
                        '<img class="domain-favicon" src="' + faviconSrc + '" alt="" width="16" height="16" loading="lazy" onerror="this.style.display=\'none\'">' +
                        '<a class="domain-name" href="https://' + escapeHtml(group.domain) + '/" target="_blank" rel="noopener">' + escapeHtml(group.domain) + '</a>' +
                        '<span class="domain-count">' + group.results.length + '</span>' +
                        '</div>' +
                        '<div class="domain-results">';
                    group.results.forEach(function (r) {
                        var scorePercent = typeof r.score === 'number' ? Math.min(100, Math.round(r.score * 100)) : null;
                        var externalIcon = '<svg class="external-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3H3v10h10v-3"/><path d="M9 2h5v5"/><path d="M14 2L7 9"/></svg>';
                        html += '<article class="result" style="--i:' + globalIdx + '">' +
                            '<cite class="result-url">' + escapeHtml(formatUrl(r.url)) + '</cite>' +
                            '<h3><a href="' + escapeHtml(r.url) + '" class="result-title" target="_blank" rel="noopener">' + escapeHtml(r.title || 'Untitled') + externalIcon + '</a></h3>' +
                            '<p class="result-snippet">' + highlightTerms(truncateSnippet(r.snippet, 280), query) + '</p>' +
                            (scorePercent !== null ? '<div class="result-score"><div class="result-score-bar"><div class="result-score-fill" style="width:' + scorePercent + '%"></div></div>' + (scorePercent / 100).toFixed(2) + '</div>' : '') +
                            '</article>';
                        globalIdx++;
                    });
                    html += '</div></div>';
                });
                resultsList.innerHTML = html;
                if (results.length >= limit) {
                    resultsList.innerHTML += '<div class="load-more-wrap"><button class="load-more-btn" onclick="window.__loadMore()">Load more results</button></div>';
                }
            })
            .catch(function (err) {
                searchInput.classList.remove('loading');
                if (err.name === 'AbortError') return;
                resultsMeta.textContent = '';
                resultsList.innerHTML = '<div class="error-msg">' +
                    '<span class="error-msg-icon">&times;</span>' +
                    '<p>Could not reach the search index. The server may be temporarily unavailable.</p>' +
                    '<button class="retry-btn" onclick="window.__retry()">Try again</button>' +
                    '</div>';
            });
    }

    window.__retry = function () {
        doSearch(searchInput.value, currentMode, false);
    };

    window.__loadMore = function () {
        currentOffset += limit;
        fetch(API_BASE + '/search?q=' + encodeURIComponent(currentQuery) + '&limit=' + limit + '&offset=' + currentOffset + '&mode=' + currentMode)
        .then(function (r) { return r.json(); })
        .then(function (data) {
            if (data.success && data.data) {
                var moreBtn = document.querySelector('.load-more-wrap');
                if (moreBtn) moreBtn.remove();
                var html = '';
                for (var i = 0; i < data.data.length; i++) {
                    html += buildResultHtml(data.data[i]);
                }
                resultsList.innerHTML += html;
                if (data.data.length >= limit) {
                    resultsList.innerHTML += '<div class="load-more-wrap"><button class="load-more-btn" onclick="window.__loadMore()">Load more results</button></div>';
                }
            }
        });
    };

    function goHome() {
        setState('home');
        searchInput.value = '';
        updateClearBtn();
        acDismiss();
        resultsList.innerHTML = '';
        resultsMeta.textContent = '';
        searchInput.focus();
        document.title = 'slither';
        history.pushState({}, '', window.location.pathname);
    }

    function acShow() {
        autocomplete.classList.add('visible');
        searchInput.classList.add('has-autocomplete');
        searchInput.setAttribute('aria-expanded', 'true');
    }

    function acHide() {
        autocomplete.classList.remove('visible');
        searchInput.classList.remove('has-autocomplete');
        searchInput.setAttribute('aria-expanded', 'false');
        acActiveIndex = -1;
    }

    function acDismiss() {
        acHide();
        acResults = [];
        acLastQuery = '';
    }

    function acHighlight(index) {
        var items = autocompleteList.querySelectorAll('.autocomplete-item');
        items.forEach(function (el) { el.classList.remove('active'); });
        acActiveIndex = index;
        if (index >= 0 && index < items.length) {
            items[index].classList.add('active');
            items[index].scrollIntoView({ block: 'nearest' });
            searchInput.setAttribute('aria-activedescendant', 'ac-item-' + index);
        } else {
            searchInput.removeAttribute('aria-activedescendant');
        }
    }

    function acSelect(item) {
        searchInput.value = item.title || item.url;
        acDismiss();
        searchInput.blur();
        updateClearBtn();
        doSearch(searchInput.value, currentMode, true);
    }

    function acRender(results, query) {
        acResults = results;
        acActiveIndex = -1;

        if (results.length === 0) {
            acHide();
            return;
        }

        var html = '';
        var limit = Math.min(results.length, 6);
        for (var i = 0; i < limit; i++) {
            var r = results[i];
            var title = highlightTerms(r.title || 'Untitled', query);
            var url = escapeHtml(formatUrl(r.url));
            var hostname = extractHostname(r.url);
            var faviconSrc = API_BASE + '/favicon?domain=' + encodeURIComponent(hostname);
            html += '<li class="autocomplete-item" id="ac-item-' + i + '" role="option" data-index="' + i + '">' +
                '<img class="autocomplete-item-icon" src="' + faviconSrc + '" alt="" width="16" height="16" loading="lazy" onerror="this.style.opacity=\'0\'">' +
                '<div class="autocomplete-item-content">' +
                '<span class="autocomplete-item-title">' + title + '</span>' +
                '<span class="autocomplete-item-url">' + url + '</span>' +
                '</div>' +
                '</li>';
        }
        autocompleteList.innerHTML = html;
        acShow();

        autocompleteList.querySelectorAll('.autocomplete-item').forEach(function (el) {
            el.addEventListener('mousedown', function (e) {
                e.preventDefault();
                var idx = parseInt(el.getAttribute('data-index'), 10);
                if (acResults[idx]) acSelect(acResults[idx]);
            });
            el.addEventListener('mouseenter', function () {
                acHighlight(parseInt(el.getAttribute('data-index'), 10));
            });
        });
    }

    function acFetch(query) {
        if (acController) acController.abort();
        acController = new AbortController();
        acLastQuery = query;

        var params = '?q=' + encodeURIComponent(query.trim()) + '&limit=6&mode=text';
        fetch(API_BASE + '/search' + params, { signal: acController.signal })
            .then(function (r) { return r.json(); })
            .then(function (data) {
                if (document.activeElement !== searchInput) return;
                if (data.success && data.data && data.data.length > 0) {
                    acRender(data.data, query);
                } else {
                    acHide();
                    acResults = [];
                }
            })
            .catch(function (err) {
                if (err.name !== 'AbortError') {
                    acHide();
                    acResults = [];
                }
            });
    }

    function acOnInput() {
        var val = searchInput.value.trim();
        if (val.length < 2) {
            acHide();
            acResults = [];
            acLastQuery = '';
            return;
        }
        clearTimeout(acDebounceTimer);
        acDebounceTimer = setTimeout(function () {
            acFetch(val);
        }, 150);
    }

    searchForm.addEventListener('submit', function (e) {
        e.preventDefault();
        acDismiss();
        searchInput.blur();
        doSearch(searchInput.value, currentMode, true);
    });

    searchInput.addEventListener('input', function () {
        updateClearBtn();
        acOnInput();
    });

    searchInput.addEventListener('focus', function () {
        var val = searchInput.value.trim();
        if (val.length >= 2 && acResults.length > 0 && acLastQuery === val) {
            acShow();
        } else if (val.length >= 2) {
            acOnInput();
        }
    });

    searchClear.addEventListener('click', function () {
        searchInput.value = '';
        updateClearBtn();
        acDismiss();
        searchInput.focus();
        if (body.getAttribute('data-state') === 'results') {
            goHome();
        }
    });

    document.addEventListener('mousedown', function (e) {
        if (!autocomplete.contains(e.target) && e.target !== searchInput) {
            acHide();
        }
    });

    window.addEventListener('scroll', function () {
        if (autocomplete.classList.contains('visible')) {
            acHide();
        }
    }, { passive: true });

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
        var acVisible = autocomplete.classList.contains('visible');

        if (e.key === 'ArrowDown' && acVisible) {
            e.preventDefault();
            var maxIdx = Math.min(acResults.length, 6) - 1;
            acHighlight(acActiveIndex < maxIdx ? acActiveIndex + 1 : 0);
            return;
        }
        if (e.key === 'ArrowUp' && acVisible) {
            e.preventDefault();
            var maxIdx2 = Math.min(acResults.length, 6) - 1;
            acHighlight(acActiveIndex > 0 ? acActiveIndex - 1 : maxIdx2);
            return;
        }
        if (e.key === 'Enter' && acVisible && acActiveIndex >= 0) {
            e.preventDefault();
            if (acResults[acActiveIndex]) acSelect(acResults[acActiveIndex]);
            return;
        }
        if (e.key === 'Escape' && acVisible) {
            e.preventDefault();
            acDismiss();
            return;
        }
        if (e.key === 'Tab' && acVisible) {
            acDismiss();
        }
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
        startStatsPolling();

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
