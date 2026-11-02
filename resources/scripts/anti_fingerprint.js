(function() {
    'use strict';
    // Injected by BrowserEngine at runtime or fallback seed
    const SESSION_SALT = "/*__ECHO_SESSION_SECRET__*/";

    // --- Private WeakSets for Tracking Farbled Objects without Global DOM Leaks ---
    const farbledCanvases = new WeakSet();
    const farbledBuffers = new WeakSet();
    const farbledDestinations = new WeakSet();

    // --- Stealth Helper: Preserves native toString behavior for overridden functions ---
    const overriddenFns = new WeakMap();
    const origToString = Function.prototype.toString;

    function registerNative(fn, name) {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
        } catch (_) {}
        overriddenFns.set(fn, `function ${name}() { [native code] }`);
        return fn;
    }

    Function.prototype.toString = function() {
        if (overriddenFns.has(this)) {
            return overriddenFns.get(this);
        }
        return origToString.apply(this, arguments);
    };
    overriddenFns.set(Function.prototype.toString, 'function toString() { [native code] }');

    function definePropertyStealth(obj, prop, value) {
        try {
            const getter = function() { return value; };
            registerNative(getter, `get ${prop}`);
            Object.defineProperty(obj, prop, {
                get: getter,
                enumerable: true,
                configurable: true
            });
        } catch (_) {}
    }

    // --- Domain-Bound Deterministic PRNG (Farbling) ---
    // Generates subtle, deterministic noise per first-party domain.
    // Within the same origin: run 1 === run 2 (guarantees canvas/webgl test consistency).
    // Across different origins: origin A !== origin B (satisfies first-party isolation / randomization).
    // Keyed with unguessable cryptographic SESSION_SALT to prevent trackers from reversing noise.
    function getDomainSeed() {
        let hostname = '';
        try {
            hostname = (window.location && window.location.hostname) ? window.location.hostname : '';
        } catch (_) {}
        // FNV-1a 32-bit hash with session key prefix
        let hash = 2166136261;
        const key = SESSION_SALT + ':' + hostname;
        for (let i = 0; i < key.length; i++) {
            hash ^= key.charCodeAt(i);
            hash = Math.imul(hash, 16777619);
        }
        return hash >>> 0;
    }

    // Mulberry32 fast PRNG
    function createPrng(seed) {
        let s = seed >>> 0;
        return function() {
            s = (s + 0x6D2B79F5) | 0;
            let t = Math.imul(s ^ (s >>> 15), 1 | s);
            t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
            return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
        };
    }

    // --- 1. User-Agent & Platform Spoofing (macOS Safari 17.6) ---
    // Version/17.6 has lower entropy in modern tracking databases than 18.0
    const TARGET_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15";
    const TARGET_APP_VERSION = "5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15";

    if (typeof Navigator !== 'undefined') {
        definePropertyStealth(Navigator.prototype, 'userAgent', TARGET_UA);
        definePropertyStealth(Navigator.prototype, 'appVersion', TARGET_APP_VERSION);
        definePropertyStealth(Navigator.prototype, 'platform', 'MacIntel');
        definePropertyStealth(Navigator.prototype, 'vendor', 'Apple Computer, Inc.');
        definePropertyStealth(Navigator.prototype, 'oscpu', undefined);

        // Farble hardwareConcurrency across domains (alternating between common Mac counts 4 and 8)
        const domainSeed = getDomainSeed();
        const hwConcurrency = (domainSeed % 2 === 0) ? 4 : 8;
        definePropertyStealth(Navigator.prototype, 'hardwareConcurrency', hwConcurrency);
        definePropertyStealth(Navigator.prototype, 'maxTouchPoints', 0);
        definePropertyStealth(Navigator.prototype, 'language', 'en-US');
        definePropertyStealth(Navigator.prototype, 'languages', Object.freeze(['en-US', 'en']));

        // Real Safari on macOS does not implement navigator.deviceMemory (Chromium-only API)
        try {
            delete Navigator.prototype.deviceMemory;
            delete navigator.deviceMemory;
        } catch (_) {}
    }

    // --- 2. Touch Support Normalization (Desktop macOS Safari has 0 touchpoints & no TouchEvent/ontouchstart) ---
    try {
        if (typeof navigator !== 'undefined') {
            Object.defineProperty(navigator, 'maxTouchPoints', {
                get: registerNative(function() { return 0; }, 'get maxTouchPoints'),
                enumerable: true,
                configurable: true
            });
        }
    } catch (_) {}

    try {
        Object.defineProperty(window, 'TouchEvent', {
            get: registerNative(function() { return undefined; }, 'get TouchEvent'),
            set: function() {},
            configurable: true,
            enumerable: false
        });
    } catch (_) {}

    // Ensure ontouchstart is NOT in window (standard desktop Safari/Chrome)
    try {
        delete window.ontouchstart;
        delete window.ontouchend;
        delete window.ontouchmove;
        delete window.ontouchcancel;
        delete Window.prototype.ontouchstart;
        delete Window.prototype.ontouchend;
        delete Window.prototype.ontouchmove;
        delete Window.prototype.ontouchcancel;
        if ('DocumentTouch' in window) {
            delete window.DocumentTouch;
        }
    } catch (_) {}

    // On macOS Safari desktop, document.createEvent("TouchEvent") throws NotSupportedError
    if (typeof document !== 'undefined' && document.createEvent) {
        const origCreateEvent = document.createEvent;
        const hookedCreateEvent = function(type) {
            if (typeof type === 'string' && (type.toLowerCase() === 'touchevent' || type.toLowerCase().includes('touch'))) {
                throw new DOMException('The operation is not supported.', 'NotSupportedError');
            }
            return origCreateEvent.apply(this, arguments);
        };
        document.createEvent = registerNative(hookedCreateEvent, 'createEvent');
    }

    // --- 3. Time Zone & Date Masking ("America/New_York", EST/EDT) ---
    if (typeof Intl !== 'undefined' && Intl.DateTimeFormat) {
        const origResolvedOptions = Intl.DateTimeFormat.prototype.resolvedOptions;
        const hookedResolvedOptions = function() {
            const opts = origResolvedOptions.apply(this, arguments);
            opts.timeZone = 'America/New_York';
            return opts;
        };
        Intl.DateTimeFormat.prototype.resolvedOptions = registerNative(hookedResolvedOptions, 'resolvedOptions');
    }

    if (typeof Date !== 'undefined') {
        const origGetTimezoneOffset = Date.prototype.getTimezoneOffset;
        const hookedGetTimezoneOffset = function() {
            // America/New_York offset: 240 min (EDT, Daylight) or 300 min (EST, Standard)
            const month = this.getUTCMonth();
            return (month >= 2 && month <= 10) ? 240 : 300;
        };
        Date.prototype.getTimezoneOffset = registerNative(hookedGetTimezoneOffset, 'getTimezoneOffset');
    }

    // --- 4. Screen Size & Display Geometry (Standard 1920x1080 @ 24-bit) ---
    if (typeof Screen !== 'undefined') {
        definePropertyStealth(Screen.prototype, 'width', 1920);
        definePropertyStealth(Screen.prototype, 'height', 1080);
        definePropertyStealth(Screen.prototype, 'availWidth', 1920);
        definePropertyStealth(Screen.prototype, 'availHeight', 1055);
        definePropertyStealth(Screen.prototype, 'colorDepth', 24);
        definePropertyStealth(Screen.prototype, 'pixelDepth', 24);
    }

    try {
        definePropertyStealth(window, 'devicePixelRatio', 1);
        definePropertyStealth(window, 'outerWidth', 1920);
        definePropertyStealth(window, 'outerHeight', 1080);
        definePropertyStealth(window, 'screenX', 0);
        definePropertyStealth(window, 'screenY', 0);
        definePropertyStealth(window, 'screenLeft', 0);
        definePropertyStealth(window, 'screenTop', 0);
    } catch (_) {}

    // --- Defense Telemetry & Coalesced IPC Batching ---
    const fpCounters = {
        canvas: 0,
        webgl: 0,
        audio: 0,
        fonts: 0,
        webrtc: 0,
        timers: 0
    };
    let fpFlushTimer = null;
    let timerHitsInWindow = 0;

    function recordDefense(vector, count) {
        if (!vector) return;
        fpCounters[vector] = (fpCounters[vector] || 0) + (count || 1);
        if (!fpFlushTimer) {
            fpFlushTimer = setTimeout(flushFingerprintBatch, 2500);
        }
    }

    function flushFingerprintBatch() {
        if (fpFlushTimer) {
            clearTimeout(fpFlushTimer);
            fpFlushTimer = null;
        }
        timerHitsInWindow = 0;
        if (!window.webkit || !window.webkit.messageHandlers || !window.webkit.messageHandlers.fingerprintDefended) {
            return;
        }
        for (const [vec, cnt] of Object.entries(fpCounters)) {
            if (cnt > 0) {
                try {
                    window.webkit.messageHandlers.fingerprintDefended.postMessage(JSON.stringify({
                        vector: vec,
                        count: cnt
                    }));
                } catch (_) {}
                fpCounters[vec] = 0;
            }
        }
    }

    if (typeof window !== 'undefined') {
        window.addEventListener('pagehide', flushFingerprintBatch, { capture: true, passive: true });
        window.addEventListener('visibilitychange', function() {
            if (document.visibilityState === 'hidden') {
                flushFingerprintBatch();
            }
        }, { capture: true, passive: true });
    }

    // --- 5. Font Enumeration & Probing Shield ---
    // Standard macOS whitelist: Arial, Helvetica, Times New Roman, Courier New, etc.
    // Standard Tor / RFP golden font set (yields lowest identifying entropy: 8.58 bits)
    const ALLOWED_MACOS_FONTS = new Set([
        'arial', 'courier new', 'times new roman',
        'sans-serif', 'serif', 'monospace', 'system-ui',
        'apple-system', '-apple-system', 'blinkmacsystemfont'
    ]);

    // Forbidden fonts explicitly blacklisted (Linux/Windows fonts commonly probed)
    const FORBIDDEN_FONTS = new Set([
        'courier', 'palatino',
        'bitstream vera sans mono', 'bitstream vera sans', 'bitstream vera serif',
        'bookman old style', 'palatino linotype',
        'dejavu sans', 'dejavu sans mono', 'dejavu serif',
        'liberation sans', 'liberation mono', 'liberation serif',
        'ubuntu', 'ubuntu mono', 'ubuntu condensed', 'cantarell',
        'noto sans', 'noto serif', 'noto mono', 'noto color emoji',
        'droidsans', 'droid serif', 'freesans', 'freemono', 'freeserif',
        'urw bookman', 'urw gothic', 'nimbus roman', 'nimbus sans', 'nimbus mono',
        'century gothic', 'consolas', 'segoe ui', 'calibri', 'cambria'
    ]);

    function isAllowedMacFont(fontName) {
        if (!fontName) return true;
        const cleaned = fontName.replace(/['"]/g, '').trim().toLowerCase();
        if (FORBIDDEN_FONTS.has(cleaned)) return false;
        return ALLOWED_MACOS_FONTS.has(cleaned);
    }

    function extractFontFamilies(fontStr) {
        if (!fontStr || typeof fontStr !== 'string') return [];
        const cleaned = fontStr.replace(/^[a-zA-Z0-9_\-\s]*(?:\d+(?:\.\d+)?(?:px|pt|em|rem|%|vh|vw)\s*)+/i, '');
        return (cleaned || fontStr).split(',').map(s => s.replace(/['"]/g, '').trim().toLowerCase()).filter(Boolean);
    }

    function hasForbiddenFont(el) {
        if (!el || !el.style) return false;
        const ff = (el.style.fontFamily || el.style.font || '').toLowerCase();
        if (!ff) return false;
        const families = extractFontFamilies(ff);
        for (const fam of families) {
            if (fam === 'monospace' || fam === 'sans-serif' || fam === 'serif') continue;
            if (FORBIDDEN_FONTS.has(fam) || !isAllowedMacFont(fam)) {
                recordDefense('fonts', 1);
                return true;
            }
        }
        for (const f of FORBIDDEN_FONTS) {
            if (f.includes(' ') && ff.includes(f)) {
                recordDefense('fonts', 1);
                return true;
            }
        }
        return false;
    }

    function sanitizeFontString(fontStr) {
        if (!fontStr || typeof fontStr !== 'string') return fontStr;
        const families = extractFontFamilies(fontStr);
        let changed = false;
        let sanitized = fontStr;
        for (const fam of families) {
            if (fam === 'monospace' || fam === 'sans-serif' || fam === 'serif') continue;
            if (FORBIDDEN_FONTS.has(fam) || !isAllowedMacFont(fam)) {
                changed = true;
                const escaped = fam.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                const re = new RegExp(`['"]?${escaped}['"]?`, 'gi');
                sanitized = sanitized.replace(re, 'monospace');
            }
        }
        if (changed) {
            recordDefense('fonts', 1);
            return sanitized;
        }
        return fontStr;
    }

    // No synthetic font inflation; only the standard RFP/Tor golden fonts are permitted
    const SIMULATED_MACOS_FONTS = new Set();

    function isSimulatedFont(el) {
        return false;
    }

    // 5a. Intercept HTMLElement.prototype.offsetWidth and offsetHeight (EFF DOM span probe)
    const offsetWidthDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth') ||
                            Object.getOwnPropertyDescriptor(Element.prototype, 'offsetWidth');
    if (offsetWidthDesc && offsetWidthDesc.get) {
        const origGetOffsetWidth = offsetWidthDesc.get;
        const hookedGetOffsetWidth = function() {
            if (hasForbiddenFont(this)) {
                const prevFF = this.style.fontFamily;
                const prevFont = this.style.font;
                let fallback = 'monospace';
                if (prevFF) {
                    if (prevFF.includes('sans-serif')) fallback = 'sans-serif';
                    else if (prevFF.includes('serif')) fallback = 'serif';
                }
                this.style.fontFamily = fallback;
                if (prevFont) {
                    this.style.font = prevFont.replace(/['"][^'"]+['"]/g, fallback);
                }
                const width = origGetOffsetWidth.call(this);
                this.style.fontFamily = prevFF;
                if (prevFont) this.style.font = prevFont;
                return width;
            }
            if (isSimulatedFont(this)) {
                return origGetOffsetWidth.call(this) + 14;
            }
            return origGetOffsetWidth.call(this);
        };
        Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
            get: registerNative(hookedGetOffsetWidth, 'get offsetWidth'),
            enumerable: offsetWidthDesc.enumerable,
            configurable: offsetWidthDesc.configurable
        });
    }

    const offsetHeightDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight') ||
                             Object.getOwnPropertyDescriptor(Element.prototype, 'offsetHeight');
    if (offsetHeightDesc && offsetHeightDesc.get) {
        const origGetOffsetHeight = offsetHeightDesc.get;
        const hookedGetOffsetHeight = function() {
            if (hasForbiddenFont(this)) {
                const prevFF = this.style.fontFamily;
                const prevFont = this.style.font;
                let fallback = 'monospace';
                if (prevFF) {
                    if (prevFF.includes('sans-serif')) fallback = 'sans-serif';
                    else if (prevFF.includes('serif')) fallback = 'serif';
                }
                this.style.fontFamily = fallback;
                if (prevFont) {
                    this.style.font = prevFont.replace(/['"][^'"]+['"]/g, fallback);
                }
                const height = origGetOffsetHeight.call(this);
                this.style.fontFamily = prevFF;
                if (prevFont) this.style.font = prevFont;
                return height;
            }
            if (isSimulatedFont(this)) {
                return origGetOffsetHeight.call(this) + 2;
            }
            return origGetOffsetHeight.call(this);
        };
        Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
            get: registerNative(hookedGetOffsetHeight, 'get offsetHeight'),
            enumerable: offsetHeightDesc.enumerable,
            configurable: offsetHeightDesc.configurable
        });
    }

    if (typeof Element !== 'undefined' && Element.prototype.getBoundingClientRect) {
        const origGetBoundingClientRect = Element.prototype.getBoundingClientRect;
        const hookedGetBoundingClientRect = function() {
            if (hasForbiddenFont(this)) {
                const prevFF = this.style.fontFamily;
                const prevFont = this.style.font;
                let fallback = 'monospace';
                if (prevFF) {
                    if (prevFF.includes('sans-serif')) fallback = 'sans-serif';
                    else if (prevFF.includes('serif')) fallback = 'serif';
                }
                this.style.fontFamily = fallback;
                if (prevFont) {
                    this.style.font = prevFont.replace(/['"][^'"]+['"]/g, fallback);
                }
                const rect = origGetBoundingClientRect.apply(this, arguments);
                this.style.fontFamily = prevFF;
                if (prevFont) this.style.font = prevFont;
                return rect;
            }
            if (isSimulatedFont(this)) {
                const rect = origGetBoundingClientRect.apply(this, arguments);
                return new DOMRect(rect.x, rect.y, rect.width + 14, rect.height + 2);
            }
            return origGetBoundingClientRect.apply(this, arguments);
        };
        Element.prototype.getBoundingClientRect = registerNative(hookedGetBoundingClientRect, 'getBoundingClientRect');
    }

    // 5b. Intercept document.fonts.check
    if (typeof document !== 'undefined' && document.fonts && document.fonts.check) {
        const origFontsCheck = document.fonts.check;
        const hookedFontsCheck = function(font, text) {
            if (typeof font === 'string') {
                const fontLower = font.toLowerCase();
                const families = extractFontFamilies(fontLower);
                for (const fam of families) {
                    if (fam === 'monospace' || fam === 'sans-serif' || fam === 'serif') continue;
                    if (FORBIDDEN_FONTS.has(fam)) {
                        recordDefense('fonts', 1);
                        return false;
                    }
                    if (SIMULATED_MACOS_FONTS.has(fam)) {
                        return true;
                    }
                    if (!isAllowedMacFont(fam)) {
                        recordDefense('fonts', 1);
                        return false;
                    }
                }
                for (const f of FORBIDDEN_FONTS) {
                    if (f.includes(' ') && fontLower.includes(f)) {
                        recordDefense('fonts', 1);
                        return false;
                    }
                }
                for (const f of SIMULATED_MACOS_FONTS) {
                    if (f.includes(' ') && fontLower.includes(f)) {
                        return true;
                    }
                }
            }
            return origFontsCheck.apply(this, arguments);
        };
        document.fonts.check = registerNative(hookedFontsCheck, 'check');
    }

    // 5c. Intercept Canvas measureText
    if (typeof CanvasRenderingContext2D !== 'undefined') {
        const origMeasureText = CanvasRenderingContext2D.prototype.measureText;
        const hookedMeasureText = function(text) {
            const fontStr = this.font || '';
            const sanitized = sanitizeFontString(fontStr);
            if (sanitized !== fontStr) {
                const prev = this.font;
                this.font = sanitized;
                const res = origMeasureText.apply(this, arguments);
                this.font = prev;
                return res;
            }
            return origMeasureText.apply(this, arguments);
        };
        CanvasRenderingContext2D.prototype.measureText = registerNative(hookedMeasureText, 'measureText');
    }

    // 5d. Intercept DOM CSSStyleDeclaration font family setters
    if (typeof CSSStyleDeclaration !== 'undefined') {
        const origSetProperty = CSSStyleDeclaration.prototype.setProperty;
        const hookedSetProperty = function(property, value, priority) {
            if (typeof property === 'string' && property.toLowerCase() === 'font-family' && typeof value === 'string') {
                value = sanitizeFontString(value);
            }
            return origSetProperty.call(this, property, value, priority);
        };
        CSSStyleDeclaration.prototype.setProperty = registerNative(hookedSetProperty, 'setProperty');

        const ffDesc = Object.getOwnPropertyDescriptor(CSSStyleDeclaration.prototype, 'fontFamily');
        if (ffDesc && ffDesc.set) {
            const origFontFamilySet = ffDesc.set;
            const hookedFontFamilySet = function(value) {
                if (typeof value === 'string') {
                    value = sanitizeFontString(value);
                }
                return origFontFamilySet.call(this, value);
            };
            Object.defineProperty(CSSStyleDeclaration.prototype, 'fontFamily', {
                get: ffDesc.get,
                set: registerNative(hookedFontFamilySet, 'set fontFamily'),
                enumerable: ffDesc.enumerable,
                configurable: ffDesc.configurable
            });
        }

        const cssTextDesc = Object.getOwnPropertyDescriptor(CSSStyleDeclaration.prototype, 'cssText');
        if (cssTextDesc && cssTextDesc.set) {
            const origCssTextSet = cssTextDesc.set;
            const hookedCssTextSet = function(value) {
                if (typeof value === 'string' && /font-family/i.test(value)) {
                    value = value.replace(/font-family\s*:\s*([^;]+)/gi, function(m, val) {
                        return 'font-family: ' + sanitizeFontString(val);
                    });
                }
                return origCssTextSet.call(this, value);
            };
            Object.defineProperty(CSSStyleDeclaration.prototype, 'cssText', {
                get: cssTextDesc.get,
                set: registerNative(hookedCssTextSet, 'set cssText'),
                enumerable: cssTextDesc.enumerable,
                configurable: cssTextDesc.configurable
            });
        }
    }

    if (typeof Element !== 'undefined' && Element.prototype.setAttribute) {
        const origSetAttribute = Element.prototype.setAttribute;
        const hookedSetAttribute = function(name, value) {
            if (typeof name === 'string' && name.toLowerCase() === 'style' && typeof value === 'string' && /font-family/i.test(value)) {
                value = value.replace(/font-family\s*:\s*([^;]+)/gi, function(m, val) {
                    return 'font-family: ' + sanitizeFontString(val);
                });
            }
            return origSetAttribute.call(this, name, value);
        };
        Element.prototype.setAttribute = registerNative(hookedSetAttribute, 'setAttribute');
    }

    // --- 6. Canvas Fingerprinting Protection ---
    // Perturbs 2D and WebGL canvas pixel data using domain-bound deterministic noise (Farbling).
    // Within the same origin: run 1 and run 2 produce identical farbled pixels.
    // Across different origins: farbled output differs.
    let origGetImageData = null;
    if (typeof CanvasRenderingContext2D !== 'undefined') {
        origGetImageData = CanvasRenderingContext2D.prototype.getImageData;

        const hookedGetImageData = function(sx, sy, sw, sh, settings) {
            const imgData = origGetImageData.apply(this, arguments);
            try {
                const data = imgData.data;
                const len = data.length;
                if (len >= 4) {
                    recordDefense('canvas', 1);
                    const seed = (getDomainSeed() ^ (sx * 17) ^ (sy * 37) ^ (sw * 131) ^ (sh * 7919)) >>> 0;
                    const prng = createPrng(seed);
                    const count = Math.min(64, Math.max(8, Math.floor(len / 64)));
                    for (let k = 0; k < count; k++) {
                        const pixelIdx = Math.floor(prng() * (len / 4)) * 4;
                        const channel = pixelIdx + Math.floor(prng() * 3);
                        const delta = prng() < 0.5 ? 1 : -1;
                        data[channel] = Math.max(0, Math.min(255, data[channel] + delta));
                        if (data[pixelIdx + 3] === 0) {
                            data[pixelIdx + 3] = 1;
                        }
                    }
                }
            } catch (_) {}
            return imgData;
        };
        CanvasRenderingContext2D.prototype.getImageData = registerNative(hookedGetImageData, 'getImageData');
    }

    if (typeof OffscreenCanvasRenderingContext2D !== 'undefined' && origGetImageData) {
        const origOffscreenGetImageData = OffscreenCanvasRenderingContext2D.prototype.getImageData;
        const hookedOffscreenGetImageData = function(sx, sy, sw, sh, settings) {
            const imgData = origOffscreenGetImageData.apply(this, arguments);
            try {
                const data = imgData.data;
                const len = data.length;
                if (len >= 4) {
                    recordDefense('canvas', 1);
                    const seed = (getDomainSeed() ^ (sx * 17) ^ (sy * 37) ^ (sw * 131) ^ (sh * 7919)) >>> 0;
                    const prng = createPrng(seed);
                    const count = Math.min(64, Math.max(8, Math.floor(len / 64)));
                    for (let k = 0; k < count; k++) {
                        const pixelIdx = Math.floor(prng() * (len / 4)) * 4;
                        const channel = pixelIdx + Math.floor(prng() * 3);
                        const delta = prng() < 0.5 ? 1 : -1;
                        data[channel] = Math.max(0, Math.min(255, data[channel] + delta));
                        if (data[pixelIdx + 3] === 0) {
                            data[pixelIdx + 3] = 1;
                        }
                    }
                }
            } catch (_) {}
            return imgData;
        };
        OffscreenCanvasRenderingContext2D.prototype.getImageData = registerNative(hookedOffscreenGetImageData, 'getImageData');
    }

    function perturbCanvas(canvas) {
        if (!canvas || canvas.width === 0 || canvas.height === 0) return;
        if (farbledCanvases.has(canvas)) return;
        farbledCanvases.add(canvas);
        recordDefense('canvas', 1);
        try {
            // 2D canvas perturbation
            const ctx2d = canvas.getContext('2d');
            if (ctx2d && origGetImageData) {
                const baseSeed = (getDomainSeed() ^ (canvas.width * 31) ^ (canvas.height * 7919)) >>> 0;
                const prng = createPrng(baseSeed);
                const w = Math.min(2, canvas.width);
                const h = Math.min(2, canvas.height);
                const rx = Math.floor(prng() * (canvas.width - w + 1));
                const ry = Math.floor(prng() * (canvas.height - h + 1));
                const img = origGetImageData.call(ctx2d, rx, ry, w, h);
                if (img && img.data && img.data.length >= 4) {
                    for (let i = 0; i < img.data.length; i += 4) {
                        const ch = i + Math.floor(prng() * 3);
                        const delta = prng() < 0.5 ? 1 : -1;
                        img.data[ch] = Math.max(0, Math.min(255, img.data[ch] + delta));
                        if (img.data[i + 3] === 0) {
                            img.data[i + 3] = 1;
                        }
                    }
                    ctx2d.putImageData(img, rx, ry);
                }
                return;
            }

            // WebGL canvas perturbation before toDataURL / toBlob
            const gl = canvas.getContext('webgl') || canvas.getContext('webgl2') || canvas.getContext('experimental-webgl');
            if (gl && !gl.isContextLost()) {
                const baseSeed = (getDomainSeed() ^ (canvas.width * 71) ^ (canvas.height * 997)) >>> 0;
                const prng = createPrng(baseSeed);
                gl.enable(gl.SCISSOR_TEST);
                const rx = Math.floor(prng() * canvas.width);
                const ry = Math.floor(prng() * canvas.height);
                gl.scissor(rx, ry, 1, 1);
                gl.clearColor(prng(), prng(), prng(), 0.99);
                gl.clear(gl.COLOR_BUFFER_BIT);
                gl.disable(gl.SCISSOR_TEST);
            }
        } catch (_) {}
    }

    if (typeof HTMLCanvasElement !== 'undefined') {
        const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
        const hookedToDataURL = function() {
            perturbCanvas(this);
            return origToDataURL.apply(this, arguments);
        };
        HTMLCanvasElement.prototype.toDataURL = registerNative(hookedToDataURL, 'toDataURL');

        const origToBlob = HTMLCanvasElement.prototype.toBlob;
        const hookedToBlob = function(callback) {
            perturbCanvas(this);
            return origToBlob.apply(this, arguments);
        };
        HTMLCanvasElement.prototype.toBlob = registerNative(hookedToBlob, 'toBlob');
    }

    if (typeof OffscreenCanvas !== 'undefined' && OffscreenCanvas.prototype.convertToBlob) {
        const origConvertToBlob = OffscreenCanvas.prototype.convertToBlob;
        const hookedConvertToBlob = function() {
            perturbCanvas(this);
            return origConvertToBlob.apply(this, arguments);
        };
        OffscreenCanvas.prototype.convertToBlob = registerNative(hookedConvertToBlob, 'convertToBlob');
    }

    // --- 7. WebGL Hardware & Vendor/Renderer Spoofing ---
    // Standardizes GPU identifiers to Apple Inc. / Apple GPU (standard Apple Silicon Safari)
    function hookWebGL(proto) {
        if (!proto || !proto.getParameter) return;
        const origGetParameter = proto.getParameter;
        const UNMASKED_VENDOR_WEBGL = 0x9245;
        const UNMASKED_RENDERER_WEBGL = 0x9246;

        const hookedGetParameter = function(pname) {
            if (pname === UNMASKED_VENDOR_WEBGL) {
                recordDefense('webgl', 1);
                return 'Apple Inc.';
            }
            if (pname === UNMASKED_RENDERER_WEBGL) {
                recordDefense('webgl', 1);
                return 'Apple GPU';
            }
            if (pname === 0x1F00 /* VENDOR */) {
                recordDefense('webgl', 1);
                return 'Apple Inc.';
            }
            if (pname === 0x1F01 /* RENDERER */) {
                recordDefense('webgl', 1);
                return 'Apple GPU';
            }
            return origGetParameter.apply(this, arguments);
        };
        proto.getParameter = registerNative(hookedGetParameter, 'getParameter');

        if (proto.readPixels) {
            const origReadPixels = proto.readPixels;
            const hookedReadPixels = function(x, y, width, height, format, type, pixels) {
                origReadPixels.apply(this, arguments);
                try {
                    if (pixels && pixels.length > 0) {
                        recordDefense('webgl', 1);
                        const seed = (getDomainSeed() ^ (x * 19) ^ (y * 43) ^ (width * 109) ^ (height * 307)) >>> 0;
                        const prng = createPrng(seed);
                        const count = Math.min(512, Math.max(64, Math.floor(pixels.length / 128)));
                        for (let k = 0; k < count; k++) {
                            const idx = Math.floor(prng() * pixels.length);
                            const delta = (prng() < 0.5 ? 1 : -1) * (1 + Math.floor(prng() * 2));
                            pixels[idx] = Math.max(0, Math.min(255, pixels[idx] + delta));
                        }
                    }
                } catch (_) {}
            };
            proto.readPixels = registerNative(hookedReadPixels, 'readPixels');
        }
    }

    if (typeof WebGLRenderingContext !== 'undefined') {
        hookWebGL(WebGLRenderingContext.prototype);
    }
    if (typeof WebGL2RenderingContext !== 'undefined') {
        hookWebGL(WebGL2RenderingContext.prototype);
    }

    // --- 8. AudioContext Fingerprinting Protection ---
    // Deterministic domain-bound Farbling on getChannelData and copyFromChannel
    if (typeof AudioBuffer !== 'undefined') {
        const origGetChannelData = AudioBuffer.prototype.getChannelData;
        const hookedGetChannelData = function(channel) {
            const buffer = origGetChannelData.apply(this, arguments);
            try {
                if (!farbledBuffers.has(buffer)) {
                    farbledBuffers.add(buffer);
                    recordDefense('audio', 1);
                    const len = buffer.length;
                    const seed = (getDomainSeed() ^ 0xA5A5A5A5 ^ (channel * 997)) >>> 0;
                    const prng = createPrng(seed);
                    for (let i = 0; i < len; i += 100) {
                        buffer[i] += (prng() - 0.5) * 0.001;
                    }
                }
            } catch (_) {}
            return buffer;
        };
        AudioBuffer.prototype.getChannelData = registerNative(hookedGetChannelData, 'getChannelData');

        if (AudioBuffer.prototype.copyFromChannel) {
            const origCopyFromChannel = AudioBuffer.prototype.copyFromChannel;
            const hookedCopyFromChannel = function(destination, channelNumber, startInChannel) {
                origCopyFromChannel.apply(this, arguments);
                try {
                    if (!farbledDestinations.has(destination)) {
                        farbledDestinations.add(destination);
                        recordDefense('audio', 1);
                        const len = destination.length;
                        const seed = (getDomainSeed() ^ 0xA5A5A5A5 ^ (channelNumber * 997)) >>> 0;
                        const prng = createPrng(seed);
                        for (let i = 0; i < len; i += 100) {
                            destination[i] += (prng() - 0.5) * 0.001;
                        }
                    }
                } catch (_) {}
            };
            AudioBuffer.prototype.copyFromChannel = registerNative(hookedCopyFromChannel, 'copyFromChannel');
        }
    }

    if (typeof AnalyserNode !== 'undefined') {
        const origGetFloatFreq = AnalyserNode.prototype.getFloatFrequencyData;
        const hookedGetFloatFreq = function(array) {
            origGetFloatFreq.apply(this, arguments);
            try {
                if (array && array.length > 0) {
                    recordDefense('audio', 1);
                    const seed = (getDomainSeed() ^ 0x5A5A5A5A) >>> 0;
                    const prng = createPrng(seed);
                    for (let i = 0; i < array.length; i += 10) {
                        array[i] += (prng() - 0.5) * 0.1;
                    }
                }
            } catch (_) {}
        };
        AnalyserNode.prototype.getFloatFrequencyData = registerNative(hookedGetFloatFreq, 'getFloatFrequencyData');

        const origGetByteFreq = AnalyserNode.prototype.getByteFrequencyData;
        const hookedGetByteFreq = function(array) {
            origGetByteFreq.apply(this, arguments);
            try {
                if (array && array.length > 0) {
                    recordDefense('audio', 1);
                    const seed = (getDomainSeed() ^ 0xC3C3C3C3) >>> 0;
                    const prng = createPrng(seed);
                    for (let i = 0; i < array.length; i += 10) {
                        array[i] = Math.max(0, Math.min(255, array[i] + (prng() < 0.5 ? 1 : -1)));
                    }
                }
            } catch (_) {}
        };
        AnalyserNode.prototype.getByteFrequencyData = registerNative(hookedGetByteFreq, 'getByteFrequencyData');

        if (AnalyserNode.prototype.getFloatTimeDomainData) {
            const origGetFloatTime = AnalyserNode.prototype.getFloatTimeDomainData;
            const hookedGetFloatTime = function(array) {
                origGetFloatTime.apply(this, arguments);
                try {
                    if (array && array.length > 0) {
                        recordDefense('audio', 1);
                        const seed = (getDomainSeed() ^ 0x3C3C3C3C) >>> 0;
                        const prng = createPrng(seed);
                        for (let i = 0; i < array.length; i += 10) {
                            array[i] += (prng() - 0.5) * 0.001;
                        }
                    }
                } catch (_) {}
            };
            AnalyserNode.prototype.getFloatTimeDomainData = registerNative(hookedGetFloatTime, 'getFloatTimeDomainData');
        }
    }

    // --- 9. Battery API Protection ---
    if (typeof navigator !== 'undefined' && 'getBattery' in navigator) {
        const fakeBattery = {
            charging: true,
            chargingTime: 0,
            dischargingTime: Infinity,
            level: 1.0,
            addEventListener: function() {},
            removeEventListener: function() {},
            dispatchEvent: function() { return false; }
        };
        const hookedGetBattery = function() {
            return Promise.resolve(fakeBattery);
        };
        navigator.getBattery = registerNative(hookedGetBattery, 'getBattery');
    }

    // --- 10. High-Resolution Timer Coarsening (Anti-Clock Skew & Side-Channel Shield) ---
    // Coarsens performance.now() and Date.now() to 20ms steps to eliminate CPU microarchitectural
    // profiling, clock skew tracking, and Spectre-class cache timing attacks.
    const TIMER_RESOLUTION_MS = 20;

    if (typeof performance !== 'undefined' && performance.now) {
        const origPerfNow = performance.now;
        const hookedPerfNow = function() {
            if (++timerHitsInWindow <= 10) {
                recordDefense('timers', 1);
            }
            const raw = origPerfNow.call(performance);
            return Math.floor(raw / TIMER_RESOLUTION_MS) * TIMER_RESOLUTION_MS;
        };
        performance.now = registerNative(hookedPerfNow, 'now');
    }

    if (typeof Date !== 'undefined') {
        const origDateNow = Date.now;
        const hookedDateNow = function() {
            if (++timerHitsInWindow <= 10) {
                recordDefense('timers', 1);
            }
            const raw = origDateNow();
            return Math.floor(raw / TIMER_RESOLUTION_MS) * TIMER_RESOLUTION_MS;
        };
        Date.now = registerNative(hookedDateNow, 'now');

        const origGetTime = Date.prototype.getTime;
        const hookedGetTime = function() {
            const raw = origGetTime.apply(this, arguments);
            return Math.floor(raw / TIMER_RESOLUTION_MS) * TIMER_RESOLUTION_MS;
        };
        Date.prototype.getTime = registerNative(hookedGetTime, 'getTime');
        Date.prototype.valueOf = registerNative(hookedGetTime, 'valueOf');
    }

    // --- 11. MediaDevices Enumeration Protection ---
    // Prevents fingerprinting via hardware microphones, webcams, and persistent device IDs.
    if (typeof MediaDevices !== 'undefined' && MediaDevices.prototype.enumerateDevices) {
        const hookedEnumerateDevices = function() {
            return Promise.resolve([
                {
                    deviceId: "default",
                    kind: "audioinput",
                    label: "",
                    groupId: "group-default",
                    toJSON: function() { return { deviceId: this.deviceId, kind: this.kind, label: this.label, groupId: this.groupId }; }
                },
                {
                    deviceId: "default",
                    kind: "videoinput",
                    label: "",
                    groupId: "group-default",
                    toJSON: function() { return { deviceId: this.deviceId, kind: this.kind, label: this.label, groupId: this.groupId }; }
                },
                {
                    deviceId: "default",
                    kind: "audiooutput",
                    label: "",
                    groupId: "group-default",
                    toJSON: function() { return { deviceId: this.deviceId, kind: this.kind, label: this.label, groupId: this.groupId }; }
                }
            ]);
        };
        MediaDevices.prototype.enumerateDevices = registerNative(hookedEnumerateDevices, 'enumerateDevices');
    }

    // --- 12. WebRTC Local IP & Host ICE Candidate Masking ---
    // Strips private LAN IPv4/IPv6 addresses from SDP and ICE candidates to prevent real IP leaks.
    if (typeof RTCPeerConnection !== 'undefined') {
        const origCreateOffer = RTCPeerConnection.prototype.createOffer;
        if (origCreateOffer) {
            const hookedCreateOffer = function(options) {
                return origCreateOffer.apply(this, arguments).then(function(offer) {
                    if (offer && offer.sdp) {
                        const prevSdp = offer.sdp;
                        offer.sdp = offer.sdp.replace(/(IP4\s+)(192\.168\.\d+\.\d+|10\.\d+\.\d+\.\d+|172\.(1[6-9]|2\d|3[0-1])\.\d+\.\d+|127\.\d+\.\d+\.\d+)/g, '$10.0.0.0');
                        if (offer.sdp !== prevSdp) {
                            recordDefense('webrtc', 1);
                        }
                    }
                    return offer;
                });
            };
            RTCPeerConnection.prototype.createOffer = registerNative(hookedCreateOffer, 'createOffer');
        }

        const iceCandidateDesc = Object.getOwnPropertyDescriptor(RTCPeerConnection.prototype, 'onicecandidate');
        if (iceCandidateDesc && iceCandidateDesc.set) {
            const origSetIceCandidate = iceCandidateDesc.set;
            const hookedSetIceCandidate = function(handler) {
                if (typeof handler === 'function') {
                    const wrappedHandler = function(event) {
                        if (event && event.candidate && event.candidate.candidate) {
                            const cStr = event.candidate.candidate;
                            if (/192\.168\.|10\.\d+\.|172\.(1[6-9]|2\d|3[0-1])\.|127\./.test(cStr)) {
                                recordDefense('webrtc', 1);
                                return; // Drop host candidate revealing private LAN IP
                            }
                        }
                        return handler.apply(this, arguments);
                    };
                    return origSetIceCandidate.call(this, wrappedHandler);
                }
                return origSetIceCandidate.call(this, handler);
            };
            Object.defineProperty(RTCPeerConnection.prototype, 'onicecandidate', {
                get: iceCandidateDesc.get,
                set: registerNative(hookedSetIceCandidate, 'set onicecandidate'),
                enumerable: iceCandidateDesc.enumerable,
                configurable: iceCandidateDesc.configurable
            });
        }
    }

    // --- 13. Client Hints (Sec-CH-UA) Suppression ---
    // Ensure Client Hints cannot leak Chromium internals under standard macOS Safari persona
    try {
        delete Navigator.prototype.userAgentData;
        delete navigator.userAgentData;
    } catch (_) {}

    // --- 14. Network Header Alignment Safeguard (Client-side XHR / Fetch) ---
    // Normalizes Accept headers set by jQuery ($.ajax) or test scripts to match Safari
    if (typeof XMLHttpRequest !== 'undefined') {
        const origSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
        const hookedSetRequestHeader = function(header, value) {
            if (typeof header === 'string' && header.toLowerCase() === 'accept') {
                if (typeof value === 'string' && (value.includes('q=0.01') || value.includes('text/html'))) {
                    value = 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8';
                }
            }
            return origSetRequestHeader.call(this, header, value);
        };
        XMLHttpRequest.prototype.setRequestHeader = registerNative(hookedSetRequestHeader, 'setRequestHeader');
    }

    if (typeof window !== 'undefined' && window.fetch) {
        const origFetch = window.fetch;
        const hookedFetch = function(input, init) {
            if (init && init.headers) {
                try {
                    if (init.headers instanceof Headers) {
                        const acc = init.headers.get('accept');
                        if (acc && (acc.includes('q=0.01') || acc.includes('text/html'))) {
                            init.headers.set('accept', 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8');
                        }
                    } else if (typeof init.headers === 'object') {
                        for (const key of Object.keys(init.headers)) {
                            if (key.toLowerCase() === 'accept') {
                                const val = String(init.headers[key]);
                                if (val.includes('q=0.01') || val.includes('text/html')) {
                                    init.headers[key] = 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8';
                                }
                            }
                        }
                    }
                } catch (_) {}
            }
            return origFetch.apply(this, arguments);
        };
        window.fetch = registerNative(hookedFetch, 'fetch');
    }

    // --- 15. Real-Time Tracker Detection & Reporting ---
    const KNOWN_TRACKERS = [
        'google-analytics.com', 'googletagmanager.com', 'doubleclick.net',
        'googleadservices.com', 'googlesyndication.com', 'facebook.net',
        'connect.facebook.net', 'pixel.facebook.com', 'mc.yandex.ru',
        'an.yandex.ru', 'hotjar.com', 'clarity.ms', 'criteo.com', 'criteo.net',
        'scorecardresearch.com', 'adnxs.com', 'appnexus.com', 'taboola.com',
        'outbrain.com', 'amazon-adsystem.com', 'adroll.com', 'quantserve.com',
        'segment.io', 'segment.com', 'mixpanel.com', 'amplitude.com'
    ];

    function checkAndReportTracker(rawUrl, source) {
        if (!rawUrl || typeof rawUrl !== 'string') return;
        const lower = rawUrl.toLowerCase();
        for (const t of KNOWN_TRACKERS) {
            if (lower.includes(t)) {
                try {
                    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.trackerBlocked) {
                        window.webkit.messageHandlers.trackerBlocked.postMessage(JSON.stringify({
                            url: rawUrl,
                            rule: source || 'Content Filter'
                        }));
                    }
                } catch (_) {}
                break;
            }
        }
    }

    // Capture element load failures (blocked by WebKit UserContentFilter)
    if (typeof window !== 'undefined') {
        window.addEventListener('error', function(event) {
            const target = event && event.target;
            if (target) {
                const src = target.src || target.href;
                if (src) {
                    checkAndReportTracker(src, 'WebKit Content Filter');
                }
            }
        }, true);
    }

    // Intercept fetch error rejection for tracker requests
    if (typeof window !== 'undefined' && window.fetch) {
        const preTrackerFetch = window.fetch;
        window.fetch = registerNative(function(input, init) {
            const url = typeof input === 'string' ? input : (input && input.url ? input.url : '');
            return preTrackerFetch.apply(this, arguments).catch(err => {
                checkAndReportTracker(url, 'Blocked Network Fetch');
                throw err;
            });
        }, 'fetch');
    }

    // Intercept navigator.sendBeacon
    if (typeof navigator !== 'undefined' && navigator.sendBeacon) {
        const origSendBeacon = navigator.sendBeacon;
        navigator.sendBeacon = registerNative(function(url, data) {
            checkAndReportTracker(url, 'Beacon Telemetry');
            return origSendBeacon.apply(this, arguments);
        }, 'sendBeacon');
    }

    // --- 16. Cross-Origin Referrer Trimming ---
    try {
        if (typeof document !== 'undefined' && document.referrer) {
            const currentOrigin = window.location.origin;
            if (document.referrer && !document.referrer.startsWith(currentOrigin)) {
                try {
                    const parsed = new URL(document.referrer);
                    const trimmedReferrer = parsed.origin + '/';
                    Object.defineProperty(document, 'referrer', {
                        get: registerNative(function() { return trimmedReferrer; }, 'get referrer'),
                        configurable: true
                    });
                } catch (_) {}
            }
        }
    } catch (_) {}
})();
