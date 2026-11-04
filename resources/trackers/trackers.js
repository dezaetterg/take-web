// Take Web Privacy & Trackers Dashboard Controller

let currentData = {
    total_blocked_7d: 0,
    total_blocked_all: 0,
    unique_trackers: 0,
    top_trackers: [],
    all_stats: [],
    fingerprint_stats: {},
    fingerprint_total: 0
};

let trackersI18n = null;
fetch('take://locales/current.json')
    .then(r => r.json())
    .then(data => { trackersI18n = data; })
    .catch(() => {});

function tr(key, fallback) {
    if (!trackersI18n) return fallback;
    const parts = key.split('.');
    let cur = trackersI18n;
    for (const p of parts) {
        if (!cur || typeof cur !== 'object') return fallback;
        cur = cur[p];
    }
    return typeof cur === 'string' ? cur : fallback;
}

document.addEventListener('DOMContentLoaded', () => {
    setupEventListeners();
});

function setupEventListeners() {
    const filterInput = document.getElementById('filter-input');
    if (filterInput) {
        filterInput.addEventListener('input', () => {
            renderTable(currentData.all_stats, filterInput.value.trim().toLowerCase());
        });
    }

    const exportBtn = document.getElementById('export-json-btn');
    if (exportBtn) {
        exportBtn.addEventListener('click', exportDataAsJson);
    }

    const clearBtn = document.getElementById('clear-stats-btn');
    const modal = document.getElementById('confirm-modal');
    const cancelBtn = document.getElementById('modal-cancel-btn');
    const confirmBtn = document.getElementById('modal-confirm-btn');

    if (clearBtn && modal) {
        clearBtn.addEventListener('click', () => {
            modal.classList.add('open');
        });
    }

    if (cancelBtn && modal) {
        cancelBtn.addEventListener('click', () => {
            modal.classList.remove('open');
        });
    }

    if (confirmBtn && modal) {
        confirmBtn.addEventListener('click', () => {
            modal.classList.remove('open');
            // Trigger internal action intercepted by WebKit decide_policy
            window.location.href = 'take://action/clear-trackers';
        });
    }

    // Close modal on click outside
    if (modal) {
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                modal.classList.remove('open');
            }
        });
    }
}

window.updateTrackersPageData = function(payload) {
    if (!payload) return;
    currentData = payload;

    // 1. Update KPI cards
    const kpi7d = document.getElementById('kpi-7d');
    const kpiAll = document.getElementById('kpi-all');
    const kpiFp = document.getElementById('kpi-fp');
    const kpiSaved = document.getElementById('kpi-saved');

    if (kpi7d) kpi7d.textContent = (payload.total_blocked_7d || 0).toLocaleString();
    if (kpiAll) kpiAll.textContent = (payload.total_blocked_all || 0).toLocaleString();
    if (kpiFp) kpiFp.textContent = (payload.fingerprint_total || 0).toLocaleString();

    if (kpiSaved) {
        // Average tracker / ad request saves ~65 KB of network bandwidth
        const totalKb = (payload.total_blocked_all || 0) * 65;
        const kbUnit = tr('trackers.unit_kb', 'KB');
        const mbUnit = tr('trackers.unit_mb', 'MB');
        const gbUnit = tr('trackers.unit_gb', 'GB');

        if (totalKb < 1024) {
            kpiSaved.textContent = `${totalKb} ${kbUnit}`;
        } else if (totalKb < 1024 * 1024) {
            kpiSaved.textContent = `${(totalKb / 1024).toFixed(1)} ${mbUnit}`;
        } else {
            kpiSaved.textContent = `${(totalKb / (1024 * 1024)).toFixed(2)} ${gbUnit}`;
        }
    }

    // 2. Update Fingerprint Defense Shields Counters
    const fpMap = payload.fingerprint_stats || {};
    const updateShieldCount = (id, val) => {
        const el = document.getElementById(id);
        if (el) el.textContent = (val || 0).toLocaleString();
    };

    updateShieldCount('cnt-canvas', fpMap.canvas);
    updateShieldCount('cnt-webgl', fpMap.webgl);
    updateShieldCount('cnt-audio', fpMap.audio);
    updateShieldCount('cnt-fonts', fpMap.fonts);
    updateShieldCount('cnt-webrtc', fpMap.webrtc);
    updateShieldCount('cnt-timers', fpMap.timers);

    // 3. Toggle Empty State vs Detailed Sections
    const emptyBox = document.getElementById('empty-state-box');
    const detailsContainer = document.getElementById('trackers-details-container');
    const totalTrackers = payload.total_blocked_all || 0;

    if (totalTrackers === 0) {
        if (emptyBox) emptyBox.style.display = 'block';
        if (detailsContainer) detailsContainer.style.display = 'none';
    } else {
        if (emptyBox) emptyBox.style.display = 'none';
        if (detailsContainer) detailsContainer.style.display = 'flex';
        // 4. Render Top Trackers
        renderTopTrackers(payload.top_trackers, payload.total_blocked_7d || payload.total_blocked_all || 1);

        // 5. Render Table
        const filterInput = document.getElementById('filter-input');
        const filterText = filterInput ? filterInput.value.trim().toLowerCase() : '';
        renderTable(payload.all_stats, filterText);
    }
};

