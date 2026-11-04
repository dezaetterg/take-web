// Take Web Start Page Controller

window.addEventListener('DOMContentLoaded', () => {
    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.take) {
        window.webkit.messageHandlers.take.postMessage({ type: 'startpage_ready' });
    }

    const searchForm = document.getElementById('search-form');
    const searchInput = document.getElementById('search-input');
    const enginePicker = document.getElementById('search-engine-picker');
    const engineBtn = document.getElementById('search-engine-btn');
    const engineDropdown = document.getElementById('search-engine-dropdown');
    let oneOffEngine = null;

    const ENGINE_URLS = {
        google: (q) => 'https://www.google.com/search?q=' + q,
        yandex: (q) => 'https://yandex.ru/search/?text=' + q,
        duckduckgo: (q) => 'https://duckduckgo.com/?q=' + q,
        bing: (q) => 'https://www.bing.com/search?q=' + q,
        wikipedia: (q) => 'https://ru.wikipedia.org/wiki/Special:Search?search=' + q,
        perplexity: (q) => 'https://www.perplexity.ai/search?q=' + q,
        startpage: (q) => 'https://www.startpage.com/do/dsearch?q=' + q + '&cat=web&language=english',
        brave: (q) => 'https://search.brave.com/search?q=' + q,
        ecosia: (q) => 'https://www.ecosia.org/search?q=' + q
    };

    const ENGINE_NAMES = {
        google: 'Google',
        yandex: 'Яндекс',
        duckduckgo: 'DuckDuckGo',
        bing: 'Bing',
        wikipedia: 'Википедии',
        perplexity: 'Perplexity',
        startpage: 'Startpage',
        brave: 'Brave',
        ecosia: 'Ecosia'
    };

    function toggleEngineDropdown(show) {
        if (!engineDropdown) return;
        const willShow = typeof show === 'boolean' ? show : engineDropdown.hidden;
        engineDropdown.hidden = !willShow;
        if (enginePicker) {
            enginePicker.classList.toggle('open', willShow);
        }
    }

    function executeSearch(query, explicitEngine) {
        let target = query;
        if (/^(https?:\/\/|take:\/\/|file:\/\/)/i.test(query)) {
            target = query;
        } else if ((query.includes('.') && !query.includes(' ')) || query.startsWith('localhost')) {
            target = 'https://' + query;
        } else {
            const engine = explicitEngine || oneOffEngine || (localStorage.getItem('default_search_engine') || 'duckduckgo').toLowerCase();
            const encoded = encodeURIComponent(query);
            const builder = ENGINE_URLS[engine] || ENGINE_URLS.duckduckgo;
            target = builder(encoded);
        }
        oneOffEngine = null;
        if (searchInput) {
            searchInput.placeholder = 'Введите запрос или адрес';
        }
        window.location.href = target;
    }

    if (engineBtn && engineDropdown) {
        engineBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            toggleEngineDropdown();
        });

        document.addEventListener('click', (e) => {
            if (enginePicker && !enginePicker.contains(e.target)) {
                toggleEngineDropdown(false);
            }
        });

        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && !engineDropdown.hidden) {
                toggleEngineDropdown(false);
            }
        });

        engineDropdown.querySelectorAll('.search-engine-item').forEach(item => {
            item.addEventListener('click', (e) => {
                e.stopPropagation();
                const eng = item.getAttribute('data-engine');
                toggleEngineDropdown(false);

                const query = searchInput ? searchInput.value.trim() : '';
                if (query) {
                    executeSearch(query, eng);
                } else if (searchInput) {
                    oneOffEngine = eng;
                    searchInput.placeholder = `Поиск в ${ENGINE_NAMES[eng] || eng}...`;
                    searchInput.focus();
                }
            });
        });
    }

    if (searchForm && searchInput) {
        searchForm.addEventListener('submit', (e) => {
            e.preventDefault();
            const query = searchInput.value.trim();
            if (!query) return;
            executeSearch(query, null);
        });
    }

    const privacyCard = document.getElementById('privacy-card');
    if (privacyCard) {
        privacyCard.addEventListener('click', () => {
            window.location.href = 'take://trackers';
        });
    }

    // Firefox-style Start Page Customization
    initCustomization();
    initAddFavoriteModal();
});

