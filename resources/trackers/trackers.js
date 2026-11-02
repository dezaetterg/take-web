// Echo Browser Privacy & Trackers Dashboard Controller

let currentData = {
    total_blocked_7d: 0,
    total_blocked_all: 0,
    unique_trackers: 0,
    top_trackers: [],
    all_stats: [],
    fingerprint_stats: {},
    fingerprint_total: 0
};

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
            window.location.href = 'echo://action/clear-trackers';
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
        if (totalKb < 1024) {
            kpiSaved.textContent = `${totalKb} КБ`;
        } else if (totalKb < 1024 * 1024) {
            kpiSaved.textContent = `${(totalKb / 1024).toFixed(1)} МБ`;
        } else {
            kpiSaved.textContent = `${(totalKb / (1024 * 1024)).toFixed(2)} ГБ`;
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

function renderTopTrackers(topTrackers, totalReference) {
    const container = document.getElementById('top-trackers-container');
    if (!container) return;

    if (!topTrackers || topTrackers.length === 0) {
        container.innerHTML = '<div class="empty-hint">Данные собираются во время веб-серфинга. Переходите по сайтам, и заблокированные трекеры отобразятся здесь.</div>';
        return;
    }

    container.innerHTML = '';
    const maxCount = topTrackers[0].count || 1;

    for (const item of topTrackers) {
        const percent = Math.min(100, Math.round((item.count / maxCount) * 100));
        const category = categorizeDomain(item.domain);

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
                Заблокировано: <strong>${item.count.toLocaleString()}</strong> раз
            </div>
        `;
        container.appendChild(card);
    }
}

function renderTable(allStats, filterText) {
    const tbody = document.getElementById('trackers-table-body');
    if (!tbody) return;

    if (!allStats || allStats.length === 0) {
        tbody.innerHTML = '<tr><td colspan="4" class="table-empty">Журнал трекеров пуст. Вы в полной безопасности.</td></tr>';
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
        tbody.innerHTML = '<tr><td colspan="4" class="table-empty">Ничего не найдено по вашему запросу.</td></tr>';
        return;
    }

    tbody.innerHTML = '';
    for (const item of filtered) {
        const tr = document.createElement('tr');
        const formattedDate = formatTimestamp(item.last_blocked_at);

        tr.innerHTML = `
            <td class="table-domain">${escapeHtml(item.domain)}</td>
            <td><span class="table-rule">${escapeHtml(item.rule || 'url-filter')}</span></td>
            <td><strong>${(item.blocked_count || 1).toLocaleString()}</strong></td>
            <td style="color: var(--text-secondary);">${formattedDate}</td>
        `;
        tbody.appendChild(tr);
    }
}

function categorizeDomain(domain) {
    if (!domain) return 'Слежка';
    const d = domain.toLowerCase();
    if (d.includes('analytics') || d.includes('metrika') || d.includes('clarity') || d.includes('hotjar') || d.includes('mixpanel') || d.includes('amplitude') || d.includes('segment')) {
        return 'Аналитика';
    }
    if (d.includes('doubleclick') || d.includes('adservice') || d.includes('criteo') || d.includes('adnxs') || d.includes('taboola') || d.includes('outbrain') || d.includes('adroll') || d.includes('adsystem')) {
        return 'Рекламная сеть';
    }
    if (d.includes('facebook') || d.includes('meta') || d.includes('twitter') || d.includes('yandex') || d.includes('google')) {
        return 'Профилирование';
    }
    return 'Трекер';
}

function formatTimestamp(timestampSec) {
    if (!timestampSec) return 'Недавно';
    const date = new Date(timestampSec * 1000);
    const now = new Date();
    const diffHours = (now - date) / (1000 * 60 * 60);

    const pad = (n) => String(n).padStart(2, '0');
    const timeStr = `${pad(date.getHours())}:${pad(date.getMinutes())}`;

    if (diffHours < 24 && date.getDate() === now.getDate()) {
        return `Сегодня в ${timeStr}`;
    }
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (date.getDate() === yesterday.getDate() && date.getMonth() === yesterday.getMonth()) {
        return `Вчера в ${timeStr}`;
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
    a.download = `echo_privacy_report_${timestamp}.json`;
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