function renderTopTrackers(topTrackers, _totalReference) {
    const container = document.getElementById('top-trackers-container');
    if (!container) return;

    if (!topTrackers || topTrackers.length === 0) {
        const hint = tr('trackers.empty_hint', 'Data is collected while browsing the web. Browse sites and blocked trackers will show up here.');
        container.innerHTML = `<div class="empty-hint">${escapeHtml(hint)}</div>`;
        return;
    }

    container.innerHTML = '';
    const maxCount = topTrackers[0].count || 1;

    for (const item of topTrackers) {
        const percent = Math.min(100, Math.round((item.count / maxCount) * 100));
        const category = categorizeDomain(item.domain);
        const countTmpl = tr('trackers.blocked_times', 'Blocked: {count} times');
        const countLabel = countTmpl.replace('{count}', item.count.toLocaleString());

        const card = document.createElement('div');
        card.className = 'top-tracker-card';
        card.innerHTML = `
            <div class="tracker-meta">
                <span class="tracker-domain-title">${escapeHtml(item.domain)}</span>
                <span class="tracker-badge-category">${escapeHtml(category)}</span>
            </div>
            <div class="tracker-stats-bar-container">
                <div class="tracker-stats-bar-fill" style="width: ${percent}%;"></div>
            </div>
            <div class="tracker-count-label">
                ${escapeHtml(countLabel)}
            </div>
        `;
        container.appendChild(card);
    }
}

function renderTable(allStats, filterText) {
    const tbody = document.getElementById('trackers-table-body');
    if (!tbody) return;

    if (!allStats || allStats.length === 0) {
        const emptyMsg = tr('trackers.empty_journal', 'Tracker log is empty. You are completely safe.');
        tbody.innerHTML = `<tr><td colspan="4" class="table-empty">${escapeHtml(emptyMsg)}</td></tr>`;
        return;
    }

    let filtered = allStats;
    if (filterText) {
        filtered = allStats.filter(s => 
            (s.domain && s.domain.toLowerCase().includes(filterText)) ||
            (s.rule && s.rule.toLowerCase().includes(filterText))
        );
    }

    if (filtered.length === 0) {
        const notFoundMsg = tr('trackers.empty_search', 'Nothing found for your search query.');
        tbody.innerHTML = `<tr><td colspan="4" class="table-empty">${escapeHtml(notFoundMsg)}</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    for (const item of filtered) {
        const trEl = document.createElement('tr');
        const formattedDate = formatTimestamp(item.last_blocked_at);

        trEl.innerHTML = `
            <td class="table-domain">${escapeHtml(item.domain)}</td>
            <td><span class="table-rule">${escapeHtml(item.rule || 'url-filter')}</span></td>
            <td><strong>${(item.blocked_count || 1).toLocaleString()}</strong></td>
            <td style="color: var(--text-secondary);">${escapeHtml(formattedDate)}</td>
        `;
        tbody.appendChild(trEl);
    }
}

function categorizeDomain(domain) {
    if (!domain) return tr('trackers.cat_tracking', 'Tracking');
    const d = domain.toLowerCase();
    if (d.includes('analytics') || d.includes('metrika') || d.includes('clarity') || d.includes('hotjar') || d.includes('mixpanel') || d.includes('amplitude') || d.includes('segment')) {
        return tr('trackers.cat_analytics', 'Analytics');
    }
    if (d.includes('doubleclick') || d.includes('adservice') || d.includes('criteo') || d.includes('adnxs') || d.includes('taboola') || d.includes('outbrain') || d.includes('adroll') || d.includes('adsystem')) {
        return tr('trackers.cat_ad_network', 'Ad Network');
    }
    if (d.includes('facebook') || d.includes('meta') || d.includes('twitter') || d.includes('yandex') || d.includes('google')) {
        return tr('trackers.cat_fingerprinting', 'Fingerprinting');
    }
    return tr('trackers.cat_tracking', 'Tracking');
}

function formatTimestamp(timestampSec) {
    if (!timestampSec) return tr('trackers.time_recently', 'Recently');
    const date = new Date(timestampSec * 1000);
    const now = new Date();
    const diffHours = (now - date) / (1000 * 60 * 60);

    const pad = (n) => String(n).padStart(2, '0');
    const timeStr = `${pad(date.getHours())}:${pad(date.getMinutes())}`;

    if (diffHours < 24 && date.getDate() === now.getDate()) {
        const tmpl = tr('trackers.time_today_at', 'Today at {time}');
        return tmpl.replace('{time}', timeStr);
    }
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (date.getDate() === yesterday.getDate() && date.getMonth() === yesterday.getMonth()) {
        const tmpl = tr('trackers.time_yesterday_at', 'Yesterday at {time}');
        return tmpl.replace('{time}', timeStr);
    }

    return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${timeStr}`;
}

function exportDataAsJson() {
    const jsonStr = JSON.stringify(currentData, null, 2);
    const blob = new Blob([jsonStr], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    const timestamp = new Date().toISOString().slice(0, 10);
    a.href = url;
    a.download = `takeweb_privacy_report_${timestamp}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

function escapeHtml(text) {
    if (!text) return '';
    return String(text)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
}
