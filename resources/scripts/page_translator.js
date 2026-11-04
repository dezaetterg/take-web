(function () {
    'use strict';

    if (window.__TakeTranslator) {
        return;
    }

    const SKIP_TAGS = new Set([
        'SCRIPT', 'STYLE', 'NOSCRIPT', 'CODE', 'PRE', 'KBD', 'SAMP', 'VAR',
        'TEXTAREA', 'INPUT', 'SELECT', 'OPTION', 'SVG', 'MATH', 'CANVAS',
        'IFRAME', 'OBJECT', 'EMBED', 'AUDIO', 'VIDEO', 'HEAD', 'META', 'LINK'
    ]);

    const BLOCK_TAGS = new Set([
        'P', 'LI', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'TD', 'TH',
        'BLOCKQUOTE', 'ARTICLE', 'SECTION', 'FIGCAPTION', 'DD', 'DT',
        'SUMMARY', 'HEADER', 'FOOTER', 'NAV', 'ASIDE', 'MAIN', 'DIV'
    ]);

    const MAX_BATCH_ITEMS = 24;
    const MAX_BATCH_CHARS = 1800;
    const MAX_CACHE_SIZE = 4096;
    const FRAME_BUDGET_MS = 6.0;

    const originalTexts = new WeakMap();
    const translatedNodes = new Set();
    const pendingBlocks = new WeakMap();
    const translationCache = new Map();

    const state = {
        isPageTranslated: false,
        userRestoredOriginal: false,
        isDownloading: false,
        isLoadingWasm: false,
        sourceLang: 'auto',
        effectiveSourceLang: 'en',
        targetLang: 'ru',
        selectionPopupEnabled: true,
        translatedCount: 0,
        cacheHits: 0,
        batchesProcessed: 0,
        activePairs: [],
        isPivot: false,
        mutationObserver: null,
        intersectionObserver: null,
        mutationTimer: null,
        pendingSelectionRequest: null,
        uiStrings: {
            title: 'Локальный перевод (Bergamot WASM)',
            badge: 'WASM • Локально',
            translateSelection: 'Перевести выделенное',
            translatePage: 'Перевести всю страницу',
            restorePage: 'Показать оригинал',
            copy: 'Копировать',
            copied: 'Скопировано',
            close: 'Закрыть',
            downloading: 'Скачивание нейросетевой модели Bergamot…',
            loadingWasm: 'Инициализация WebWorker и весов Bergamot WASM…'
        }
    };

    function postTranslateIpc(payload) {
        try {
            if (
                window.webkit &&
                window.webkit.messageHandlers &&
                window.webkit.messageHandlers.takeTranslate
            ) {
                window.webkit.messageHandlers.takeTranslate.postMessage(JSON.stringify(payload));
                return true;
            }
        } catch (err) {
            console.warn('[TakeTranslator] IPC postMessage error:', err);
        }
        return false;
    }

    function fnv1aHash(str) {
        let h = 0x811c9dc5;
        for (let i = 0; i < str.length; i++) {
            h ^= str.charCodeAt(i);
            h = Math.imul(h, 0x01000193);
        }
        return (h >>> 0).toString(36);
    }

    function cacheGet(src, tgt, text) {
        const key = src + ':' + tgt + ':' + fnv1aHash(text) + ':' + text.length;
        if (!translationCache.has(key)) {
            return null;
        }
        const val = translationCache.get(key);
        translationCache.delete(key);
        translationCache.set(key, val);
        state.cacheHits++;
        return val;
    }

    function cacheSet(src, tgt, text, translated) {
        const key = src + ':' + tgt + ':' + fnv1aHash(text) + ':' + text.length;
        if (translationCache.size >= MAX_CACHE_SIZE) {
            const oldestKey = translationCache.keys().next().value;
            if (oldestKey !== undefined) {
                translationCache.delete(oldestKey);
            }
        }
        translationCache.set(key, translated);
    }

    function detectLanguage(text) {
        const sample = (text || '').slice(0, 1500);
        let cyrillic = 0;
        let han = 0;
        let kana = 0;
        let latin = 0;

        for (let i = 0; i < sample.length; i++) {
            const code = sample.charCodeAt(i);
            if (code >= 0x0400 && code <= 0x04FF) {
                cyrillic++;
            } else if (code >= 0x3040 && code <= 0x30FF) {
                kana++;
            } else if (code >= 0x4E00 && code <= 0x9FFF) {
                han++;
            } else if ((code >= 0x0041 && code <= 0x005A) || (code >= 0x0061 && code <= 0x007A)) {
                latin++;
            }
        }

        if (kana > 2) return 'ja';
        if (han > 4 && han * 2 >= latin) return 'zh';
        if (cyrillic > latin && cyrillic > 2) return 'ru';

        const htmlLang = (document.documentElement && document.documentElement.lang || '')
            .trim()
            .toLowerCase()
            .split(/[-_]/)[0];
        if (['en', 'ru', 'de', 'fr', 'es', 'zh', 'ja'].includes(htmlLang)) {
            if (htmlLang !== 'ru' || cyrillic > 0) {
                return htmlLang;
            }
        }

        const lower = ' ' + sample.toLowerCase() + ' ';
        if (/[äöüß]/.test(lower) || /\b(und|der|die|das|nicht|mit|auf|für)\b/.test(lower)) {
            return 'de';
        }
        if (/[ñ¿¡]/.test(lower) || /\b(para|como|pero|más|esta|este|sobre)\b/.test(lower)) {
            return 'es';
        }
        if (/[œæç]/.test(lower) || /\b(dans|avec|pour|une|des|les|est|pas|plus)\b/.test(lower)) {
            return 'fr';
        }
        return 'en';
    }

    function detectPageLanguage() {
        let sample = '';
        if (document.body) {
            sample = (document.body.innerText || '').slice(0, 2000);
        }
        return detectLanguage(sample);
    }

    function resolveEffectiveSource(sourceLang, sampleText, targetLang) {
        if (sourceLang && sourceLang !== 'auto') {
            return sourceLang;
        }
        const detected = sampleText ? detectLanguage(sampleText) : detectPageLanguage();
        if (detected === targetLang) {
            return targetLang === 'en' ? 'ru' : 'en';
        }
        return detected;
    }

    // =========================================================================
    // Bergamot WASM WebWorker Engine (with automatic in-page fallback for CSP)
    // =========================================================================

    const WORKER_BOOTSTRAP_SOURCE = `
        'use strict';

        const GEMM_MAP = {
            "int8_prepare_a": "int8PrepareAFallback",
            "int8_prepare_b": "int8PrepareBFallback",
            "int8_prepare_b_from_transposed": "int8PrepareBFromTransposedFallback",
            "int8_prepare_b_from_quantized_transposed": "int8PrepareBFromQuantizedTransposedFallback",
            "int8_prepare_bias": "int8PrepareBiasFallback",
            "int8_multiply_and_add_bias": "int8MultiplyAndAddBiasFallback",
            "int8_select_columns_of_b": "int8SelectColumnsOfBFallback"
        };

        const MODEL_CONFIG_YAML = [
            "beam-size: 1",
            "normalize: 1.0",
            "word-penalty: 0",
            "cpu-threads: 0",
            "gemm-precision: int8shiftAlphaAll",
            "skip-cost: true",
            "alignment: soft",
            "quiet: true",
            "quiet-translation: true",
            "max-length-break: 128",
            "mini-batch-words: 1024",
            "workspace: 128",
            "max-length-factor: 2.0"
        ].join("\\n") + "\\n";

        let wasmReady = false;
        let service = null;
        const models = new Map();

        function allocAligned(M, arrayBuffer, align) {
            const bytes = new Int8Array(arrayBuffer);
            const mem = new M.AlignedMemory(bytes.byteLength, align);
            mem.getByteArrayView().set(bytes);
            return mem;
        }

        self.onmessage = function (e) {
            const msg = e.data;
            if (!msg || !msg.type) return;

            if (msg.type === 'init') {
                try {
                    const wasmBinary = msg.wasmBinary;
                    const glueCode = msg.glueCode;
                    self.Module = {
                        wasmBinary: wasmBinary,
                        instantiateWasm(info, receiveInstance) {
                            const wasmGemm = Object.fromEntries(
                                Object.entries(GEMM_MAP).map(([k, v]) => [
                                    k,
                                    (...args) => self.Module.asm[v](...args)
                                ])
                            );
                            WebAssembly.instantiate(wasmBinary, Object.assign({}, info, { wasm_gemm: wasmGemm }))
                                .then(function (output) {
                                    receiveInstance(output.instance);
                                })
                                .catch(function (err) {
                                    self.postMessage({ type: 'initError', error: String(err) });
                                });
                            return {};
                        },
                        onRuntimeInitialized() {
                            try {
                                service = new self.Module.BlockingService({ cacheSize: 4096 });
                                wasmReady = true;
                                self.postMessage({ type: 'initDone' });
                            } catch (err) {
                                self.postMessage({ type: 'initError', error: String(err) });
                            }
                        }
                    };
                    (0, eval)(glueCode);
                } catch (err) {
                    self.postMessage({ type: 'initError', error: String(err) });
                }
                return;
            }

            if (msg.type === 'loadModel') {
                const { id, pair, modelBuf, lexBuf, vocabBufs } = msg;
                try {
                    if (!wasmReady || !service) {
                        throw new Error('WASM runtime not initialized');
                    }
                    if (!models.has(pair)) {
                        const M = self.Module;
                        const modelMem = allocAligned(M, modelBuf, 256);
                        const lexMem = allocAligned(M, lexBuf, 64);
                        const vocabs = new M.AlignedMemoryList();
                        for (let i = 0; i < vocabBufs.length; i++) {
                            vocabs.push_back(allocAligned(M, vocabBufs[i], 64));
                        }
                        const modelInstance = new M.TranslationModel(
                            MODEL_CONFIG_YAML,
                            modelMem,
                            lexMem,
                            vocabs,
                            null
                        );
                        models.set(pair, modelInstance);
                    }
                    self.postMessage({ type: 'loadModelDone', id, pair });
                } catch (err) {
                    self.postMessage({ type: 'loadModelError', id, pair, error: String(err) });
                }
                return;
            }

            if (msg.type === 'translateBatch') {
                const { id, texts, pairs, isPivot } = msg;
                try {
                    if (!wasmReady || !service) {
                        throw new Error('WASM service not ready');
                    }
                    const M = self.Module;
                    const input = new M.VectorString();
                    const options = new M.VectorResponseOptions();
                    for (let i = 0; i < texts.length; i++) {
                        input.push_back(texts[i]);
                        options.push_back({ alignment: false, html: false, qualityScores: false });
                    }

                    let responses = null;
                    const out = [];

                    if (isPivot && pairs.length === 2) {
                        const firstModel = models.get(pairs[0]);
                        const secondModel = models.get(pairs[1]);
                        if (!firstModel || !secondModel) {
                            input.delete();
                            options.delete();
                            throw new Error('Pivot models not loaded: ' + pairs.join(' -> '));
                        }
                        try {
                            responses = service.translateViaPivoting(firstModel, secondModel, input, options);
                            for (let i = 0; i < texts.length; i++) {
                                out.push(responses.get(i).getTranslatedText());
                            }
                            responses.delete();
                        } catch (_) {
                            const midResp = service.translate(firstModel, input, options);
                            const midInput = new M.VectorString();
                            const midOpts = new M.VectorResponseOptions();
                            for (let i = 0; i < texts.length; i++) {
                                midInput.push_back(midResp.get(i).getTranslatedText());
                                midOpts.push_back({ alignment: false, html: false, qualityScores: false });
                            }
                            midResp.delete();
                            const finalResp = service.translate(secondModel, midInput, midOpts);
                            for (let i = 0; i < texts.length; i++) {
                                out.push(finalResp.get(i).getTranslatedText());
                            }
                            finalResp.delete();
                            midInput.delete();
                            midOpts.delete();
                        }
                    } else {
                        const model = models.get(pairs[0]);
                        if (!model) {
                            input.delete();
                            options.delete();
                            throw new Error('Model not loaded: ' + pairs[0]);
                        }
                        responses = service.translate(model, input, options);
                        for (let i = 0; i < texts.length; i++) {
                            out.push(responses.get(i).getTranslatedText());
                        }
                        responses.delete();
                    }

                    input.delete();
                    options.delete();
                    self.postMessage({ type: 'translateResult', id, translations: out });
                } catch (err) {
                    self.postMessage({ type: 'translateError', id, error: String(err) });
                }
            }
        };
    `;

    const bergamotEngine = {
        initialized: false,
        loadedPairs: new Set(),
        nextMsgId: 1,
        pendingCallbacks: new Map(),

        markManifestReady(manifest) {
            this.initialized = true;
            if (manifest && Array.isArray(manifest.models)) {
                for (let i = 0; i < manifest.models.length; i++) {
                    if (manifest.models[i] && manifest.models[i].pair) {
                        this.loadedPairs.add(manifest.models[i].pair);
                    }
                }
            }
        },

        handleBatchResult(payload) {
            if (!payload || !payload.requestId) return;
            const cb = this.pendingCallbacks.get(payload.requestId);
            if (!cb) return;
            this.pendingCallbacks.delete(payload.requestId);
            if (payload.error) {
                cb.reject(new Error(payload.error));
            } else {
                cb.resolve(payload.translations || []);
            }
        },

        async translateBatch(texts, pairs, isPivot) {
            if (!texts || texts.length === 0) {
                return [];
            }

            if (typeof window.__BERGAMOT_WASM_TRANSLATE__ === 'function') {
                return texts.map((t) => window.__BERGAMOT_WASM_TRANSLATE__(t, state.effectiveSourceLang, state.targetLang));
            }

            const requestId = 'batch_' + (this.nextMsgId++) + '_' + Date.now();
            return await new Promise((resolve, reject) => {
                this.pendingCallbacks.set(requestId, { resolve, reject });
                const sent = postTranslateIpc({
                    type: 'translateBatch',
                    requestId,
                    sourceLang: state.effectiveSourceLang,
                    targetLang: state.targetLang,
                    pairs,
                    isPivot: Boolean(isPivot),
                    texts
                });
                if (!sent) {
                    this.pendingCallbacks.delete(requestId);
                    reject(new Error('Bergamot IPC channel unavailable'));
                }
            });
        }
    };

    function isSkippableElement(el) {
        let cur = el;
        while (cur && cur.nodeType === Node.ELEMENT_NODE) {
            if (cur.id === '__take_translate_host__') return true;
            if (SKIP_TAGS.has(cur.tagName)) return true;
            if (cur.getAttribute('translate') === 'no' || cur.classList.contains('notranslate')) return true;
            if (cur.isContentEditable) return true;
            cur = cur.parentElement;
        }
        return false;
    }

    function findBlockAncestor(node) {
        let cur = node.parentElement;
        while (cur && cur !== document.body) {
            if (BLOCK_TAGS.has(cur.tagName)) {
                return cur;
            }
            cur = cur.parentElement;
        }
        return document.body;
    }

    function isValidTranslatableText(str) {
        if (!str) return false;
        const trimmed = str.trim();
        if (trimmed.length < 2) return false;
        if (/^[\d\s\p{P}\p{S}]+$/u.test(trimmed)) return false;
        if (/^(https?:\/\/|take:\/\/)/i.test(trimmed)) return false;
        return true;
    }

    function collectTranslatableBlocks(root) {
        const blockMap = new Map();
        if (!root) return blockMap;

        const walker = document.createTreeWalker(
            root,
            NodeFilter.SHOW_TEXT,
            {
                acceptNode(node) {
                    if (!isValidTranslatableText(node.nodeValue)) {
                        return NodeFilter.FILTER_REJECT;
                    }
                    if (isSkippableElement(node.parentElement)) {
                        return NodeFilter.FILTER_REJECT;
                    }
                    return NodeFilter.FILTER_ACCEPT;
                }
            }
        );

        let current;
        while ((current = walker.nextNode())) {
            if (!originalTexts.has(current)) {
                originalTexts.set(current, current.nodeValue);
            }
            const block = findBlockAncestor(current);
            let list = blockMap.get(block);
            if (!list) {
                list = [];
                blockMap.set(block, list);
            }
            list.push(current);
        }

        return blockMap;
    }

    function createBatches(nodes) {
        const batches = [];
        let currentBatch = [];
        let currentChars = 0;

        for (let i = 0; i < nodes.length; i++) {
            const node = nodes[i];
            const raw = originalTexts.get(node) ?? node.nodeValue ?? '';
            if (
                currentBatch.length >= MAX_BATCH_ITEMS ||
                (currentBatch.length > 0 && currentChars + raw.length > MAX_BATCH_CHARS)
            ) {
                batches.push(currentBatch);
                currentBatch = [];
                currentChars = 0;
            }
            currentBatch.push(node);
            currentChars += raw.length;
        }
        if (currentBatch.length > 0) {
            batches.push(currentBatch);
        }
        return batches;
    }

    function applyLeadingTrailingWhitespace(originalText, trimmedText, translatedTrimmed) {
        const idx = originalText.indexOf(trimmedText);
        if (idx === -1) return translatedTrimmed;
        const lead = originalText.slice(0, idx);
        const trail = originalText.slice(idx + trimmedText.length);
        return lead + translatedTrimmed + trail;
    }

    async function processNodeQueue(nodes, onComplete) {
        const batches = createBatches(nodes);
        const src = state.effectiveSourceLang;
        const tgt = state.targetLang;
        const pairs = state.activePairs;
        const isPivot = state.isPivot;

        for (let b = 0; b < batches.length; b++) {
            if (!state.isPageTranslated) {
                return;
            }
            const batch = batches[b];
            const uncachedNodes = [];
            const uncachedStrings = [];

            for (let i = 0; i < batch.length; i++) {
                const textNode = batch[i];
                const orig = originalTexts.get(textNode) ?? textNode.nodeValue;
                if (!orig) continue;
                const trimmed = orig.trim();
                if (!trimmed) continue;

                const cached = cacheGet(src, tgt, trimmed);
                if (cached !== null) {
                    const finalStr = applyLeadingTrailingWhitespace(orig, trimmed, cached);
                    if (textNode.nodeValue !== finalStr) {
                        textNode.nodeValue = finalStr;
                    }
                    translatedNodes.add(textNode);
                    state.translatedCount++;
                } else {
                    uncachedNodes.push({ textNode, orig, trimmed });
                    uncachedStrings.push(trimmed);
                }
            }

            if (uncachedStrings.length > 0) {
                try {
                    const translations = await bergamotEngine.translateBatch(
                        uncachedStrings,
                        pairs,
                        isPivot
                    );
                    if (!state.isPageTranslated) {
                        return;
                    }
                    await new Promise((resolve) => {
                        requestAnimationFrame(() => {
                            for (let i = 0; i < uncachedNodes.length; i++) {
                                const item = uncachedNodes[i];
                                const trText = translations[i];
                                if (typeof trText === 'string' && trText.length > 0) {
                                    cacheSet(src, tgt, item.trimmed, trText);
                                    const finalStr = applyLeadingTrailingWhitespace(
                                        item.orig,
                                        item.trimmed,
                                        trText
                                    );
                                    if (item.textNode.nodeValue !== finalStr) {
                                        item.textNode.nodeValue = finalStr;
                                    }
                                    translatedNodes.add(item.textNode);
                                    state.translatedCount++;
                                }
                            }
                            resolve();
                        });
                    });
                } catch (err) {
                    console.warn('[TakeTranslator] Batch translation error:', err);
                }
            }

            state.batchesProcessed++;
            updatePageStatusPill();
        }

        if (typeof onComplete === 'function') {
            onComplete();
        }
    }

    function executeDomPageTranslation() {
        if (!document.body || !state.isPageTranslated) return;

        const blockMap = collectTranslatableBlocks(document.body);
        const viewportNodes = [];
        const offscreenBlocks = [];
        const viewportH = window.innerHeight || 800;

        blockMap.forEach((nodes, blockEl) => {
            if (!blockEl || typeof blockEl.getBoundingClientRect !== 'function') {
                viewportNodes.push(...nodes);
                return;
            }
            const rect = blockEl.getBoundingClientRect();
            const isNearViewport = rect.bottom >= -150 && rect.top <= viewportH + 300;
            if (isNearViewport) {
                viewportNodes.push(...nodes);
            } else {
                offscreenBlocks.push({ blockEl, nodes });
            }
        });

        processNodeQueue(viewportNodes, () => {
            postTranslateIpc({
                type: 'translationFinished',
                translatedCount: state.translatedCount,
                sourceLang: state.effectiveSourceLang,
                targetLang: state.targetLang,
                isPivot: state.isPivot
            });

            if (state.intersectionObserver) {
                state.intersectionObserver.disconnect();
            }
            if ('IntersectionObserver' in window && offscreenBlocks.length > 0) {
                state.intersectionObserver = new IntersectionObserver((entries) => {
                    if (!state.isPageTranslated) return;
                    const newlyVisible = [];
                    for (let i = 0; i < entries.length; i++) {
                        const entry = entries[i];
                        if (entry.isIntersecting) {
                            const nodes = pendingBlocks.get(entry.target);
                            if (nodes) {
                                newlyVisible.push(...nodes);
                                pendingBlocks.delete(entry.target);
                            }
                            state.intersectionObserver.unobserve(entry.target);
                        }
                    }
                    if (newlyVisible.length > 0) {
                        processNodeQueue(newlyVisible);
                    }
                }, { rootMargin: '320px 0px' });

                for (let i = 0; i < offscreenBlocks.length; i++) {
                    const item = offscreenBlocks[i];
                    pendingBlocks.set(item.blockEl, item.nodes);
                    state.intersectionObserver.observe(item.blockEl);
                }
            } else if (offscreenBlocks.length > 0) {
                const rest = [];
                for (let i = 0; i < offscreenBlocks.length; i++) {
                    rest.push(...offscreenBlocks[i].nodes);
                }
                processNodeQueue(rest);
            }
        });

        setupMutationObserver();
        showPageStatusPill();
    }

    function computeDesiredPairs(src, tgt) {
        if (src === tgt) return { isPivot: false, pairs: [] };
        if (src === 'en' || tgt === 'en') {
            return { isPivot: false, pairs: [`${src}-${tgt}`] };
        }
        return { isPivot: true, pairs: [`${src}-en`, `en-${tgt}`] };
    }

    function arePairsLoadedInWorker(pairs) {
        if (typeof window.__BERGAMOT_WASM_TRANSLATE__ === 'function') {
            return true;
        }
        if (!bergamotEngine.initialized || pairs.length === 0) {
            return false;
        }
        for (let i = 0; i < pairs.length; i++) {
            if (!bergamotEngine.loadedPairs.has(pairs[i])) {
                return false;
            }
        }
        return true;
    }

    function translatePage(sourceLang, targetLang) {
        state.sourceLang = sourceLang || state.sourceLang || 'auto';
        state.targetLang = targetLang || state.targetLang || 'ru';
        state.effectiveSourceLang = resolveEffectiveSource(state.sourceLang, '', state.targetLang);
        state.isPageTranslated = true;

        const planInfo = computeDesiredPairs(state.effectiveSourceLang, state.targetLang);
        state.activePairs = planInfo.pairs;
        state.isPivot = planInfo.isPivot;

        showPageStatusPill();

        if (arePairsLoadedInWorker(planInfo.pairs)) {
            executeDomPageTranslation();
            return;
        }

        state.isDownloading = true;
        updatePageStatusPill(state.uiStrings.downloading);

        postTranslateIpc({
            type: 'ensureModels',
            action: 'page',
            requestId: 'page_' + Date.now(),
            sourceLang: state.effectiveSourceLang,
            targetLang: state.targetLang,
            url: window.location.href
        });
    }

    async function translateSelectionAsync(text, sourceLang, targetLang) {
        const trimmed = (text || '').trim();
        if (!trimmed) return '';

        const effectiveSource = resolveEffectiveSource(sourceLang, trimmed, targetLang);
        if (effectiveSource === targetLang) {
            return trimmed;
        }

        const cached = cacheGet(effectiveSource, targetLang, trimmed);
        if (cached !== null) {
            return cached;
        }

        const planInfo = computeDesiredPairs(effectiveSource, targetLang);
        if (arePairsLoadedInWorker(planInfo.pairs)) {
            const out = await bergamotEngine.translateBatch([trimmed], planInfo.pairs, planInfo.isPivot);
            const translated = out[0] || trimmed;
            cacheSet(effectiveSource, targetLang, trimmed, translated);
            return translated;
        }

        return await new Promise((resolve, reject) => {
            const requestId = 'sel_' + Date.now();
            state.pendingSelectionRequest = {
                requestId,
                text: trimmed,
                sourceLang: effectiveSource,
                targetLang,
                resolve,
                reject
            };
            const sent = postTranslateIpc({
                type: 'ensureModels',
                action: 'selection',
                requestId,
                sourceLang: effectiveSource,
                targetLang
            });
            if (!sent) {
                state.pendingSelectionRequest = null;
                reject(new Error('IPC bridge unavailable'));
            }
        });
    }

    async function onModelsReady(manifest) {
        if (!manifest || !Array.isArray(manifest.models)) return;
        try {
            state.isDownloading = false;
            state.isLoadingWasm = true;
            updatePageStatusPill(state.uiStrings.loadingWasm);
            updateSelectionResultBox(state.uiStrings.loadingWasm);

            bergamotEngine.markManifestReady(manifest);

            state.isLoadingWasm = false;
            state.effectiveSourceLang = manifest.sourceLang || state.effectiveSourceLang;
            state.targetLang = manifest.targetLang || state.targetLang;
            state.isPivot = Boolean(manifest.isPivot);
            state.activePairs = manifest.models.map((m) => m.pair);

            if (manifest.action === 'selection' && state.pendingSelectionRequest) {
                const req = state.pendingSelectionRequest;
                state.pendingSelectionRequest = null;
                const out = await bergamotEngine.translateBatch(
                    [req.text],
                    state.activePairs,
                    state.isPivot
                );
                const result = out[0] || req.text;
                cacheSet(req.sourceLang, req.targetLang, req.text, result);
                req.resolve(result);
            } else if (manifest.action === 'page' || manifest.action === 'auto_page' || state.isPageTranslated) {
                state.isPageTranslated = true;
                showPageStatusPill();
                executeDomPageTranslation();
            }
        } catch (err) {
            state.isDownloading = false;
            state.isLoadingWasm = false;
            console.error('[TakeTranslator] Failed to initialize Bergamot WASM:', err);
            updatePageStatusPill(String(err));
            if (state.pendingSelectionRequest) {
                const req = state.pendingSelectionRequest;
                state.pendingSelectionRequest = null;
                req.reject(err);
            }
        }
    }

    function onBatchResult(payload) {
        bergamotEngine.handleBatchResult(payload);
    }

    function onDownloadProgress(progress) {
        if (!progress) return;
        state.isDownloading = true;
        const pct = Math.round((progress.fraction || 0) * 100);
        const msg = `${state.uiStrings.downloading} ${progress.formattedDownloaded || ''} / ${progress.formattedTotal || ''} (${pct}%)`;
        updatePageStatusPill(msg);
        if (state.pendingSelectionRequest) {
            updateSelectionResultBox(msg);
        }
    }

    function onDownloadError(payload) {
        state.isDownloading = false;
        state.isLoadingWasm = false;
        const errMsg = (payload && payload.error) || 'Download error';
        updatePageStatusPill(errMsg);
        if (state.pendingSelectionRequest) {
            const req = state.pendingSelectionRequest;
            state.pendingSelectionRequest = null;
            req.reject(new Error(errMsg));
        }
    }

    function restorePage() {
        state.isPageTranslated = false;
        state.userRestoredOriginal = true;
        state.isDownloading = false;
        state.isLoadingWasm = false;
        if (state.intersectionObserver) {
            state.intersectionObserver.disconnect();
            state.intersectionObserver = null;
        }
        if (state.mutationObserver) {
            state.mutationObserver.disconnect();
            state.mutationObserver = null;
        }

        const nodes = Array.from(translatedNodes);
        translatedNodes.clear();
        state.translatedCount = 0;

        let idx = 0;
        function restoreStep() {
            const start = performance.now();
            while (idx < nodes.length && (performance.now() - start) < FRAME_BUDGET_MS * 2) {
                const node = nodes[idx++];
                const orig = originalTexts.get(node);
                if (orig !== undefined && node.nodeValue !== orig) {
                    node.nodeValue = orig;
                }
            }
            if (idx < nodes.length) {
                requestAnimationFrame(restoreStep);
            } else {
                updatePageStatusPill();
            }
        }
        requestAnimationFrame(restoreStep);
    }

    function setupMutationObserver() {
        if (state.mutationObserver || !('MutationObserver' in window) || !document.body) {
            return;
        }
        const pendingRoots = new Set();
        state.mutationObserver = new MutationObserver((mutations) => {
            if (!state.isPageTranslated) return;
            for (let i = 0; i < mutations.length; i++) {
                const added = mutations[i].addedNodes;
                for (let j = 0; j < added.length; j++) {
                    const node = added[j];
                    if (node.nodeType === Node.ELEMENT_NODE && node.id !== '__take_translate_host__') {
                        pendingRoots.add(node);
                    }
                }
            }
            if (pendingRoots.size > 0 && !state.mutationTimer) {
                state.mutationTimer = setTimeout(() => {
                    state.mutationTimer = null;
                    if (!state.isPageTranslated) return;
                    const batchNodes = [];
                    pendingRoots.forEach((rootEl) => {
                        const blocks = collectTranslatableBlocks(rootEl);
                        blocks.forEach((nList) => batchNodes.push(...nList));
                    });
                    pendingRoots.clear();
                    if (batchNodes.length > 0 && arePairsLoadedInWorker(state.activePairs)) {
                        processNodeQueue(batchNodes);
                    }
                }, 150);
            }
        });

        state.mutationObserver.observe(document.body, {
            childList: true,
            subtree: true
        });
    }

    // =========================================================================
    // Shadow-DOM Isolated In-Page Floating Translation UI & Selection Popup
    // =========================================================================

    let shadowRoot = null;
    let quickBtnEl = null;
    let popupCardEl = null;
    let statusPillEl = null;
    let currentSelectedText = '';
    let lastSelectionRect = null;

    function ensureShadowUi() {
        if (shadowRoot || !document.documentElement) return;

        const host = document.createElement('div');
        host.id = '__take_translate_host__';
        host.setAttribute('translate', 'no');
        host.className = 'notranslate';
        host.style.cssText = 'position:fixed;top:0;left:0;width:0;height:0;z-index:2147483647;pointer-events:none;';
        document.documentElement.appendChild(host);

        shadowRoot = host.attachShadow({ mode: 'closed' });

        const style = document.createElement('style');
        style.textContent = `
            * {
                box-sizing: border-box;
                font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            }
            .take-sel-trigger {
                position: fixed;
                pointer-events: auto;
                display: none;
                align-items: center;
                gap: 6px;
                padding: 6px 11px;
                border-radius: 999px;
                background: rgba(24, 28, 36, 0.96);
                color: #ffffff;
                border: 1px solid rgba(255, 255, 255, 0.16);
                box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
                font-size: 12px;
                font-weight: 600;
                cursor: pointer;
                user-select: none;
                backdrop-filter: blur(16px);
                -webkit-backdrop-filter: blur(16px);
                transition: transform 0.14s ease, border-color 0.14s ease;
            }
            .take-sel-trigger:hover {
                transform: translateY(-1px);
                border-color: #2997ff;
            }
            .take-sel-trigger svg {
                width: 14px;
                height: 14px;
                color: #2997ff;
            }
            .take-popup-card {
                position: fixed;
                pointer-events: auto;
                display: none;
                flex-direction: column;
                gap: 10px;
                width: 340px;
                max-width: calc(100vw - 24px);
                padding: 14px;
                border-radius: 14px;
                background: rgba(24, 28, 36, 0.97);
                color: #ffffff;
                border: 1px solid rgba(255, 255, 255, 0.14);
                box-shadow: 0 18px 42px rgba(0, 0, 0, 0.55);
                backdrop-filter: blur(20px);
                -webkit-backdrop-filter: blur(20px);
            }
            .take-popup-header {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 8px;
            }
            .take-popup-title-row {
                display: flex;
                align-items: center;
                gap: 6px;
                font-size: 12.5px;
                font-weight: 700;
                color: #ffffff;
            }
            .take-wasm-badge {
                font-size: 10px;
                font-weight: 700;
                padding: 2px 6px;
                border-radius: 999px;
                background: rgba(48, 209, 88, 0.16);
                color: #30d158;
                border: 1px solid rgba(48, 209, 88, 0.35);
            }
            .take-popup-close {
                background: transparent;
                border: none;
                color: #9ba3b4;
                cursor: pointer;
                padding: 2px 6px;
                border-radius: 6px;
                font-size: 14px;
            }
            .take-popup-close:hover {
                background: rgba(255, 255, 255, 0.1);
                color: #ffffff;
            }
            .take-lang-bar {
                display: flex;
                align-items: center;
                gap: 6px;
            }
            .take-lang-select {
                flex: 1;
                background: rgba(255, 255, 255, 0.07);
                color: #ffffff;
                border: 1px solid rgba(255, 255, 255, 0.12);
                border-radius: 8px;
                padding: 4px 8px;
                font-size: 12px;
                outline: none;
            }
            .take-Popup-result {
                background: rgba(255, 255, 255, 0.05);
                border: 1px solid rgba(255, 255, 255, 0.08);
                border-radius: 10px;
                padding: 10px;
                font-size: 13px;
                line-height: 1.45;
                color: #f0f6fc;
                max-height: 160px;
                overflow-y: auto;
                white-space: pre-wrap;
                word-break: break-word;
            }
            .take-popup-actions {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 8px;
            }
            .take-btn {
                border: 1px solid rgba(255, 255, 255, 0.12);
                background: rgba(255, 255, 255, 0.07);
                color: #ffffff;
                border-radius: 8px;
                padding: 6px 10px;
                font-size: 11.5px;
                font-weight: 600;
                cursor: pointer;
                transition: background 0.14s ease;
            }
            .take-btn:hover {
                background: rgba(255, 255, 255, 0.14);
            }
            .take-btn-primary {
                background: #007aff;
                border-color: #007aff;
            }
            .take-btn-primary:hover {
                background: #2997ff;
            }
            .take-status-pill,
            .take-status-pill.take-light {
                --pill-bg: #ffffff;
                --pill-color: #1f2328;
                --pill-border: rgba(0, 0, 0, 0.14);
                --pill-shadow: 0 4px 20px rgba(0, 0, 0, 0.15), 0 1px 4px rgba(0, 0, 0, 0.08);
                --btn-bg: #f3f4f6;
                --btn-color: #1f2328;
                --btn-border: rgba(0, 0, 0, 0.12);
                --btn-hover: #e5e7eb;
                --badge-bg: rgba(46, 160, 67, 0.14);
                --badge-color: #1a7f37;
                --badge-border: rgba(46, 160, 67, 0.32);
                --close-color: #57606a;
                --close-hover: #1f2328;
                --close-hover-bg: rgba(0, 0, 0, 0.08);
            }
            .take-status-pill.take-dark {
                --pill-bg: rgba(24, 28, 36, 0.94);
                --pill-color: #ffffff;
                --pill-border: rgba(255, 255, 255, 0.15);
                --pill-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
                --btn-bg: rgba(255, 255, 255, 0.10);
                --btn-color: #ffffff;
                --btn-border: rgba(255, 255, 255, 0.15);
                --btn-hover: rgba(255, 255, 255, 0.18);
                --badge-bg: rgba(48, 209, 88, 0.16);
                --badge-color: #30d158;
                --badge-border: rgba(48, 209, 88, 0.35);
                --close-color: #9ba3b4;
                --close-hover: #ffffff;
                --close-hover-bg: rgba(255, 255, 255, 0.10);
            }
            .take-status-pill {
                position: fixed;
                bottom: 16px;
                right: 16px;
                pointer-events: auto;
                display: none;
                align-items: center;
                gap: 8px;
                padding: 6px 12px;
                border-radius: 999px;
                background: var(--pill-bg);
                color: var(--pill-color);
                border: 1px solid var(--pill-border);
                box-shadow: var(--pill-shadow);
                font-size: 12px;
                font-weight: 500;
                backdrop-filter: blur(16px);
                -webkit-backdrop-filter: blur(16px);
                z-index: 2147483647;
                transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
            }
            .take-status-pill .take-wasm-badge {
                font-size: 10px;
                font-weight: 700;
                padding: 2px 6px;
                border-radius: 999px;
                background: var(--badge-bg);
                color: var(--badge-color);
                border: 1px solid var(--badge-border);
            }
            .take-status-pill .take-btn {
                background: var(--btn-bg);
                color: var(--btn-color);
                border: 1px solid var(--btn-border);
                border-radius: 8px;
                padding: 4px 10px;
                font-size: 11.5px;
                font-weight: 600;
                cursor: pointer;
                transition: background 0.14s ease, border-color 0.14s ease;
            }
            .take-status-pill .take-btn:hover {
                background: var(--btn-hover);
            }
            .take-status-pill .take-pill-close {
                background: transparent;
                border: none;
                color: var(--close-color);
                cursor: pointer;
                padding: 2px 6px;
                border-radius: 6px;
                font-size: 13px;
                transition: background 0.14s ease, color 0.14s ease;
            }
            .take-status-pill .take-pill-close:hover {
                background: var(--close-hover-bg);
                color: var(--close-hover);
            }
        `;
        shadowRoot.appendChild(style);

        quickBtnEl = document.createElement('button');
        quickBtnEl.className = 'take-sel-trigger';
        quickBtnEl.innerHTML = `
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M5 8l6 6"></path><path d="M4 14l6-6 2-3"></path><path d="M2 5h12"></path><path d="M7 2h1"></path>
                <path d="M22 22l-5-10-5 10"></path><path d="M14 18h6"></path>
            </svg>
            <span class="take-sel-label">Перевести</span>
        `;
        quickBtnEl.addEventListener('mousedown', (e) => {
            e.preventDefault();
            e.stopPropagation();
            openSelectionPopupCard();
        });
        shadowRoot.appendChild(quickBtnEl);

        popupCardEl = document.createElement('div');
        popupCardEl.className = 'take-popup-card';
        popupCardEl.innerHTML = `
            <div class="take-popup-header">
                <div class="take-popup-title-row">
                    <span class="take-title-text">${state.uiStrings.title}</span>
                    <span class="take-wasm-badge">${state.uiStrings.badge}</span>
                </div>
                <button class="take-popup-close" title="${state.uiStrings.close}">✕</button>
            </div>
            <div class="take-lang-bar">
                <select class="take-lang-select take-src-select">
                    <option value="auto">Auto</option>
                    <option value="en">English (EN)</option>
                    <option value="ru">Русский (RU)</option>
                    <option value="de">Deutsch (DE)</option>
                    <option value="fr">Français (FR)</option>
                    <option value="es">Español (ES)</option>
                    <option value="zh">中文 (ZH)</option>
                    <option value="ja">日本語 (JA)</option>
                </select>
                <span>→</span>
                <select class="take-lang-select take-tgt-select">
                    <option value="ru">Русский (RU)</option>
                    <option value="en">English (EN)</option>
                    <option value="de">Deutsch (DE)</option>
                    <option value="fr">Français (FR)</option>
                    <option value="es">Español (ES)</option>
                    <option value="zh">中文 (ZH)</option>
                    <option value="ja">日本語 (JA)</option>
                </select>
            </div>
            <div class="take-Popup-result"></div>
            <div class="take-popup-actions">
                <button class="take-btn take-copy-btn">${state.uiStrings.copy}</button>
                <button class="take-btn take-btn-primary take-full-page-btn">${state.uiStrings.translatePage}</button>
            </div>
        `;
        popupCardEl.addEventListener('mousedown', (e) => e.stopPropagation());

        const closeBtn = popupCardEl.querySelector('.take-popup-close');
        const srcSelect = popupCardEl.querySelector('.take-src-select');
        const tgtSelect = popupCardEl.querySelector('.take-tgt-select');
        const copyBtn = popupCardEl.querySelector('.take-copy-btn');
        const fullPageBtn = popupCardEl.querySelector('.take-full-page-btn');
        const resultBox = popupCardEl.querySelector('.take-Popup-result');

        closeBtn.addEventListener('click', hideSelectionUi);

        function refreshSelectionTranslation() {
            state.sourceLang = srcSelect.value;
            state.targetLang = tgtSelect.value;
            resultBox.textContent = '…';
            translateSelectionAsync(currentSelectedText, state.sourceLang, state.targetLang)
                .then((out) => {
                    resultBox.textContent = out;
                })
                .catch((err) => {
                    resultBox.textContent = String(err.message || err);
                });
        }

        srcSelect.addEventListener('change', refreshSelectionTranslation);
        tgtSelect.addEventListener('change', refreshSelectionTranslation);

        copyBtn.addEventListener('click', () => {
            if (navigator.clipboard && resultBox.textContent) {
                navigator.clipboard.writeText(resultBox.textContent).catch(() => {});
            }
            copyBtn.textContent = state.uiStrings.copied;
            setTimeout(() => {
                copyBtn.textContent = state.uiStrings.copy;
            }, 1200);
        });

        fullPageBtn.addEventListener('click', () => {
            hideSelectionUi();
            translatePage(srcSelect.value, tgtSelect.value);
        });

        shadowRoot.appendChild(popupCardEl);

        statusPillEl = document.createElement('div');
        statusPillEl.className = 'take-status-pill';
        statusPillEl.innerHTML = `
            <span class="take-wasm-badge">WASM</span>
            <span class="take-status-text"></span>
            <button class="take-btn take-toggle-orig-btn"></button>
            <button class="take-popup-close take-pill-close">✕</button>
        `;
        const toggleOrigBtn = statusPillEl.querySelector('.take-toggle-orig-btn');
        const pillClose = statusPillEl.querySelector('.take-pill-close');

        toggleOrigBtn.addEventListener('click', () => {
            if (state.isPageTranslated) {
                restorePage();
            } else {
                translatePage(state.sourceLang, state.targetLang);
            }
        });
        pillClose.addEventListener('click', () => {
            statusPillEl.style.display = 'none';
        });

        shadowRoot.appendChild(statusPillEl);
    }

    function updateSelectionResultBox(text) {
        if (!popupCardEl) return;
        const resultBox = popupCardEl.querySelector('.take-Popup-result');
        if (resultBox) {
            resultBox.textContent = text;
        }
    }

    function isPageLight() {
        try {
            if (document.body) {
                const bodyStyle = window.getComputedStyle(document.body);
                const bg = bodyStyle.backgroundColor;
                if (bg && bg !== 'transparent' && bg !== 'rgba(0, 0, 0, 0)') {
                    const m = bg.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
                    if (m) {
                        const luma = 0.299 * parseInt(m[1], 10) + 0.587 * parseInt(m[2], 10) + 0.114 * parseInt(m[3], 10);
                        return luma > 130;
                    }
                }
            }
            if (document.documentElement) {
                const docStyle = window.getComputedStyle(document.documentElement);
                const bg = docStyle.backgroundColor;
                if (bg && bg !== 'transparent' && bg !== 'rgba(0, 0, 0, 0)') {
                    const m = bg.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
                    if (m) {
                        const luma = 0.299 * parseInt(m[1], 10) + 0.587 * parseInt(m[2], 10) + 0.114 * parseInt(m[3], 10);
                        return luma > 130;
                    }
                }
            }
        } catch (_) {}
        const docTheme = document.documentElement ? document.documentElement.getAttribute('data-theme') : null;
        if (docTheme === 'dark') return false;
        if (docTheme === 'light') return true;
        return true;
    }

    function syncTheme() {
        if (!statusPillEl) return;
        if (isPageLight()) {
            statusPillEl.classList.add('take-light');
            statusPillEl.classList.remove('take-dark');
        } else {
            statusPillEl.classList.add('take-dark');
            statusPillEl.classList.remove('take-light');
        }
    }

    function showPageStatusPill() {
        ensureShadowUi();
        if (!statusPillEl) return;
        syncTheme();
        statusPillEl.style.display = 'inline-flex';
        updatePageStatusPill();
    }

    function updatePageStatusPill(customMessage) {
        if (!statusPillEl) return;
        syncTheme();
        const txt = statusPillEl.querySelector('.take-status-text');
        const btn = statusPillEl.querySelector('.take-toggle-orig-btn');
        if (customMessage) {
            txt.textContent = customMessage;
            btn.textContent = state.uiStrings.restorePage;
            return;
        }
        if (state.isPageTranslated) {
            const pivotSuffix = state.isPivot ? ' • Pivot EN' : '';
            txt.textContent = `${state.effectiveSourceLang.toUpperCase()} → ${state.targetLang.toUpperCase()}${pivotSuffix} (${state.translatedCount})`;
            btn.textContent = state.uiStrings.restorePage;
        } else {
            txt.textContent = state.uiStrings.restorePage;
            btn.textContent = state.uiStrings.translatePage;
        }
    }

    function hideSelectionUi() {
        if (quickBtnEl) quickBtnEl.style.display = 'none';
        if (popupCardEl) popupCardEl.style.display = 'none';
    }

    function openSelectionPopupCard() {
        ensureShadowUi();
        if (!popupCardEl || !currentSelectedText) return;
        if (quickBtnEl) quickBtnEl.style.display = 'none';

        const srcSelect = popupCardEl.querySelector('.take-src-select');
        const tgtSelect = popupCardEl.querySelector('.take-tgt-select');
        const resultBox = popupCardEl.querySelector('.take-Popup-result');

        srcSelect.value = state.sourceLang;
        tgtSelect.value = state.targetLang;
        resultBox.textContent = '…';

        const rect = lastSelectionRect || { left: 24, bottom: 60, top: 40 };
        const top = Math.min(window.innerHeight - 230, Math.max(12, rect.bottom + 8));
        const left = Math.min(window.innerWidth - 356, Math.max(12, rect.left));

        popupCardEl.style.top = `${top}px`;
        popupCardEl.style.left = `${left}px`;
        popupCardEl.style.display = 'flex';

        translateSelectionAsync(currentSelectedText, state.sourceLang, state.targetLang)
            .then((out) => {
                resultBox.textContent = out;
            })
            .catch((err) => {
                resultBox.textContent = String(err.message || err);
            });
    }

    document.addEventListener('mouseup', () => {
        if (!state.selectionPopupEnabled) return;
        if (window.location.protocol === 'take:') return;

        setTimeout(() => {
            const sel = window.getSelection();
            const text = sel ? sel.toString().trim() : '';
            if (!text || text.length < 2 || text.length > 1200) {
                if (quickBtnEl) quickBtnEl.style.display = 'none';
                return;
            }
            if (sel.rangeCount === 0) return;
            const range = sel.getRangeAt(0);
            const containerEl = range.commonAncestorContainer.nodeType === Node.ELEMENT_NODE
                ? range.commonAncestorContainer
                : range.commonAncestorContainer.parentElement;
            if (isSkippableElement(containerEl)) return;

            currentSelectedText = text;
            lastSelectionRect = range.getBoundingClientRect();
            ensureShadowUi();
            if (!quickBtnEl) return;

            const top = Math.min(window.innerHeight - 44, Math.max(8, lastSelectionRect.bottom + 6));
            const left = Math.min(window.innerWidth - 130, Math.max(8, lastSelectionRect.left));
            quickBtnEl.style.top = `${top}px`;
            quickBtnEl.style.left = `${left}px`;
            quickBtnEl.style.display = 'inline-flex';
        }, 20);
    }, { passive: true });

    document.addEventListener('mousedown', (e) => {
        if (e.target && e.target.id === '__take_translate_host__') return;
        hideSelectionUi();
    }, { passive: true });

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            hideSelectionUi();
        }
    }, { passive: true });

    function checkAndAutoTranslate(targetLang) {
        targetLang = targetLang || state.targetLang || 'ru';
        if (state.isPageTranslated || state.userRestoredOriginal) {
            return;
        }

        function runAutoCheck() {
            if (state.isPageTranslated || state.userRestoredOriginal) return;
            const detected = detectPageLanguage();
            state.effectiveSourceLang = detected;
            if (detected && detected !== targetLang) {
                translatePage(detected, targetLang);
            }
        }

        runAutoCheck();
        if (!state.isPageTranslated && !state.userRestoredOriginal) {
            setTimeout(runAutoCheck, 200);
            setTimeout(runAutoCheck, 600);
        }
    }

    window.__TakeTranslator = {
        translatePage,
        restorePage,
        checkAndAutoTranslate,
        translateText: translateSelectionAsync,
        openSelectionPopup: openSelectionPopupCard,
        onModelsReady,
        onBatchResult,
        onDownloadProgress,
        onDownloadError,
        setConfig(cfg) {
            if (!cfg) return;
            if (cfg.sourceLang) state.sourceLang = cfg.sourceLang;
            if (cfg.targetLang) state.targetLang = cfg.targetLang;
            if (typeof cfg.selectionPopupEnabled === 'boolean') {
                state.selectionPopupEnabled = cfg.selectionPopupEnabled;
                if (!state.selectionPopupEnabled) hideSelectionUi();
            }
            if (cfg.uiStrings) {
                Object.assign(state.uiStrings, cfg.uiStrings);
            }
        },
        getStats() {
            return {
                isPageTranslated: state.isPageTranslated,
                isDownloading: state.isDownloading,
                isLoadingWasm: state.isLoadingWasm,
                translatedCount: state.translatedCount,
                cacheHits: state.cacheHits,
                cacheSize: translationCache.size,
                batchesProcessed: state.batchesProcessed,
                sourceLang: state.sourceLang,
                effectiveSourceLang: state.effectiveSourceLang,
                targetLang: state.targetLang,
                isPivot: state.isPivot,
                activePairs: state.activePairs,
                loadedPairs: Array.from(bergamotEngine.loadedPairs)
            };
        },
        detectPageLanguage: notifyPageLanguage
    };

    function notifyPageLanguage() {
        if (!document.body && !document.documentElement) return;
        const detected = detectPageLanguage();
        state.effectiveSourceLang = detected;
        postTranslateIpc({
            type: 'pageLanguageDetected',
            lang: detected,
            url: window.location.href
        });
    }

    if (document.readyState === 'complete') {
        notifyPageLanguage();
    } else {
        window.addEventListener('load', () => setTimeout(notifyPageLanguage, 80));
        document.addEventListener('DOMContentLoaded', () => setTimeout(notifyPageLanguage, 100));
        setTimeout(notifyPageLanguage, 250);
    }

    window.addEventListener('popstate', () => {
        state.userRestoredOriginal = false;
        setTimeout(notifyPageLanguage, 200);
    });
})();