function openAddFavoriteModal() {
    const modal = document.getElementById('add-favorite-modal');
    if (!modal) return;
    const titleInput = document.getElementById('fav-title-input');
    const urlInput = document.getElementById('fav-url-input');
    if (titleInput) titleInput.value = '';
    if (urlInput) urlInput.value = '';
    modal.removeAttribute('hidden');
    if (titleInput) titleInput.focus();
}

function closeAddFavoriteModal() {
    const modal = document.getElementById('add-favorite-modal');
    if (modal) modal.setAttribute('hidden', '');
}

function initAddFavoriteModal() {
    const initialBtn = document.getElementById('add-favorite-btn');
    if (initialBtn) {
        initialBtn.addEventListener('click', (e) => {
            e.preventDefault();
            openAddFavoriteModal();
        });
    }

    const modal = document.getElementById('add-favorite-modal');
    const closeBtn = document.getElementById('add-favorite-close');
    const cancelBtn = document.getElementById('add-favorite-cancel');
    const form = document.getElementById('add-favorite-form');

    if (closeBtn) closeBtn.addEventListener('click', closeAddFavoriteModal);
    if (cancelBtn) cancelBtn.addEventListener('click', closeAddFavoriteModal);
    if (modal) {
        modal.addEventListener('click', (e) => {
            if (e.target === modal) closeAddFavoriteModal();
        });
    }

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && modal && !modal.hasAttribute('hidden')) {
            closeAddFavoriteModal();
        }
    });

    if (form) {
        form.addEventListener('submit', (e) => {
            e.preventDefault();
            const titleInput = document.getElementById('fav-title-input');
            const urlInput = document.getElementById('fav-url-input');
            const title = titleInput ? titleInput.value.trim() : '';
            let url = urlInput ? urlInput.value.trim() : '';
            if (!title || !url) return;

            if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(url)) {
                url = 'https://' + url;
            }

            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.takeAction) {
                window.webkit.messageHandlers.takeAction.postMessage(JSON.stringify({
                    action: 'add_favorite',
                    title: title,
                    url: url
                }));
            }
            closeAddFavoriteModal();
        });
    }
}

