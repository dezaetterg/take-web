// settings.js - Take Web Browser Settings Page Controller

(function () {
    'use strict';

    const TAB_TITLES = {
        general: 'Основные настройки',
        privacy: 'Приватность и Защита',
        appearance: 'Внешний вид',
        downloads: 'Файлы и Загрузки',
        about: 'О браузере'
    };

    const navItems = document.querySelectorAll('.nav-item');
    const sections = document.querySelectorAll('.settings-section');
    const sectionTitle = document.getElementById('section-title');
    const searchInput = document.getElementById('settings-search');
    const toast = document.getElementById('settings-toast');
    let toastTimeout = null;

    const SETTINGS_KEY = 'echo_browser_settings';

    function showToast(message) {
        if (!toast) return;
        toast.textContent = message;
        toast.classList.add('show');
        if (toastTimeout) {
            clearTimeout(toastTimeout);
        }
        toastTimeout = setTimeout(() => {
            toast.classList.remove('show');
        }, 2500);
    }

    function notifySettingChanged(key, value) {
        try {
            const token = window.__ECHO_IPC_TOKEN__ || '';
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.settingsChange) {
                window.webkit.messageHandlers.settingsChange.postMessage(JSON.stringify({
                    token: token,
                    key: key,
                    value: String(value)
                }));
            }
        } catch (e) {
            console.warn('IPC settingsChange failed:', e);
        }
    }

    function switchTab(tabId) {
        navItems.forEach(item => {
            item.classList.toggle('active', item.dataset.tab === tabId);
        });

        sections.forEach(sec => {
            sec.classList.toggle('active', sec.id === `${tabId}-section`);
        });

        if (sectionTitle && TAB_TITLES[tabId]) {
            sectionTitle.textContent = TAB_TITLES[tabId];
        }

        // Close any open dropdowns when switching tabs
        closeAllSelects();

        // Clear search on tab change
        if (searchInput && searchInput.value) {
            searchInput.value = '';
            filterSettings('');
        }

        try {
            window.location.hash = tabId;
        } catch (_) {}
    }

    // Attach tab click events
    navItems.forEach(item => {
        item.addEventListener('click', () => {
            const tabId = item.dataset.tab;
            if (tabId) {
                switchTab(tabId);
            }
        });
    });

    // Check initial hash
    const initialHash = (window.location.hash || '').replace('#', '');
    if (initialHash && TAB_TITLES[initialHash]) {
        switchTab(initialHash);
    }

    // -------------------------------------------------------------------------
    // Custom Select Component Logic (Zero native popups, 100% theme adaptive)
    // -------------------------------------------------------------------------

    function closeAllSelects() {
        document.querySelectorAll('.custom-select.open').forEach(el => {
            el.classList.remove('open');
            const trigger = el.querySelector('.custom-select-trigger');
            if (trigger) trigger.setAttribute('aria-expanded', 'false');
            const card = el.closest('.settings-card');
            if (card) {
                card.classList.remove('has-open-select');
                card.style.zIndex = '';
            }
            const row = el.closest('.setting-row');
            if (row) {
                row.classList.remove('has-open-select');
                row.style.zIndex = '';
            }
        });
    }

    function openSelect(selectEl, trigger) {
        closeAllSelects();
        selectEl.classList.add('open');
        if (trigger) trigger.setAttribute('aria-expanded', 'true');
        const card = selectEl.closest('.settings-card');
        if (card) {
            card.classList.add('has-open-select');
            card.style.zIndex = '500';
        }
        const row = selectEl.closest('.setting-row');
        if (row) {
            row.classList.add('has-open-select');
            row.style.zIndex = '500';
        }
    }

    function updateTriggerDisplay(selectEl, optionEl) {
        if (!selectEl || !optionEl) return;
        const trigger = selectEl.querySelector('.custom-select-trigger');
        const display = trigger ? trigger.querySelector('.select-selected-content') : null;
        if (!display) return;

        let icon = null;
        // User requested removing search engine icons completely
        if (selectEl.id !== 'select-default-search-engine') {
            icon = optionEl.querySelector('.theme-icon') ||
                   optionEl.querySelector('.startup-icon') ||
                   optionEl.querySelector('.dns-icon') ||
                   optionEl.querySelector('.cookie-icon');
        }
        const nameEl = optionEl.querySelector('.option-name') || optionEl.querySelector('.option-title');
        const text = nameEl ? nameEl.textContent.trim() : optionEl.textContent.trim();

        display.innerHTML = '';
        if (icon) {
            display.appendChild(icon.cloneNode(true));
        }
        const textSpan = document.createElement('span');
        textSpan.className = 'selected-text';
        textSpan.textContent = text;
        display.appendChild(textSpan);
    }

    function initCustomSelects() {
        const selects = document.querySelectorAll('.custom-select');

        selects.forEach(selectEl => {
            const settingKey = selectEl.dataset.settingKey;
            const trigger = selectEl.querySelector('.custom-select-trigger');
            const menu = selectEl.querySelector('.custom-select-menu');
            const options = selectEl.querySelectorAll('.custom-select-option');

            // 1. Determine initial value
            let savedVal = null;
            try {
                savedVal = localStorage.getItem(settingKey);
                if (!savedVal) {
                    const rawAll = localStorage.getItem(SETTINGS_KEY);
                    if (rawAll) {
                        const parsed = JSON.parse(rawAll);
                        savedVal = parsed[settingKey] || parsed[selectEl.id];
                    }
                }
            } catch (_) {}

            if (!savedVal) {
                savedVal = selectEl.dataset.value || (options[0] ? options[0].dataset.value : '');
            }

            // Sync initial state
            selectEl.dataset.value = savedVal;
            let activeOption = null;
            options.forEach(opt => {
                if (opt.dataset.value === savedVal) {
                    opt.classList.add('selected');
                    activeOption = opt;
                } else {
                    opt.classList.remove('selected');
                }
            });

            if (!activeOption && options.length > 0) {
                activeOption = options[0];
                activeOption.classList.add('selected');
                selectEl.dataset.value = activeOption.dataset.value;
            }

            updateTriggerDisplay(selectEl, activeOption);

            // 2. Trigger click handler
            if (trigger) {
                trigger.addEventListener('click', (e) => {
                    e.stopPropagation();
                    const isOpen = selectEl.classList.contains('open');
                    if (isOpen) {
                        closeAllSelects();
                    } else {
                        openSelect(selectEl, trigger);
                    }
                });

                trigger.addEventListener('keydown', (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        const isOpen = selectEl.classList.contains('open');
                        if (isOpen) {
                            closeAllSelects();
                        } else {
                            openSelect(selectEl, trigger);
                        }
                    } else if (e.key === 'Escape') {
                        closeAllSelects();
                    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                        e.preventDefault();
                        if (!selectEl.classList.contains('open')) {
                            openSelect(selectEl, trigger);
                        }
                    }
                });
            }

            // 3. Option click handler
            options.forEach(opt => {
                opt.addEventListener('click', (e) => {
                    e.stopPropagation();
                    const val = opt.dataset.value;
                    selectEl.dataset.value = val;

                    options.forEach(o => o.classList.remove('selected'));
                    opt.classList.add('selected');
                    updateTriggerDisplay(selectEl, opt);
                    closeAllSelects();

                    // Persist
                    try {
                        localStorage.setItem(settingKey, val);
                        let settingsMap = {};
                        const raw = localStorage.getItem(SETTINGS_KEY);
                        if (raw) settingsMap = JSON.parse(raw);
                        settingsMap[settingKey] = val;
                        settingsMap[selectEl.id] = val;
                        localStorage.setItem(SETTINGS_KEY, JSON.stringify(settingsMap));
                    } catch (err) {
                        console.error('Failed to save custom select setting:', err);
                    }

                    // Send IPC to Rust
                    notifySettingChanged(settingKey, val);

                    // Special handlers
                    if (settingKey === 'theme') {
                        handleThemeChange(val);
                    }
                    if (settingKey === 'cookie_policy' && !isApplyingPrivacyPreset) {
                        checkCurrentPrivacyMatch();
                    }

                    const nameEl = opt.querySelector('.option-name') || opt.querySelector('.option-title');
                    const label = nameEl ? nameEl.textContent.trim() : val;
                    showToast(`Выбрано: ${label}`);
                });
            });
        });
    }

    function setCustomSelectValue(selectElOrId, val) {
        const selectEl = typeof selectElOrId === 'string' ? document.getElementById(selectElOrId) : selectElOrId;
        if (!selectEl) return;
        const strVal = String(val);
        selectEl.dataset.value = strVal;
        let activeOption = null;
        const options = selectEl.querySelectorAll('.custom-select-option');
        options.forEach(opt => {
            if (opt.dataset.value === strVal) {
                opt.classList.add('selected');
                activeOption = opt;
            } else {
                opt.classList.remove('selected');
            }
        });
        if (activeOption) {
            updateTriggerDisplay(selectEl, activeOption);
        }
    }

    // Dismiss custom dropdowns whenever window loses focus (Alt-Tab, window change) or clicks outside
    window.addEventListener('blur', closeAllSelects);
    document.addEventListener('visibilitychange', () => {
        if (document.hidden) closeAllSelects();
    });
    document.addEventListener('click', (e) => {
        if (!e.target.closest('.custom-select')) {
            closeAllSelects();
        }
    });
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            closeAllSelects();
        }
    });

    // -------------------------------------------------------------------------
    // Theme Management
    // -------------------------------------------------------------------------

    function handleThemeChange(val) {
        if (val === 'light') {
            document.documentElement.setAttribute('data-theme', 'light');
            document.body.classList.add('light-mode');
        } else if (val === 'dark') {
            document.documentElement.setAttribute('data-theme', 'dark');
            document.body.classList.remove('light-mode');
        } else {
            // System: determine from matchMedia
            document.documentElement.removeAttribute('data-theme');
            const isDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
            if (isDark) {
                document.body.classList.remove('light-mode');
            } else {
                document.body.classList.add('light-mode');
            }
        }
    }

    function initTheme() {
        const savedTheme = localStorage.getItem('theme') || 'system';
        handleThemeChange(savedTheme);
    }

    // -------------------------------------------------------------------------
    // Privacy Level Stepped Slider Controller
    // -------------------------------------------------------------------------

    let isApplyingPrivacyPreset = false;

    const PRIVACY_PRESETS = {
        1: {
            id: 1,
            name: 'Базовый',
            badgeClass: 'level-1',
            desc: 'Минимальная фильтрация. Максимальная совместимость со всеми сайтами.',
            settings: {
                'shield-adblock': true,
                'shield-https-only': false,
                'shield-fingerprint': false,
                'shield-itp': false,
                'select-cookie-policy': 'always',
                'block-autoplay': false,
                'webrtc-protection': false,
                'enable-javascript': true
            }
        },
        2: {
            id: 2,
            name: 'Баланс',
            badgeClass: 'level-2',
            desc: 'Оптимальный баланс защиты от трекеров без нарушения работы сайтов.',
            settings: {
                'shield-adblock': true,
                'shield-https-only': true,
                'shield-fingerprint': true,
                'shield-itp': true,
                'select-cookie-policy': 'no_third_party',
                'block-autoplay': false,
                'webrtc-protection': false,
                'enable-javascript': true
            }
        },
        3: {
            id: 3,
            name: 'Строгий',
            badgeClass: 'level-3',
            desc: 'Усиленная защита: изоляция WebRTC и блокировка автовоспроизведения.',
            settings: {
                'shield-adblock': true,
                'shield-https-only': true,
                'shield-fingerprint': true,
                'shield-itp': true,
                'select-cookie-policy': 'no_third_party',
                'block-autoplay': true,
                'webrtc-protection': true,
                'enable-javascript': true
            }
        },
        4: {
            id: 4,
            name: 'Максимум',
            badgeClass: 'level-4',
            desc: 'Максимальная анонимность: блокировка всех cookie и отключение JavaScript.',
            settings: {
                'shield-adblock': true,
                'shield-https-only': true,
                'shield-fingerprint': true,
                'shield-itp': true,
                'select-cookie-policy': 'never',
                'block-autoplay': true,
                'webrtc-protection': true,
                'enable-javascript': false
            }
        }
    };

    function updatePrivacySliderUI(level, isCustom = false) {
        const badgeText = document.getElementById('privacy-level-badge-text');
        const descEl = document.getElementById('privacy-level-desc');
        const stopLabels = document.querySelectorAll('.slider-stop-label');

        const preset = PRIVACY_PRESETS[level] || PRIVACY_PRESETS[2];

        if (badgeText) {
            badgeText.className = 'privacy-level-pill';
            if (isCustom) {
                badgeText.classList.add('level-custom');
                badgeText.textContent = 'Настраиваемый';
            } else {
                badgeText.classList.add(preset.badgeClass);
                badgeText.textContent = preset.name;
            }
        }

        if (descEl) {
            if (isCustom) {
                descEl.textContent = 'Индивидуальная конфигурация параметров защиты.';
            } else {
                descEl.textContent = preset.desc;
            }
        }

        stopLabels.forEach(lbl => {
            const lblLevel = parseInt(lbl.dataset.level, 10);
            lbl.classList.toggle('active', !isCustom && lblLevel === level);
        });
    }

    function applyPrivacyPreset(level, showToastMsg = true) {
        const preset = PRIVACY_PRESETS[level];
        if (!preset) return;

        isApplyingPrivacyPreset = true;

        const slider = document.getElementById('privacy-level-slider');
        if (slider) {
            slider.value = level;
        }

        // Apply all preset settings
        for (const [id, value] of Object.entries(preset.settings)) {
            if (id === 'select-cookie-policy') {
                const selectEl = document.getElementById(id);
                if (selectEl) {
                    setCustomSelectValue(selectEl, value);
                    try {
                        localStorage.setItem('cookie_policy', value);
                        let settingsMap = {};
                        const raw = localStorage.getItem(SETTINGS_KEY);
                        if (raw) settingsMap = JSON.parse(raw);
                        settingsMap['cookie_policy'] = value;
                        settingsMap[id] = value;
                        localStorage.setItem(SETTINGS_KEY, JSON.stringify(settingsMap));
                    } catch (_) {}
                    notifySettingChanged('cookie_policy', value);
                }
            } else {
                const cb = document.getElementById(id);
                if (cb) {
                    cb.checked = value;
                    try {
                        localStorage.setItem(cb.id, value ? 'true' : 'false');
                        let settingsMap = {};
                        const raw = localStorage.getItem(SETTINGS_KEY);
                        if (raw) settingsMap = JSON.parse(raw);
                        settingsMap[cb.id] = value;
                        localStorage.setItem(SETTINGS_KEY, JSON.stringify(settingsMap));
                    } catch (_) {}
                    const ipcKey = cb.id.replace(/-/g, '_');
                    notifySettingChanged(ipcKey, value ? 'true' : 'false');
                }
            }
        }

        // Save privacy_level
        try {
            localStorage.setItem('privacy_level', String(level));
            let settingsMap = {};
            const raw = localStorage.getItem(SETTINGS_KEY);
            if (raw) settingsMap = JSON.parse(raw);
            settingsMap['privacy_level'] = String(level);
            localStorage.setItem(SETTINGS_KEY, JSON.stringify(settingsMap));
        } catch (_) {}
        notifySettingChanged('privacy_level', String(level));

        updatePrivacySliderUI(level, false);

        if (showToastMsg) {
            showToast(`Уровень приватности: ${preset.name}`);
        }

        isApplyingPrivacyPreset = false;
    }

    function checkCurrentPrivacyMatch() {
        if (isApplyingPrivacyPreset) return;

        const current = {
            'shield-adblock': !!document.getElementById('shield-adblock')?.checked,
            'shield-https-only': !!document.getElementById('shield-https-only')?.checked,
            'shield-fingerprint': !!document.getElementById('shield-fingerprint')?.checked,
            'shield-itp': !!document.getElementById('shield-itp')?.checked,
            'select-cookie-policy': document.getElementById('select-cookie-policy')?.dataset.value || 'no_third_party',
            'block-autoplay': !!document.getElementById('block-autoplay')?.checked,
            'webrtc-protection': !!document.getElementById('webrtc-protection')?.checked,
            'enable-javascript': !!document.getElementById('enable-javascript')?.checked
        };

        let matchedLevel = null;
        for (let lvl = 1; lvl <= 4; lvl++) {
            const p = PRIVACY_PRESETS[lvl].settings;
            let match = true;
            for (const key of Object.keys(p)) {
                if (current[key] !== p[key]) {
                    match = false;
                    break;
                }
            }
            if (match) {
                matchedLevel = lvl;
                break;
            }
        }

        const slider = document.getElementById('privacy-level-slider');
        if (matchedLevel !== null) {
            if (slider) slider.value = matchedLevel;
            try {
                localStorage.setItem('privacy_level', String(matchedLevel));
            } catch (_) {}
            notifySettingChanged('privacy_level', String(matchedLevel));
            updatePrivacySliderUI(matchedLevel, false);
        } else {
            let score = 0;
            if (current['shield-adblock']) score++;
            if (current['shield-https-only']) score++;
            if (current['shield-fingerprint']) score++;
            if (current['shield-itp']) score++;
            if (current['select-cookie-policy'] === 'never') score += 2;
            else if (current['select-cookie-policy'] === 'no_third_party') score += 1;
            if (current['block-autoplay']) score++;
            if (current['webrtc-protection']) score++;
            if (!current['enable-javascript']) score += 2;

            let closest = 2;
            if (score <= 2) closest = 1;
            else if (score <= 5) closest = 2;
            else if (score <= 7) closest = 3;
            else closest = 4;

            if (slider) slider.value = closest;
            try {
                localStorage.setItem('privacy_level', 'custom');
            } catch (_) {}
            notifySettingChanged('privacy_level', 'custom');
            updatePrivacySliderUI(closest, true);
        }
    }

    function initPrivacySlider() {
        const slider = document.getElementById('privacy-level-slider');
        const stopLabels = document.querySelectorAll('.slider-stop-label');

        if (slider) {
            slider.addEventListener('input', (e) => {
                const lvl = parseInt(e.target.value, 10);
                updatePrivacySliderUI(lvl, false);
            });

            slider.addEventListener('change', (e) => {
                const lvl = parseInt(e.target.value, 10);
                applyPrivacyPreset(lvl, true);
            });
        }

        stopLabels.forEach(lbl => {
            lbl.addEventListener('click', () => {
                const lvl = parseInt(lbl.dataset.level, 10);
                if (lvl >= 1 && lvl <= 4) {
                    applyPrivacyPreset(lvl, true);
                }
            });
        });

        let savedLevel = null;
        try {
            savedLevel = localStorage.getItem('privacy_level');
        } catch (_) {}

        if (savedLevel && savedLevel !== 'custom') {
            const lvl = parseInt(savedLevel, 10);
            if (lvl >= 1 && lvl <= 4) {
                if (slider) slider.value = lvl;
                updatePrivacySliderUI(lvl, false);
                return;
            }
        }
        checkCurrentPrivacyMatch();
    }

    // -------------------------------------------------------------------------
    // Settings Persistence (Checkboxes & inputs)
    // -------------------------------------------------------------------------

    function loadOtherSettings() {
        try {
            const raw = localStorage.getItem(SETTINGS_KEY);
            const data = raw ? JSON.parse(raw) : {};

            document.querySelectorAll('input[type="checkbox"]').forEach(cb => {
                if (cb.id) {
                    const val = localStorage.getItem(cb.id);
                    if (val === 'true') {
                        cb.checked = true;
                    } else if (val === 'false') {
                        cb.checked = false;
                    } else if (typeof data[cb.id] === 'boolean') {
                        cb.checked = data[cb.id];
                    }
                }
            });
        } catch (e) {
            console.error('Failed to load other settings:', e);
        }
    }

    document.querySelectorAll('input[type="checkbox"]').forEach(cb => {
        cb.addEventListener('change', () => {
            if (isApplyingPrivacyPreset) return;
            const row = cb.closest('.setting-row');
            const nameEl = row ? row.querySelector('.setting-name') : null;
            const settingName = nameEl ? nameEl.textContent.trim() : cb.id;

            try {
                localStorage.setItem(cb.id, cb.checked ? 'true' : 'false');
                let settingsMap = {};
                const raw = localStorage.getItem(SETTINGS_KEY);
                if (raw) settingsMap = JSON.parse(raw);
                settingsMap[cb.id] = cb.checked;
                localStorage.setItem(SETTINGS_KEY, JSON.stringify(settingsMap));
            } catch (_) {}

            const ipcKey = cb.id.replace(/-/g, '_');
            notifySettingChanged(ipcKey, cb.checked ? 'true' : 'false');
            showToast(`${settingName}: ${cb.checked ? 'Включено' : 'Выключено'}`);

            const privacyCheckboxIds = [
                'shield-adblock', 'shield-https-only', 'shield-fingerprint',
                'shield-itp', 'block-autoplay', 'webrtc-protection', 'enable-javascript'
            ];
            if (privacyCheckboxIds.includes(cb.id)) {
                checkCurrentPrivacyMatch();
            }
        });
    });

    // Clear history button handler
    const clearHistoryBtn = document.getElementById('clear-history-btn');
    if (clearHistoryBtn) {
        clearHistoryBtn.addEventListener('click', () => {
            if (confirm('Вы уверены, что хотите удалить всю историю посещений?')) {
                notifySettingChanged('clear_history', 'true');
                try {
                    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.clearHistory) {
                        window.webkit.messageHandlers.clearHistory.postMessage({});
                    }
                } catch (_) {}
                showToast('История посещений успешно очищена');
            }
        });
    }

    // Clear browser cache button handler
    const clearCacheBtn = document.getElementById('clear-cache-btn');
    if (clearCacheBtn) {
        clearCacheBtn.addEventListener('click', () => {
            notifySettingChanged('clear_cache', 'true');
            showToast('Кэш браузера успешно очищен');
        });
    }

    // Clear tracker stats button handler
    const clearTrackersBtn = document.getElementById('clear-trackers-btn');
    if (clearTrackersBtn) {
        clearTrackersBtn.addEventListener('click', () => {
            notifySettingChanged('clear_tracker_stats', 'true');
            showToast('Статистика заблокированных трекеров сброшена');
        });
    }

    // Clear site permissions button handler
    const clearPermissionsBtn = document.getElementById('clear-permissions-btn');
    if (clearPermissionsBtn) {
        clearPermissionsBtn.addEventListener('click', () => {
            if (confirm('Сбросить все сохраненные разрешения для сайтов?')) {
                notifySettingChanged('clear_site_permissions', 'true');
                showToast('Разрешения сайтов успешно сброшены');
            }
        });
    }

    // Change download directory button handler
    const chooseDownloadBtn = document.getElementById('choose-download-dir');
    const downloadDesc = document.getElementById('download-dir-desc');
    if (downloadDesc) {
        const savedDir = localStorage.getItem('download_dir');
        if (savedDir) downloadDesc.textContent = savedDir;
    }

    if (chooseDownloadBtn && downloadDesc) {
        chooseDownloadBtn.addEventListener('click', () => {
            const newPath = prompt('Укажите путь для сохранения загрузок:', downloadDesc.textContent);
            if (newPath && newPath.trim()) {
                const path = newPath.trim();
                downloadDesc.textContent = path;
                localStorage.setItem('download_dir', path);
                notifySettingChanged('download_dir', path);
                showToast(`Папка загрузок: ${path}`);
            }
        });
    }

    // Filter settings search
    function filterSettings(query) {
        const q = (query || '').toLowerCase().trim();
        const allCards = document.querySelectorAll('.settings-card');

        if (!q) {
            document.querySelectorAll('.setting-row').forEach(row => {
                row.style.display = '';
            });
            allCards.forEach(card => {
                card.style.display = '';
            });
            const activeTab = document.querySelector('.nav-item.active');
            const tabId = activeTab ? activeTab.dataset.tab : 'general';
            sections.forEach(sec => {
                sec.classList.toggle('active', sec.id === `${tabId}-section`);
            });
            return;
        }

        sections.forEach(sec => sec.classList.add('active'));

        allCards.forEach(card => {
            let hasVisibleRow = false;
            const rows = card.querySelectorAll('.setting-row');
            rows.forEach(row => {
                const name = (row.querySelector('.setting-name')?.textContent || '').toLowerCase();
                const desc = (row.querySelector('.setting-desc')?.textContent || '').toLowerCase();
                // Also search through custom select options and slider stop labels
                const selectOptions = Array.from(row.querySelectorAll('.custom-select-option'))
                    .map(o => o.textContent.toLowerCase())
                    .join(' ');
                const sliderStops = Array.from(row.querySelectorAll('.slider-stop-label'))
                    .map(o => o.textContent.toLowerCase())
                    .join(' ');
                const match = name.includes(q) || desc.includes(q) || selectOptions.includes(q) || sliderStops.includes(q);
                row.style.display = match ? '' : 'none';
                if (match) hasVisibleRow = true;
            });

            if (card.classList.contains('about-card')) {
                const text = card.textContent.toLowerCase();
                card.style.display = text.includes(q) ? '' : 'none';
            } else {
                card.style.display = hasVisibleRow ? '' : 'none';
            }
        });
    }

    if (searchInput) {
        searchInput.addEventListener('input', (e) => {
            filterSettings(e.target.value);
        });
    }

    window.updateSettingsPageData = function(data) {
        if (!data || typeof data !== 'object') return;

        if (data.ipc_token) {
            window.__ECHO_IPC_TOKEN__ = data.ipc_token;
        }

        // Custom selects
        document.querySelectorAll('.custom-select').forEach(selectEl => {
            const key = selectEl.dataset.settingKey;
            let val = data[key];
            if (val === undefined && selectEl.id) {
                val = data[selectEl.id];
            }
            if (val !== undefined && val !== null) {
                const strVal = String(val);
                selectEl.dataset.value = strVal;
                let activeOption = null;
                const options = selectEl.querySelectorAll('.custom-select-option');
                options.forEach(opt => {
                    if (opt.dataset.value === strVal) {
                        opt.classList.add('selected');
                        activeOption = opt;
                    } else {
                        opt.classList.remove('selected');
                    }
                });
                if (activeOption) {
                    updateTriggerDisplay(selectEl, activeOption);
                }
                if (key === 'theme') {
                    handleThemeChange(strVal);
                }
            }
        });

        // Checkboxes
        document.querySelectorAll('input[type="checkbox"]').forEach(cb => {
            if (!cb.id) return;
            const ipcKey = cb.id.replace(/-/g, '_');
            let val = data[ipcKey];
            if (val === undefined) {
                val = data[cb.id];
            }
            if (val !== undefined && val !== null) {
                cb.checked = (val === 'true' || val === true || val === '1');
            }
        });

        // Download directory description
        const downloadDesc = document.getElementById('download-dir-desc');
        if (downloadDesc && data['download_dir']) {
            downloadDesc.textContent = data['download_dir'];
        }

        // Privacy Level Slider
        if (data.privacy_level !== undefined && data.privacy_level !== null) {
            if (data.privacy_level === 'custom') {
                checkCurrentPrivacyMatch();
            } else {
                const lvl = parseInt(data.privacy_level, 10);
                if (lvl >= 1 && lvl <= 4) {
                    const slider = document.getElementById('privacy-level-slider');
                    if (slider) slider.value = lvl;
                    updatePrivacySliderUI(lvl, false);
                } else {
                    checkCurrentPrivacyMatch();
                }
            }
        } else {
            checkCurrentPrivacyMatch();
        }
    };

    // Initialize all components
    initTheme();
    initCustomSelects();
    loadOtherSettings();
    initPrivacySlider();
})();
