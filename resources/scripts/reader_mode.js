(function() {
    'use strict';

    if (window.__TakeReaderMode) return;

    let originalState = null;
    let currentTheme = localStorage.getItem('take_reader_theme') || 'auto';
    let currentFontSize = parseInt(localStorage.getItem('take_reader_fontsize') || '18', 10);

    const UNLIKELY_CANDIDATES = /combx|comment|community|disqus|extra|foot|header|menu|remark|rss|shoutbox|sidebar|sponsor|ad-break|agegate|pagination|pager|popup|tweet|twitter|fb-|facebook|meta-social|share|nav|login|signup/i;
    const OK_MAYBE_ITS_CANDIDATE = /and|article|body|column|main|shadow|content|entry|page|pagination|post|text|blog|story/i;
    const REMOVE_TAGS = ['script', 'style', 'noscript', 'nav', 'header', 'footer', 'aside', 'form', 'button', 'iframe', 'img', 'picture', 'figure', 'svg', 'canvas', 'video', 'audio'];

    function getArticleTitle() {
        const ogTitle = document.querySelector('meta[property="og:title"]')?.getAttribute('content');
        if (ogTitle && ogTitle.trim()) return ogTitle.trim();

        const h1 = document.querySelector('h1');
        if (h1 && h1.innerText.trim()) return h1.innerText.trim();

        return document.title || 'Статья';
    }

    function getArticleByline() {
        const authorMeta = document.querySelector('meta[name="author"]')?.getAttribute('content')
            || document.querySelector('meta[property="article:author"]')?.getAttribute('content');
        if (authorMeta && authorMeta.trim()) return authorMeta.trim();

        const bylineEl = document.querySelector('[rel="author"], .byline, .author, .entry-author, .article__author');
        if (bylineEl && bylineEl.innerText.trim()) return bylineEl.innerText.trim();

        const timeEl = document.querySelector('time');
        if (timeEl && timeEl.innerText.trim()) return timeEl.innerText.trim();

        return '';
    }

    function scoreNode(node) {
        if (!node || node.nodeType !== Node.ELEMENT_NODE) return 0;
        let score = 0;
        const tag = node.tagName.toLowerCase();

        if (tag === 'article' || tag === 'main' || node.getAttribute('role') === 'main') score += 35;
        if (tag === 'section') score += 15;
        if (tag === 'div') score += 5;

        const className = typeof node.className === 'string' ? node.className : '';
        const id = node.id || '';
        const matchStr = `${className} ${id}`;

        if (OK_MAYBE_ITS_CANDIDATE.test(matchStr)) score += 25;
        if (UNLIKELY_CANDIDATES.test(matchStr)) score -= 35;

        const pTags = node.getElementsByTagName('p');
        score += pTags.length * 5;

        const textLen = (node.innerText || '').trim().length;
        score += Math.min(Math.floor(textLen / 100), 50);

        const links = node.getElementsByTagName('a');
        let linkTextLen = 0;
        for (let i = 0; i < links.length; i++) {
            linkTextLen += (links[i].innerText || '').length;
        }
        const linkDensity = textLen > 0 ? (linkTextLen / textLen) : 1;
        if (linkDensity > 0.4) score -= 30;

        return score;
    }

    function findArticleElement() {
        const explicit = document.querySelector('article, [role="main"], .article-content, .post-content, .entry-content, #content, .story-body');
        if (explicit && (explicit.innerText || '').trim().length > 250) {
            return explicit;
        }

        const candidates = document.querySelectorAll('div, section, article, main');
        let bestCandidate = document.body;
        let maxScore = -999;

        for (let i = 0; i < candidates.length; i++) {
            const c = candidates[i];
            const score = scoreNode(c);
            if (score > maxScore) {
                maxScore = score;
                bestCandidate = c;
            }
        }
        return bestCandidate;
    }

    function cleanContent(rootNode) {
        const clone = rootNode.cloneNode(true);

        REMOVE_TAGS.forEach((tag) => {
            const els = clone.getElementsByTagName(tag);
            while (els.length > 0) {
                els[0].parentNode?.removeChild(els[0]);
            }
        });

        const all = clone.querySelectorAll('*');
        for (let i = 0; i < all.length; i++) {
            const el = all[i];
            const cls = typeof el.className === 'string' ? el.className : '';
            const id = el.id || '';
            const test = `${cls} ${id}`;
            if (UNLIKELY_CANDIDATES.test(test) && !OK_MAYBE_ITS_CANDIDATE.test(test)) {
                el.parentNode?.removeChild(el);
                continue;
            }

            // Remove non-essential attributes
            el.removeAttribute('onclick');
            el.removeAttribute('onmouseover');
            el.removeAttribute('style');
        }

        // Zero images / photos / embedded media in Reader Mode as requested
        const mediaEls = clone.querySelectorAll('img, picture, figure, svg, canvas, video, audio');
        for (let i = 0; i < mediaEls.length; i++) {
            mediaEls[i].parentNode?.removeChild(mediaEls[i]);
        }

        return clone.innerHTML;
    }

    function postReaderIpc(isActive) {
        try {
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.takeReaderMode) {
                window.webkit.messageHandlers.takeReaderMode.postMessage(JSON.stringify({
                    active: isActive,
                    url: window.location.href
                }));
            }
        } catch (_) {}
    }

    function enterReaderMode() {
        if (originalState && originalState.isActive) return;

        const title = getArticleTitle();
        const byline = getArticleByline();
        const container = findArticleElement();
        const bodyHtml = cleanContent(container);

        originalState = {
            isActive: true,
            originalHtml: document.documentElement.outerHTML,
            scrollTop: window.scrollY
        };

        const readerCss = `
            :root {
                --r-bg: #fdfdfd;
                --r-fg: #1c1c1e;
                --r-muted: #6e6e73;
                --r-border: rgba(0, 0, 0, 0.12);
                --r-bar-bg: rgba(255, 255, 255, 0.85);
                --r-card-bg: #f2f2f7;
                --r-link: #007aff;
                --r-font: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Liberation Sans", sans-serif;
            }
            [data-reader-theme="sepia"] {
                --r-bg: #fbf0d9;
                --r-fg: #5f4b32;
                --r-muted: #8c7658;
                --r-border: rgba(95, 75, 50, 0.15);
                --r-bar-bg: rgba(251, 240, 217, 0.85);
                --r-card-bg: #f4e3c3;
                --r-link: #a05a2c;
            }
            [data-reader-theme="dark"] {
                --r-bg: #1c1c1e;
                --r-fg: #f2f2f7;
                --r-muted: #98989d;
                --r-border: rgba(255, 255, 255, 0.15);
                --r-bar-bg: rgba(28, 28, 30, 0.85);
                --r-card-bg: #2c2c2e;
                --r-link: #0a84ff;
            }
            @media (prefers-color-scheme: dark) {
                [data-reader-theme="auto"] {
                    --r-bg: #1c1c1e;
                    --r-fg: #f2f2f7;
                    --r-muted: #98989d;
                    --r-border: rgba(255, 255, 255, 0.15);
                    --r-bar-bg: rgba(28, 28, 30, 0.85);
                    --r-card-bg: #2c2c2e;
                    --r-link: #0a84ff;
                }
            }
            html, body {
                margin: 0;
                padding: 0;
                background-color: var(--r-bg) !important;
                color: var(--r-fg) !important;
                font-family: var(--r-font);
                font-size: ${currentFontSize}px;
                line-height: 1.75;
                text-rendering: optimizeLegibility;
                -webkit-font-smoothing: antialiased;
                transition: background-color 0.2s ease, color 0.2s ease;
            }
            /* Clean reading without images, ads or banners */
            img, picture, figure, svg, canvas, video, audio {
                display: none !important;
            }
            #take-reader-bar {
                position: sticky;
                top: 0;
                left: 0;
                right: 0;
                display: flex;
                align-items: center;
                justify-content: space-between;
                padding: 10px 24px;
                background: var(--r-bar-bg);
                backdrop-filter: blur(20px);
                -webkit-backdrop-filter: blur(20px);
                border-bottom: 1px solid var(--r-border);
                z-index: 10000;
            }
            .take-bar-group {
                display: flex;
                align-items: center;
                gap: 8px;
            }
            .take-bar-btn {
                background: var(--r-card-bg);
                color: var(--r-fg);
                border: 1px solid var(--r-border);
                border-radius: 8px;
                padding: 6px 12px;
                font-size: 13px;
                font-weight: 500;
                cursor: pointer;
                transition: opacity 0.15s ease, transform 0.1s ease;
                user-select: none;
            }
            .take-bar-btn:hover {
                opacity: 0.85;
            }
            .take-bar-btn:active {
                transform: scale(0.97);
            }
            #take-reader-article {
                max-width: 700px;
                margin: 0 auto;
                padding: 40px 20px 80px 20px;
                box-sizing: border-box;
            }
            #take-reader-header {
                margin-bottom: 32px;
                border-bottom: 1px solid var(--r-border);
                padding-bottom: 24px;
            }
            #take-reader-header h1 {
                font-size: 2.1em;
                line-height: 1.25;
                font-weight: 700;
                margin: 0 0 16px 0;
                letter-spacing: -0.02em;
            }
            .take-reader-meta {
                font-size: 0.9em;
                color: var(--r-muted);
                font-weight: 500;
            }
            #take-reader-content p {
                margin: 0 0 1.5em 0;
            }
            #take-reader-content h2, #take-reader-content h3, #take-reader-content h4 {
                line-height: 1.35;
                margin: 2em 0 0.8em 0;
                font-weight: 600;
            }
            #take-reader-content blockquote {
                margin: 1.5em 0;
                padding-left: 20px;
                border-left: 4px solid var(--r-link);
                font-style: italic;
                color: var(--r-muted);
            }
            #take-reader-content a {
                color: var(--r-link);
                text-decoration: underline;
                text-underline-offset: 3px;
            }
            #take-reader-content pre, #take-reader-content code {
                font-family: monospace;
                background: var(--r-card-bg);
                border-radius: 6px;
                padding: 2px 6px;
                font-size: 0.9em;
            }
            #take-reader-content pre {
                padding: 16px;
                overflow-x: auto;
            }
            #take-reader-content pre code {
                padding: 0;
                background: transparent;
            }
        `;

        const bylineHtml = byline ? `<div class="take-reader-meta">${byline}</div>` : '';

        const readerHtml = `
            <!DOCTYPE html>
            <html lang="${document.documentElement.lang || 'ru'}" data-reader-theme="${currentTheme}">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>${title}</title>
                <style>${readerCss}</style>
            </head>
            <body>
                <div id="take-reader-bar">
                    <div class="take-bar-group">
                        <button class="take-bar-btn" id="take-exit-reader">✕ Вернуться к оригиналу</button>
                    </div>
                    <div class="take-bar-group">
                        <button class="take-bar-btn" id="take-font-down" title="Уменьшить шрифт">A−</button>
                        <button class="take-bar-btn" id="take-font-up" title="Увеличить шрифт">A+</button>
                        <button class="take-bar-btn" id="take-theme-btn" title="Сменить тему">Тема</button>
                    </div>
                </div>
                <article id="take-reader-article">
                    <header id="take-reader-header">
                        <h1>${title}</h1>
                        ${bylineHtml}
                    </header>
                    <div id="take-reader-content">
                        ${bodyHtml}
                    </div>
                </article>
            </body>
            </html>
        `;

        document.open();
        document.write(readerHtml);
        document.close();

        // Wire toolbar inside reader mode
        const exitBtn = document.getElementById('take-exit-reader');
        if (exitBtn) exitBtn.addEventListener('click', exitReaderMode);

        const fontUpBtn = document.getElementById('take-font-up');
        if (fontUpBtn) fontUpBtn.addEventListener('click', () => adjustFontSize(1));

        const fontDownBtn = document.getElementById('take-font-down');
        if (fontDownBtn) fontDownBtn.addEventListener('click', () => adjustFontSize(-1));

        const themeBtn = document.getElementById('take-theme-btn');
        if (themeBtn) themeBtn.addEventListener('click', cycleTheme);

        postReaderIpc(true);
    }

    function exitReaderMode() {
        if (!originalState || !originalState.originalHtml) return;

        const html = originalState.originalHtml;
        const scrollY = originalState.scrollTop;
        originalState = null;

        document.open();
        document.write(html);
        document.close();

        window.scrollTo(0, scrollY);
        postReaderIpc(false);
    }

    function adjustFontSize(delta) {
        currentFontSize = Math.max(14, Math.min(26, currentFontSize + delta));
        document.body.style.fontSize = `${currentFontSize}px`;
        localStorage.setItem('take_reader_fontsize', currentFontSize);
    }

    function cycleTheme() {
        const themes = ['auto', 'sepia', 'dark'];
        const nextIdx = (themes.indexOf(currentTheme) + 1) % themes.length;
        currentTheme = themes[nextIdx];
        document.documentElement.setAttribute('data-reader-theme', currentTheme);
        localStorage.setItem('take_reader_theme', currentTheme);
    }

    window.__TakeReaderMode = {
        isActive() {
            return Boolean(originalState && originalState.isActive);
        },
        toggle() {
            if (originalState && originalState.isActive) {
                exitReaderMode();
            } else {
                enterReaderMode();
            }
        },
        enter: enterReaderMode,
        exit: exitReaderMode
    };
})();