function initCustomization() {
    const customizeBtn = document.getElementById('customize-btn');
    const backdrop = document.getElementById('customize-backdrop');
    const drawer = document.getElementById('customize-drawer');
    const closeBtn = document.getElementById('drawer-close-btn');
    const wallpaperEl = document.getElementById('wallpaper');
    const fileInput = document.getElementById('wallpaper-file-input');
    const uploadCustomBtn = document.getElementById('upload-custom-btn');
    const customThumb = document.getElementById('custom-thumb');
    const customCardLabel = document.getElementById('custom-card-label');
    const deleteCustomBtn = document.getElementById('delete-custom-btn');

    if (!customizeBtn || !drawer || !wallpaperEl) return;

    // --- Drawer Visibility ---
    function openDrawer() {
        drawer.classList.add('open');
        if (backdrop) backdrop.classList.add('active');
        drawer.setAttribute('aria-hidden', 'false');
    }

    function closeDrawer() {
        drawer.classList.remove('open');
        if (backdrop) backdrop.classList.remove('active');
        drawer.setAttribute('aria-hidden', 'true');
    }

    customizeBtn.addEventListener('click', openDrawer);
    if (closeBtn) closeBtn.addEventListener('click', closeDrawer);
    if (backdrop) backdrop.addEventListener('click', closeDrawer);
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && drawer.classList.contains('open')) {
            closeDrawer();
        }
    });

    // --- Wallpaper Management ---
    const allCards = document.querySelectorAll('.wallpaper-card');

    function updateActiveCard(type, val) {
        allCards.forEach(c => {
            const cardType = c.dataset.wallpaper;
            if (type === 'none' && cardType === 'none') {
                c.classList.add('active');
            } else if (type === 'custom' && cardType === 'custom') {
                c.classList.add('active');
            } else if (type === 'preset' && cardType === val) {
                c.classList.add('active');
            } else {
                c.classList.remove('active');
            }
        });
    }

    function applyWallpaper(type, value) {
        if (type === 'none') {
            wallpaperEl.style.backgroundImage = '';
            wallpaperEl.classList.remove('active');
            document.body.classList.remove('has-wallpaper');
            localStorage.setItem('takeweb_wallpaper_type', 'none');
            updateActiveCard('none');
        } else if (type === 'custom' && value) {
            wallpaperEl.style.backgroundImage = `url("${value}")`;
            wallpaperEl.classList.add('active');
            document.body.classList.add('has-wallpaper');
            localStorage.setItem('takeweb_wallpaper_type', 'custom');
            updateActiveCard('custom');
        } else if (type === 'preset' && value) {
            if (value.startsWith('gradient:')) {
                wallpaperEl.style.backgroundImage = value.substring('gradient:'.length);
            } else {
                wallpaperEl.style.backgroundImage = `url("${value}")`;
            }
            wallpaperEl.classList.add('active');
            document.body.classList.add('has-wallpaper');
            localStorage.setItem('takeweb_wallpaper_type', 'preset');
            localStorage.setItem('takeweb_wallpaper_preset_url', value);
            updateActiveCard('preset', value);
        }
    }

    // Restore custom photo state if exists
    function refreshCustomThumbnail(dataUrl) {
        if (dataUrl && customThumb) {
            customThumb.style.backgroundImage = `url("${dataUrl}")`;
            customThumb.innerHTML = '';
            if (customCardLabel) customCardLabel.textContent = 'Мое фото';
            if (deleteCustomBtn) deleteCustomBtn.style.display = 'flex';
        } else if (customThumb) {
            customThumb.style.backgroundImage = '';
            customThumb.innerHTML = `
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <line x1="12" y1="5" x2="12" y2="19"></line>
                    <line x1="5" y1="12" x2="19" y2="12"></line>
                </svg>`;
            if (customCardLabel) customCardLabel.textContent = 'Свое фото';
            if (deleteCustomBtn) deleteCustomBtn.style.display = 'none';
        }
    }

    const savedCustomPhoto = localStorage.getItem('takeweb_custom_wallpaper');
    if (savedCustomPhoto) {
        refreshCustomThumbnail(savedCustomPhoto);
    }

    // Determine initial wallpaper
    const isPrivateSession = document.documentElement.getAttribute('data-private') === 'true' ||
                             document.body.classList.contains('private-session') ||
                             window.location.search.includes('private=1');
    if (isPrivateSession) {
        document.documentElement.setAttribute('data-private', 'true');
        document.body.classList.add('private-session');
    }

    const savedType = localStorage.getItem('takeweb_wallpaper_type');
    if (isPrivateSession && (!savedType || savedType === 'none')) {
        applyWallpaper('preset', 'purple_hills.png');
    } else if (savedType === 'custom' && savedCustomPhoto) {
        applyWallpaper('custom', savedCustomPhoto);
    } else if (savedType === 'preset') {
        const presetUrl = localStorage.getItem('takeweb_wallpaper_preset_url');
        if (presetUrl) {
            applyWallpaper('preset', presetUrl);
        } else {
            applyWallpaper('none');
        }
    } else if (!savedType && savedCustomPhoto) {
        // Migration from previous setup
        applyWallpaper('custom', savedCustomPhoto);
    } else {
        applyWallpaper('none');
    }

    // Card click handlers
    allCards.forEach(card => {
        card.addEventListener('click', () => {
            const val = card.dataset.wallpaper;
            if (val === 'none') {
                applyWallpaper('none');
            } else if (val === 'custom') {
                const currentCustom = localStorage.getItem('takeweb_custom_wallpaper');
                if (currentCustom) {
                    applyWallpaper('custom', currentCustom);
                } else if (fileInput) {
                    fileInput.click();
                }
            } else {
                applyWallpaper('preset', val);
            }
        });
    });

    // File input change for custom photo
    if (fileInput) {
        fileInput.addEventListener('change', (e) => {
            const file = e.target.files && e.target.files[0];
            if (!file) return;

            const reader = new FileReader();
            reader.onload = (loadEvent) => {
                const rawData = loadEvent.target.result;
                const img = new Image();
                img.onload = () => {
                    const maxDim = 2560;
                    let w = img.width;
                    let h = img.height;
                    if (w > maxDim || h > maxDim) {
                        if (w > h) {
                            h = Math.round((h * maxDim) / w);
                            w = maxDim;
                        } else {
                            w = Math.round((w * maxDim) / h);
                            h = maxDim;
                        }
                    }
                    const canvas = document.createElement('canvas');
                    canvas.width = w;
                    canvas.height = h;
                    const ctx = canvas.getContext('2d');
                    ctx.drawImage(img, 0, 0, w, h);
                    const optimizedData = canvas.toDataURL('image/jpeg', 0.90);

                    try {
                        localStorage.setItem('takeweb_custom_wallpaper', optimizedData);
                    } catch (err) {
                        console.warn('Could not save wallpaper to localStorage:', err);
                    }

                    refreshCustomThumbnail(optimizedData);
                    applyWallpaper('custom', optimizedData);
                };
                img.src = rawData;
            };
            reader.readAsDataURL(file);
            fileInput.value = '';
        });
    }

    // Delete custom photo button
    if (deleteCustomBtn) {
        deleteCustomBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            localStorage.removeItem('takeweb_custom_wallpaper');
            refreshCustomThumbnail(null);
            if (localStorage.getItem('takeweb_wallpaper_type') === 'custom') {
                applyWallpaper('none');
            }
        });
    }

    // --- Section Toggles (Firefox Style) ---
    const sections = [
        { id: 'section-search', toggleId: 'toggle-search', key: 'takeweb_show_search' },
        { id: 'section-favorites', toggleId: 'toggle-favorites', key: 'takeweb_show_favorites' },
        { id: 'section-privacy', toggleId: 'toggle-privacy', key: 'takeweb_show_privacy' },
        { id: 'section-reading', toggleId: 'toggle-reading', key: 'takeweb_show_reading' }
    ];

    sections.forEach(({ id, toggleId, key }) => {
        const secEl = document.getElementById(id);
        const toggleEl = document.getElementById(toggleId);
        if (!secEl || !toggleEl) return;

        // Restore saved preference (default to true)
        const savedVal = localStorage.getItem(key);
        const isVisible = savedVal === null ? true : savedVal === '1';
        toggleEl.checked = isVisible;
        secEl.style.display = isVisible ? '' : 'none';

        toggleEl.addEventListener('change', () => {
            const checked = toggleEl.checked;
            secEl.style.display = checked ? '' : 'none';
            localStorage.setItem(key, checked ? '1' : '0');
        });
    });
}

function normalizeUrl(url) {
    try {
        const u = new URL(url);
        return (u.origin + u.pathname).replace(/\/$/, '');
    } catch (_) {
        return url;
    }
}

function renderFavorites(favorites) {
    const grid = document.getElementById('favorites-grid');
    if (!grid) return;

    // Static entries carry the brand SVG icons; reuse them when the URL matches.
    if (!grid.dataset.templates) {
        const templates = {};
        grid.querySelectorAll('a.favorite-item').forEach((a) => {
            const icon = a.querySelector('.squircle-icon');
            if (icon) templates[normalizeUrl(a.getAttribute('href'))] = icon.outerHTML;
        });
        window.__favoriteIcons = templates;
        grid.dataset.templates = '1';
    }

    grid.textContent = '';
    for (const fav of favorites) {
        const a = document.createElement('a');
        a.className = 'favorite-item';
        a.href = fav.url;

        const known = window.__favoriteIcons ? window.__favoriteIcons[normalizeUrl(fav.url)] : null;
        if (known) {
            a.innerHTML = known;
        } else {
            const icon = document.createElement('div');
            icon.className = 'squircle-icon';
            icon.textContent = (fav.title || '?').trim().charAt(0).toUpperCase();
            icon.style.fontSize = '26px';
            icon.style.fontWeight = '600';
            a.appendChild(icon);
        }

        const label = document.createElement('span');
        label.className = 'favorite-label';
        label.textContent = fav.title;
        a.appendChild(label);
        grid.appendChild(a);
    }

    const addBtn = document.createElement('button');
    addBtn.className = 'favorite-item add-favorite-btn';
    addBtn.type = 'button';
    addBtn.id = 'add-favorite-btn';
    addBtn.title = tr('startpage.add_favorite', 'Add');
    addBtn.innerHTML = `
        <div class="squircle-icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                <line x1="12" y1="5" x2="12" y2="19"></line>
                <line x1="5" y1="12" x2="19" y2="12"></line>
            </svg>
        </div>
        <span class="favorite-label">${tr('startpage.add_favorite', 'Add')}</span>
    `;
    addBtn.addEventListener('click', (e) => {
        e.preventDefault();
        openAddFavoriteModal();
    });
    grid.appendChild(addBtn);
}

let startpageI18n = (typeof window !== 'undefined' && window.__TAKEWEB_I18N__) || null;
if (!startpageI18n) {
    fetch('take://locales/current.json')
        .then(r => r.json())
        .then(data => { startpageI18n = data; })
        .catch(() => {});
}

function tr(key, fallback) {
    if (!startpageI18n) return fallback;
    const parts = key.split('.');
    let cur = startpageI18n;
    for (const p of parts) {
        if (!cur || typeof cur !== 'object') return fallback;
        cur = cur[p];
    }
    return typeof cur === 'string' ? cur : fallback;
}

// Callback when Rust backend sends updated privacy and favorites data
window.updateStartPageData = function(data) {
    if (!data) return;

    if (data.tracker_count !== undefined) {
        const countEl = document.getElementById('tracker-count');
        const textEl = document.getElementById('privacy-text');
        const total = data.tracker_count;
        if (countEl) countEl.textContent = total;
        if (textEl) {
            if (total > 0) {
                const tmpl = tr('startpage.privacy_blocked_some', 'Take Web prevented surveillance ({count} network trackers in 7 days). Shields active.');
                textEl.textContent = tmpl.replace('{count}', total);
            } else {
                textEl.textContent = tr('startpage.privacy_blocked_none', 'No trackers blocked yet. All shields are active.');
            }
        }
    }

    if (Array.isArray(data.favorites)) {
        renderFavorites(data.favorites);
    }

    const card = document.getElementById('reading-card');
    if (data.reading_item) {
        const titleEl = document.getElementById('reading-title');
        const previewEl = document.getElementById('reading-preview');
        const domainEl = document.getElementById('reading-domain');
        if (titleEl) titleEl.textContent = data.reading_item.title;
        if (previewEl) previewEl.textContent = data.reading_item.preview_text || '';
        if (domainEl) domainEl.textContent = data.reading_item.domain;
        if (card && data.reading_item.url) {
            card.style.cursor = 'pointer';
            card.onclick = () => { window.location.href = data.reading_item.url; };
        }
    } else if (card) {
        card.onclick = null;
        card.style.cursor = '';
        document.getElementById('reading-title').textContent = tr('startpage.reading_list_empty', 'Reading list is empty');
        document.getElementById('reading-preview').textContent = tr('startpage.reading_list_empty_hint', 'Add pages via Ctrl+Shift+D.');
        document.getElementById('reading-domain').textContent = '';
    }
};
