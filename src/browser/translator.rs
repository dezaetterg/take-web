use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::prelude::*;
use serde::{Deserialize, Serialize};
use soup::prelude::*;

use crate::i18n::tr;

pub const PAGE_TRANSLATOR_SCRIPT: &str = include_str!("../../resources/scripts/page_translator.js");

pub const BERGAMOT_SCHEME: &str = "take-bergamot";

pub const SUPPORTED_SOURCE_LANGS: &[(&str, &str)] = &[
    ("auto", "translate_lang_auto"),
    ("en", "translate_lang_en"),
    ("ru", "translate_lang_ru"),
    ("de", "translate_lang_de"),
    ("fr", "translate_lang_fr"),
    ("es", "translate_lang_es"),
    ("zh", "translate_lang_zh"),
    ("ja", "translate_lang_ja"),
];

pub const SUPPORTED_TARGET_LANGS: &[(&str, &str)] = &[
    ("ru", "translate_lang_ru"),
    ("en", "translate_lang_en"),
    ("de", "translate_lang_de"),
    ("fr", "translate_lang_fr"),
    ("es", "translate_lang_es"),
    ("zh", "translate_lang_zh"),
    ("ja", "translate_lang_ja"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BergamotFileRole {
    WasmGlue,
    WasmBinary,
    Model,
    Lex,
    Vocab,
    SrcVocab,
    TrgVocab,
}

#[derive(Debug, Clone, Copy)]
pub struct BergamotAsset {
    pub role: BergamotFileRole,
    pub filename: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    pub primary_url: &'static str,
    pub fallback_url: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct BergamotModelSpec {
    pub pair: &'static str,
    pub source_lang: &'static str,
    pub target_lang: &'static str,
    pub version: &'static str,
    pub files: &'static [BergamotAsset],
}

impl BergamotModelSpec {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

pub const WASM_RUNTIME_ASSETS: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::WasmGlue,
        filename: "bergamot-translator-worker.js",
        size: 80_474,
        sha256: "748b2418418a2ffc6e70721aeb10098d8e6fb589ea156b91f4ae8bc3490d8f7a",
        primary_url: "https://cdn.jsdelivr.net/npm/@browsermt/bergamot-translator@0.4.9/worker/bergamot-translator-worker.js",
        fallback_url: "https://unpkg.com/@browsermt/bergamot-translator@0.4.9/worker/bergamot-translator-worker.js",
    },
    BergamotAsset {
        role: BergamotFileRole::WasmBinary,
        filename: "bergamot-translator-worker.wasm",
        size: 5_174_294,
        sha256: "95a2b58dd6773bf1b3f345d71f9149928b9f75f4ec9c9064c0b3e42c298671b2",
        primary_url: "https://cdn.jsdelivr.net/npm/@browsermt/bergamot-translator@0.4.9/worker/bergamot-translator-worker.wasm",
        fallback_url: "https://unpkg.com/@browsermt/bergamot-translator@0.4.9/worker/bergamot-translator-worker.wasm",
    },
];

const EN_RU_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.enru.intgemm.alphas.bin",
        size: 17_141_051,
        sha256: "01db60f75a54ef39c617cbdfdd78759a6a216576efa9a9088da8a6433d90af77",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0ed2208e-01d5-4251-9fcd-5ad2167ba456.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0ed2208e-01d5-4251-9fcd-5ad2167ba456.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.enru.s2t.bin",
        size: 2_768_468,
        sha256: "3587f93c10c1d457f874c2ba7ff4d5e2686fdaf7f3fd179cfefeacab3c6990b9",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0a9c0bc3-19a7-4b0a-9f9c-f8494a0e59f9.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0a9c0bc3-19a7-4b0a-9f9c-f8494a0e59f9.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.enru.spm",
        size: 904_455,
        sha256: "56ee63e14e8cb926c394242adc3ed7cc602644c3d33058cff2ce2959d52a6258",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/d410fe5f-3f5d-4af6-9841-ca55beab47f7.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/d410fe5f-3f5d-4af6-9841-ca55beab47f7.spm",
    },
];

const RU_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.ruen.intgemm.alphas.bin",
        size: 17_141_051,
        sha256: "b1d85c13cfbb05e1d326dd6f0fb5ef270a2011b547450260f96567a93f446c94",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/466ccac8-f8c7-4fee-81ee-477a1f817f1c.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/466ccac8-f8c7-4fee-81ee-477a1f817f1c.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.ruen.s2t.bin",
        size: 4_483_844,
        sha256: "f654693577505fd38b1f3d220cdd4ffffbb45afb900a60cf751f0724eadc74e0",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/fe5156a8-ab98-4597-84c5-f1248eda4770.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/fe5156a8-ab98-4597-84c5-f1248eda4770.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.ruen.spm",
        size: 905_257,
        sha256: "93bdc941b16e523695c319f74778bca9fd8b75a25ad75020cdc98aef74cdc0fc",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/40916383-0537-4869-992a-6606ee0cfb97.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/40916383-0537-4869-992a-6606ee0cfb97.spm",
    },
];

const EN_DE_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.ende.intgemm.alphas.bin",
        size: 17_140_835,
        sha256: "b3e980d6602ab0bdfe8d9315cb5fc282a16bb1c8dccf38e70945c584551c4318",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/8a830d1c-e167-4238-8473-4065c65c02d2.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/8a830d1c-e167-4238-8473-4065c65c02d2.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.ende.s2t.bin",
        size: 3_943_644,
        sha256: "f03eb8245042feb7a5800815b8d0dc215d7a60691632405b65c461d250cedbe6",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/db204e54-8565-4bba-aa52-1e857bcc68a5.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/db204e54-8565-4bba-aa52-1e857bcc68a5.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.ende.spm",
        size: 784_269,
        sha256: "417668f2ed297970febafb5b079a9d5ebc4ed0b3550ac8386d67a90473a09bd7",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/a5117ec6-0b87-4ae1-9c2e-5151de59af1d.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/a5117ec6-0b87-4ae1-9c2e-5151de59af1d.spm",
    },
];

const DE_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.deen.intgemm.alphas.bin",
        size: 17_140_837,
        sha256: "1980225d00dc5645491777accff5b3c9d20b92eff67a25135f1cf8fe2ed8fb8f",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/8e3df11a-c2fc-422a-bc2d-ae234dab44c9.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/8e3df11a-c2fc-422a-bc2d-ae234dab44c9.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.deen.s2t.bin",
        size: 5_047_568,
        sha256: "2f7c0f7bbce97ae5b52454074a892ba7b7610fb98e3c5d341e4ca79f0850c4de",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e2f7beb4-b45d-49da-b83e-ff421ad1268c.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e2f7beb4-b45d-49da-b83e-ff421ad1268c.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.deen.spm",
        size: 784_269,
        sha256: "417668f2ed297970febafb5b079a9d5ebc4ed0b3550ac8386d67a90473a09bd7",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/9fb51529-8473-4138-a9e0-0a588ab574f0.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/9fb51529-8473-4138-a9e0-0a588ab574f0.spm",
    },
];

const EN_FR_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.enfr.intgemm.alphas.bin",
        size: 17_140_961,
        sha256: "0678019c4d74c8c81d2de17e3e58d3aba5f5eb48f5595d9240c17f69d30461de",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/762a23ff-317e-4f0d-95ff-e9d95674a521.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/762a23ff-317e-4f0d-95ff-e9d95674a521.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.enfr.s2t.bin",
        size: 7_886_500,
        sha256: "38fb44bad1fd5f1e6bfdcf15cc8baa09d61aad2a4f9c587914e24e7b5c25c32c",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/7178ceaa-3ae6-49cb-a7f0-a72bdded2d9b.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/7178ceaa-3ae6-49cb-a7f0-a72bdded2d9b.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.enfr.spm",
        size: 831_382,
        sha256: "4c84b95b62c930f0791466d73eb996841eef474c96d0c2f0e6c6d80640f2005a",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/808114a9-eff2-47c3-a981-b7119105c40c.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/808114a9-eff2-47c3-a981-b7119105c40c.spm",
    },
];

const FR_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.fren.intgemm.alphas.bin",
        size: 17_140_961,
        sha256: "185f76d24c2d400fe4ea0cb2487df77722641b97a3ef10633872e8a7fdf40e09",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e8e44aae-f20c-48c7-89c8-dade397a5858.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e8e44aae-f20c-48c7-89c8-dade397a5858.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.fren.s2t.bin",
        size: 8_818_768,
        sha256: "3148abf21ea98a4d69d0e4504e0d68a6c060204a9b9a39b76855aee1d5b2c8ea",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/75287f2c-8f15-4045-a435-ec1b9394d973.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/75287f2c-8f15-4045-a435-ec1b9394d973.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.fren.spm",
        size: 831_382,
        sha256: "4c84b95b62c930f0791466d73eb996841eef474c96d0c2f0e6c6d80640f2005a",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/3096e2ad-89fe-4f9b-aac7-57f011bd4c49.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/3096e2ad-89fe-4f9b-aac7-57f011bd4c49.spm",
    },
];

const EN_ES_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.enes.intgemm.alphas.bin",
        size: 17_140_755,
        sha256: "fa7460037a3163e03fe1d23602f964bff2331da6ee813637e092ddf37156ef53",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/50fbae09-5219-4393-ad75-28b23f44a17d.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/50fbae09-5219-4393-ad75-28b23f44a17d.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.enes.s2t.bin",
        size: 3_347_104,
        sha256: "3a113d713dec3cf1d12bba5b138ae616e28bba4bbc7fe7fd39ba145e26b86d7f",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/d2d8de5b-b41e-4eb0-b978-dd48725e3e77.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/d2d8de5b-b41e-4eb0-b978-dd48725e3e77.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.enes.spm",
        size: 825_463,
        sha256: "909b1eea1face0d7f90a474fe29a8c0fef8d104b6e41e65616f864c964ba8845",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e9c774ba-69ff-4951-8783-895aeeb05439.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/e9c774ba-69ff-4951-8783-895aeeb05439.spm",
    },
];

const ES_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.esen.intgemm.alphas.bin",
        size: 17_140_755,
        sha256: "4b6b7f451094aaa447d012658af158ffc708fc8842dde2f871a58404f5457fe0",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/9ee26e91-9b52-44ba-8d30-c0230dd587b2.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/9ee26e91-9b52-44ba-8d30-c0230dd587b2.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.esen.s2t.bin",
        size: 3_860_888,
        sha256: "f11a2c23ef85ab1fee1c412b908d69bc20d66fd59faa8f7da5a5f0347eddf969",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/54c5d8e2-3900-4a31-860e-8a18f7d028b5.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/54c5d8e2-3900-4a31-860e-8a18f7d028b5.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.esen.spm",
        size: 825_463,
        sha256: "909b1eea1face0d7f90a474fe29a8c0fef8d104b6e41e65616f864c964ba8845",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/cab5e093-7b55-47ea-a247-9747cc0109e3.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/cab5e093-7b55-47ea-a247-9747cc0109e3.spm",
    },
];

const EN_ZH_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.enzh.intgemm.alphas.bin",
        size: 42_992_955,
        sha256: "ce4486f728641a36269a245248dcb53405e76d96d9eba68dcb4172f29521e092",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/ff689b9c-0ab6-4562-bfa5-6902a94523bd.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/ff689b9c-0ab6-4562-bfa5-6902a94523bd.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.enzh.s2t.bin",
        size: 6_506_248,
        sha256: "4a5e5827788060f1d718a8132b69440929387514a045796e9b77f935db68c055",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/3719cb80-29be-4a9d-b5c0-25500c95f822.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/3719cb80-29be-4a9d-b5c0-25500c95f822.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::SrcVocab,
        filename: "srcvocab.enzh.spm",
        size: 806_952,
        sha256: "bd9b65504acc6d9726dd281f7defc2adb7c2c22d0688fe2f84697de25197c8c5",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/57b4392b-58f1-448f-b968-857e99e1a501.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/57b4392b-58f1-448f-b968-857e99e1a501.spm",
    },
    BergamotAsset {
        role: BergamotFileRole::TrgVocab,
        filename: "trgvocab.enzh.spm",
        size: 772_004,
        sha256: "aded6993c36e440284d11cec3f6b8aef9c0e43188a772d80be342a713adf223d",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0c8d28e9-8bbd-4570-8cf6-1188fee24d2b.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/0c8d28e9-8bbd-4570-8cf6-1188fee24d2b.spm",
    },
];

const ZH_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.zhen.intgemm.alphas.bin",
        size: 25_461_051,
        sha256: "4465041a02ecc3976f899bc2ca73181c5a3e3b286c672a72da37972262a08f87",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/f669d62a-12c8-46fa-b029-0f047fe6afc6.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/f669d62a-12c8-46fa-b029-0f047fe6afc6.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.zhen.s2t.bin",
        size: 7_556_580,
        sha256: "37e2f2feb3382155cbb750dd17d925f28a73c9ca2cd9d246f977c2b1c1928f1b",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/f22c4dcb-85b2-442a-bfe0-096d207c7bea.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/f22c4dcb-85b2-442a-bfe0-096d207c7bea.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.zhen.spm",
        size: 1_332_993,
        sha256: "81e49ac79fd15d0cf95a0c6d190f8f77815c914598759ef9b2952048bb837506",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/1252ca3b-1520-4b62-891c-3652cab82637.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/1252ca3b-1520-4b62-891c-3652cab82637.spm",
    },
];

const EN_JA_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.enja.intgemm.alphas.bin",
        size: 43_849_787,
        sha256: "59ae659f9bb63e4f81f474fe3c03d3f4499434b5f9e779fab7c12a45f31fd562",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/4e19636e-4644-4072-8a05-9ef695d4a3ae.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/4e19636e-4644-4072-8a05-9ef695d4a3ae.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.enja.s2t.bin",
        size: 4_128_360,
        sha256: "edfb7eb47b98a2689b804ab3614c31b24f1257aa54b1154da3d85e1ae8152d9f",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/87258444-8fac-4d3c-a175-9275e9d10ffa.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/87258444-8fac-4d3c-a175-9275e9d10ffa.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::SrcVocab,
        filename: "srcvocab.enja.spm",
        size: 796_275,
        sha256: "970c98d174fc01e0339fbabbf45af36a4be3f26f819ec1a5ea1189f71e091889",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/cdea5422-fe0d-4941-a653-ca090423e8f8.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/cdea5422-fe0d-4941-a653-ca090423e8f8.spm",
    },
    BergamotAsset {
        role: BergamotFileRole::TrgVocab,
        filename: "trgvocab.enja.spm",
        size: 827_144,
        sha256: "3b3d9f8f3a034d98d0a476f1794fa79c01e4e98a967ceb6777a66ba2d03ec1e1",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/5c1a663f-5736-4100-b681-c6737fe79cb5.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/5c1a663f-5736-4100-b681-c6737fe79cb5.spm",
    },
];

const JA_EN_FILES: &[BergamotAsset] = &[
    BergamotAsset {
        role: BergamotFileRole::Model,
        filename: "model.jaen.intgemm.alphas.bin",
        size: 43_977_787,
        sha256: "3a603e20bfe1be86071913f9e23ab5129075bc0a8490151020ac4821e4f17302",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/2b066368-11b5-4cee-92e8-e6156b334f80.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/2b066368-11b5-4cee-92e8-e6156b334f80.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Lex,
        filename: "lex.50.50.jaen.s2t.bin",
        size: 9_348_172,
        sha256: "525f412f0d210536c2933c78ae395fa0bf2b5ee6cc5dda61ebc2e79410ebaee4",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/4e6b3270-8a2f-491d-b717-f99749328622.bin",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/4e6b3270-8a2f-491d-b717-f99749328622.bin",
    },
    BergamotAsset {
        role: BergamotFileRole::Vocab,
        filename: "vocab.jaen.spm",
        size: 1_443_222,
        sha256: "5cb217758bae05877bb3f0c2f612e4e7c1e4cb03c10db11f4a47098d7ae62919",
        primary_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/6f4898e3-ebaa-4a79-a1af-93a4f65b96fa.spm",
        fallback_url: "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models/6f4898e3-ebaa-4a79-a1af-93a4f65b96fa.spm",
    },
];

pub const MODEL_CATALOG: &[BergamotModelSpec] = &[
    BergamotModelSpec {
        pair: "en-ru",
        source_lang: "en",
        target_lang: "ru",
        version: "1.0a2",
        files: EN_RU_FILES,
    },
    BergamotModelSpec {
        pair: "ru-en",
        source_lang: "ru",
        target_lang: "en",
        version: "1.0a2",
        files: RU_EN_FILES,
    },
    BergamotModelSpec {
        pair: "en-de",
        source_lang: "en",
        target_lang: "de",
        version: "1.0",
        files: EN_DE_FILES,
    },
    BergamotModelSpec {
        pair: "de-en",
        source_lang: "de",
        target_lang: "en",
        version: "1.0",
        files: DE_EN_FILES,
    },
    BergamotModelSpec {
        pair: "en-fr",
        source_lang: "en",
        target_lang: "fr",
        version: "1.0",
        files: EN_FR_FILES,
    },
    BergamotModelSpec {
        pair: "fr-en",
        source_lang: "fr",
        target_lang: "en",
        version: "1.0",
        files: FR_EN_FILES,
    },
    BergamotModelSpec {
        pair: "en-es",
        source_lang: "en",
        target_lang: "es",
        version: "1.0",
        files: EN_ES_FILES,
    },
    BergamotModelSpec {
        pair: "es-en",
        source_lang: "es",
        target_lang: "en",
        version: "1.0",
        files: ES_EN_FILES,
    },
    BergamotModelSpec {
        pair: "en-zh",
        source_lang: "en",
        target_lang: "zh",
        version: "2.0",
        files: EN_ZH_FILES,
    },
    BergamotModelSpec {
        pair: "zh-en",
        source_lang: "zh",
        target_lang: "en",
        version: "2.0a1",
        files: ZH_EN_FILES,
    },
    BergamotModelSpec {
        pair: "en-ja",
        source_lang: "en",
        target_lang: "ja",
        version: "2.3",
        files: EN_JA_FILES,
    },
    BergamotModelSpec {
        pair: "ja-en",
        source_lang: "ja",
        target_lang: "en",
        version: "2.1",
        files: JA_EN_FILES,
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPlan {
    pub source_lang: String,
    pub target_lang: String,
    pub is_pivot: bool,
    pub pairs: Vec<String>,
    pub total_bytes: u64,
    pub missing_bytes: u64,
}

impl TranslationPlan {
    pub fn is_ready_on_disk(&self) -> bool {
        self.missing_bytes == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModelInfo {
    pub pair: String,
    pub source_lang: String,
    pub target_lang: String,
    pub version: String,
    pub size_bytes: u64,
    pub formatted_size: String,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub current_file: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateDownloadError {
    Cancelled,
    NetworkError(String),
    ValidationFailed(String),
    IoError(String),
}

impl std::fmt::Display for TranslateDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "{}", tr("translate_status_cancelled")),
            Self::NetworkError(msg) => write!(f, "{}: {msg}", tr("translate_status_error_network")),
            Self::ValidationFailed(msg) => {
                write!(f, "{}: {msg}", tr("translate_status_error_corrupt"))
            }
            Self::IoError(msg) => write!(f, "I/O: {msg}"),
        }
    }
}

pub fn bergamot_models_dir() -> PathBuf {
    let mut dir = glib::user_data_dir();
    dir.push("takeweb");
    dir.push("models");
    dir.push("bergamot");
    dir
}

pub fn wasm_runtime_dir() -> PathBuf {
    bergamot_models_dir().join("wasm")
}

pub fn model_pair_dir(pair: &str) -> PathBuf {
    bergamot_models_dir().join(pair)
}

pub fn find_model_spec(pair: &str) -> Option<&'static BergamotModelSpec> {
    MODEL_CATALOG.iter().find(|m| m.pair == pair)
}

pub fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} МБ", bytes as f64 / MB)
    } else if bytes >= 1024 {
        format!("{:.0} КБ", bytes as f64 / KB)
    } else {
        format!("{bytes} Б")
    }
}

pub fn sanitize_lang_code(code: &str, fallback: &str) -> String {
    let cleaned: String = code
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || *c == '-')
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if cleaned.is_empty() {
        fallback.to_string()
    } else if cleaned.starts_with("zh") {
        "zh".to_string()
    } else {
        cleaned
    }
}

pub fn check_magic_bytes(header: &[u8], role: BergamotFileRole) -> bool {
    if header.is_empty() {
        return false;
    }
    // Reject HTML error pages or Git LFS pointer text files masquerading as binaries
    if header.starts_with(b"<!DOCTYPE")
        || header.starts_with(b"<!doctype")
        || header.starts_with(b"<html")
        || header.starts_with(b"<HTML")
        || header.starts_with(b"version https://git-lfs")
    {
        return false;
    }
    match role {
        BergamotFileRole::WasmBinary => header.len() >= 4 && &header[0..4] == b"\0asm",
        BergamotFileRole::WasmGlue => header.len() >= 16,
        BergamotFileRole::Model
        | BergamotFileRole::Lex
        | BergamotFileRole::Vocab
        | BergamotFileRole::SrcVocab
        | BergamotFileRole::TrgVocab => header.len() >= 16,
    }
}

pub fn is_asset_present_fast(path: &Path, asset: &BergamotAsset) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() != asset.size {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 32];
    let Ok(n) = file.read(&mut header) else {
        return false;
    };
    check_magic_bytes(&header[..n], asset.role)
}

pub fn verify_asset_sha256(path: &Path, asset: &BergamotAsset) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() != asset.size {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Some(mut checksum) = glib::Checksum::new(glib::ChecksumType::Sha256) else {
        return false;
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut first_chunk = true;
    loop {
        let Ok(n) = file.read(&mut buf) else {
            return false;
        };
        if n == 0 {
            break;
        }
        if first_chunk {
            first_chunk = false;
            if !check_magic_bytes(&buf[..n.min(32)], asset.role) {
                return false;
            }
        }
        checksum.update(&buf[..n]);
    }
    checksum
        .string()
        .is_some_and(|digest| digest.eq_ignore_ascii_case(asset.sha256))
}

pub fn is_wasm_runtime_ready() -> bool {
    let dir = wasm_runtime_dir();
    WASM_RUNTIME_ASSETS
        .iter()
        .all(|asset| is_asset_present_fast(&dir.join(asset.filename), asset))
}

pub fn is_model_pair_ready(pair: &str) -> bool {
    let Some(spec) = find_model_spec(pair) else {
        return false;
    };
    let dir = model_pair_dir(pair);
    spec.files
        .iter()
        .all(|asset| is_asset_present_fast(&dir.join(asset.filename), asset))
}

pub fn resolve_translation_plan(source_lang: &str, target_lang: &str) -> Option<TranslationPlan> {
    let tgt = sanitize_lang_code(target_lang, "ru");
    let raw_src = sanitize_lang_code(source_lang, "auto");
    let src = if raw_src == "auto" {
        if tgt == "en" {
            "ru".to_string()
        } else {
            "en".to_string()
        }
    } else {
        raw_src
    };

    if src == tgt {
        return None;
    }

    let direct_pair = format!("{src}-{tgt}");
    let pairs: Vec<&'static str> = if let Some(spec) = find_model_spec(&direct_pair) {
        vec![spec.pair]
    } else {
        let hop1 = format!("{src}-en");
        let hop2 = format!("en-{tgt}");
        let spec1 = find_model_spec(&hop1)?;
        let spec2 = find_model_spec(&hop2)?;
        vec![spec1.pair, spec2.pair]
    };

    let is_pivot = pairs.len() > 1;
    let wasm_dir = wasm_runtime_dir();
    let mut total_bytes: u64 = 0;
    let mut missing_bytes: u64 = 0;

    for asset in WASM_RUNTIME_ASSETS {
        total_bytes += asset.size;
        if !is_asset_present_fast(&wasm_dir.join(asset.filename), asset) {
            missing_bytes += asset.size;
        }
    }

    for pair in &pairs {
        if let Some(spec) = find_model_spec(pair) {
            let dir = model_pair_dir(pair);
            for asset in spec.files {
                total_bytes += asset.size;
                if !is_asset_present_fast(&dir.join(asset.filename), asset) {
                    missing_bytes += asset.size;
                }
            }
        }
    }

    Some(TranslationPlan {
        source_lang: src,
        target_lang: tgt,
        is_pivot,
        pairs: pairs.into_iter().map(str::to_string).collect(),
        total_bytes,
        missing_bytes,
    })
}

pub fn list_installed_models() -> Vec<InstalledModelInfo> {
    let mut installed = Vec::new();
    for spec in MODEL_CATALOG {
        if is_model_pair_ready(spec.pair) {
            let bytes = spec.total_bytes();
            installed.push(InstalledModelInfo {
                pair: spec.pair.to_string(),
                source_lang: spec.source_lang.to_string(),
                target_lang: spec.target_lang.to_string(),
                version: spec.version.to_string(),
                size_bytes: bytes,
                formatted_size: format_bytes(bytes),
            });
        }
    }
    installed
}

pub fn total_installed_bytes() -> u64 {
    let mut total = 0u64;
    if is_wasm_runtime_ready() {
        total += WASM_RUNTIME_ASSETS.iter().map(|a| a.size).sum::<u64>();
    }
    for spec in MODEL_CATALOG {
        if is_model_pair_ready(spec.pair) {
            total += spec.total_bytes();
        }
    }
    total
}

pub fn delete_model_pair(pair: &str) -> Result<u64, String> {
    let clean_pair = pair.trim();
    let Some(spec) = find_model_spec(clean_pair) else {
        return Err(format!("Unknown model pair: {clean_pair}"));
    };
    let dir = model_pair_dir(spec.pair);
    if !dir.exists() {
        return Ok(0);
    }
    let freed = spec.total_bytes();
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(freed)
}

pub fn delete_all_models() -> Result<u64, String> {
    let dir = bergamot_models_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let freed = total_installed_bytes();
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(freed)
}

async fn soup_send_stream(
    session: &soup::Session,
    msg: &soup::Message,
    cancellable: &gio::Cancellable,
) -> Result<gio::InputStream, glib::Error> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    session.send_async(
        msg,
        glib::Priority::DEFAULT,
        Some(cancellable),
        move |res| {
            let _ = tx.send(res);
        },
    );
    match rx.await {
        Ok(res) => res,
        Err(_) => Err(glib::Error::new(
            gio::IOErrorEnum::Cancelled,
            "Download cancelled",
        )),
    }
}

async fn stream_read_chunk(
    stream: &gio::InputStream,
    count: usize,
    cancellable: &gio::Cancellable,
) -> Result<glib::Bytes, glib::Error> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    stream.read_bytes_async(
        count,
        glib::Priority::DEFAULT,
        Some(cancellable),
        move |res| {
            let _ = tx.send(res);
        },
    );
    match rx.await {
        Ok(res) => res,
        Err(_) => Err(glib::Error::new(
            gio::IOErrorEnum::Cancelled,
            "Download cancelled",
        )),
    }
}

async fn download_single_asset_from_url<F>(
    session: &soup::Session,
    url: &str,
    dest_path: &Path,
    asset: &BergamotAsset,
    (base_downloaded, total_plan_bytes): (u64, u64),
    cancellable: &gio::Cancellable,
    on_progress: &F,
) -> Result<(), TranslateDownloadError>
where
    F: Fn(DownloadProgress),
{
    if cancellable.is_cancelled() {
        return Err(TranslateDownloadError::Cancelled);
    }

    let Some(parent) = dest_path.parent() else {
        return Err(TranslateDownloadError::IoError(
            "Invalid destination path".to_string(),
        ));
    };
    fs::create_dir_all(parent).map_err(|e| TranslateDownloadError::IoError(e.to_string()))?;

    let part_path = dest_path.with_extension("part");
    let msg = soup::Message::new("GET", url)
        .map_err(|e| TranslateDownloadError::NetworkError(e.to_string()))?;

    let stream = soup_send_stream(session, &msg, cancellable)
        .await
        .map_err(|e| {
            if cancellable.is_cancelled() || e.matches(gio::IOErrorEnum::Cancelled) {
                TranslateDownloadError::Cancelled
            } else {
                TranslateDownloadError::NetworkError(e.to_string())
            }
        })?;

    let status = msg.status();
    if status != soup::Status::Ok {
        return Err(TranslateDownloadError::NetworkError(format!(
            "HTTP {:?}",
            status
        )));
    }

    let mut part_file =
        File::create(&part_path).map_err(|e| TranslateDownloadError::IoError(e.to_string()))?;
    let Some(mut checksum) = glib::Checksum::new(glib::ChecksumType::Sha256) else {
        let _ = fs::remove_file(&part_path);
        return Err(TranslateDownloadError::ValidationFailed(
            "Failed to initialize SHA-256".to_string(),
        ));
    };

    let mut written: u64 = 0;
    let mut first_chunk = true;

    loop {
        if cancellable.is_cancelled() {
            let _ = fs::remove_file(&part_path);
            return Err(TranslateDownloadError::Cancelled);
        }

        let chunk = match stream_read_chunk(&stream, 64 * 1024, cancellable).await {
            Ok(bytes) => bytes,
            Err(e) => {
                let _ = fs::remove_file(&part_path);
                if cancellable.is_cancelled() || e.matches(gio::IOErrorEnum::Cancelled) {
                    return Err(TranslateDownloadError::Cancelled);
                }
                return Err(TranslateDownloadError::NetworkError(e.to_string()));
            }
        };

        if chunk.is_empty() {
            break;
        }

        let slice = chunk.as_ref();
        if first_chunk {
            first_chunk = false;
            if !check_magic_bytes(&slice[..slice.len().min(32)], asset.role) {
                let _ = fs::remove_file(&part_path);
                return Err(TranslateDownloadError::ValidationFailed(format!(
                    "Invalid file header for {}",
                    asset.filename
                )));
            }
        }

        written += slice.len() as u64;
        if written > asset.size + 64 * 1024 {
            let _ = fs::remove_file(&part_path);
            return Err(TranslateDownloadError::ValidationFailed(format!(
                "File {} exceeded expected size ({})",
                asset.filename, asset.size
            )));
        }

        if let Err(e) = part_file.write_all(slice) {
            let _ = fs::remove_file(&part_path);
            return Err(TranslateDownloadError::IoError(e.to_string()));
        }
        checksum.update(slice);

        let current_total = (base_downloaded + written).min(total_plan_bytes);
        let fraction = if total_plan_bytes > 0 {
            (current_total as f64 / total_plan_bytes as f64).clamp(0.0, 1.0)
        } else {
            1.0
        };
        on_progress(DownloadProgress {
            current_file: asset.filename.to_string(),
            downloaded_bytes: current_total,
            total_bytes: total_plan_bytes,
            fraction,
        });
    }

    if let Err(e) = part_file.flush() {
        let _ = fs::remove_file(&part_path);
        return Err(TranslateDownloadError::IoError(e.to_string()));
    }
    drop(part_file);

    if written != asset.size {
        let _ = fs::remove_file(&part_path);
        return Err(TranslateDownloadError::ValidationFailed(format!(
            "Size mismatch for {}: got {written}, expected {}",
            asset.filename, asset.size
        )));
    }

    let computed_hash = checksum.string().unwrap_or_default();
    if !computed_hash.eq_ignore_ascii_case(asset.sha256) {
        let _ = fs::remove_file(&part_path);
        return Err(TranslateDownloadError::ValidationFailed(format!(
            "SHA-256 mismatch for {}",
            asset.filename
        )));
    }

    fs::rename(&part_path, dest_path)
        .map_err(|e| TranslateDownloadError::IoError(e.to_string()))?;

    Ok(())
}

pub async fn ensure_plan_downloaded<F>(
    plan: &TranslationPlan,
    cancellable: &gio::Cancellable,
    on_progress: F,
) -> Result<(), TranslateDownloadError>
where
    F: Fn(DownloadProgress),
{
    let mut tasks: Vec<(PathBuf, BergamotAsset)> = Vec::new();
    let wasm_dir = wasm_runtime_dir();
    for asset in WASM_RUNTIME_ASSETS {
        tasks.push((wasm_dir.join(asset.filename), *asset));
    }
    for pair in &plan.pairs {
        if let Some(spec) = find_model_spec(pair) {
            let dir = model_pair_dir(pair);
            for asset in spec.files {
                tasks.push((dir.join(asset.filename), *asset));
            }
        }
    }

    let total_plan_bytes: u64 = tasks.iter().map(|(_, a)| a.size).sum();
    let mut completed_bytes: u64 = 0;

    let session = soup::Session::new();
    session.set_timeout(30);
    session.set_user_agent("TakeWeb/0.1 (Mozilla Bergamot Local Neural Translator)");

    for (dest_path, asset) in tasks {
        if cancellable.is_cancelled() {
            return Err(TranslateDownloadError::Cancelled);
        }

        if verify_asset_sha256(&dest_path, &asset) {
            completed_bytes += asset.size;
            let fraction = if total_plan_bytes > 0 {
                (completed_bytes as f64 / total_plan_bytes as f64).clamp(0.0, 1.0)
            } else {
                1.0
            };
            on_progress(DownloadProgress {
                current_file: asset.filename.to_string(),
                downloaded_bytes: completed_bytes,
                total_bytes: total_plan_bytes,
                fraction,
            });
            continue;
        }

        let primary_res = download_single_asset_from_url(
            &session,
            asset.primary_url,
            &dest_path,
            &asset,
            (completed_bytes, total_plan_bytes),
            cancellable,
            &on_progress,
        )
        .await;

        if let Err(err) = primary_res {
            if err == TranslateDownloadError::Cancelled || asset.fallback_url == asset.primary_url {
                return Err(err);
            }
            log::warn!(
                "Primary URL failed for {} ({err}), trying fallback mirror...",
                asset.filename
            );
            download_single_asset_from_url(
                &session,
                asset.fallback_url,
                &dest_path,
                &asset,
                (completed_bytes, total_plan_bytes),
                cancellable,
                &on_progress,
            )
            .await?;
        }

        completed_bytes += asset.size;
    }

    Ok(())
}

pub fn build_configure_translator_js(
    source_lang: &str,
    target_lang: &str,
    selection_popup_enabled: bool,
) -> String {
    let src = sanitize_lang_code(source_lang, "auto");
    let tgt = sanitize_lang_code(target_lang, "ru");
    let sel_bool = if selection_popup_enabled {
        "true"
    } else {
        "false"
    };

    let title = tr("translate_popup_selection_title");
    let badge = tr("translate_local_wasm_badge");
    let translate_sel = tr("translate_popup_translate_btn");
    let translate_page = tr("translate_popup_page_btn");
    let restore_page = tr("translate_action_restore");
    let copy = tr("translate_popup_copy_btn");
    let copied = tr("translate_popup_copied");
    let close = tr("translate_popup_close_btn");
    let downloading = tr("translate_status_downloading");
    let loading_wasm = tr("translate_status_loading_wasm");

    format!(
        "if (window.__TakeTranslator) {{ \
            window.__TakeTranslator.setConfig({{ \
                sourceLang: '{src}', \
                targetLang: '{tgt}', \
                selectionPopupEnabled: {sel_bool}, \
                uiStrings: {{ \
                    title: '{title}', \
                    badge: '{badge}', \
                    translateSelection: '{translate_sel}', \
                    translatePage: '{translate_page}', \
                    restorePage: '{restore_page}', \
                    copy: '{copy}', \
                    copied: '{copied}', \
                    close: '{close}', \
                    downloading: '{downloading}', \
                    loadingWasm: '{loading_wasm}' \
                }} \
            }}); \
        }}"
    )
}

pub fn build_plan_manifest_json(
    plan: &TranslationPlan,
    action: &str,
    request_id: &str,
) -> serde_json::Value {
    let mut models_json = Vec::new();
    for pair in &plan.pairs {
        if let Some(spec) = find_model_spec(pair) {
            let mut model_url = String::new();
            let mut lex_url = String::new();
            let mut vocab_urls = Vec::new();

            for file in spec.files {
                let url = format!("{BERGAMOT_SCHEME}://models/{}/{}", spec.pair, file.filename);
                match file.role {
                    BergamotFileRole::Model => model_url = url,
                    BergamotFileRole::Lex => lex_url = url,
                    BergamotFileRole::Vocab | BergamotFileRole::SrcVocab => {
                        vocab_urls.insert(0, url);
                    }
                    BergamotFileRole::TrgVocab => {
                        vocab_urls.push(url);
                    }
                    _ => {}
                }
            }

            models_json.push(serde_json::json!({
                "pair": spec.pair,
                "sourceLang": spec.source_lang,
                "targetLang": spec.target_lang,
                "modelUrl": model_url,
                "lexUrl": lex_url,
                "vocabUrls": vocab_urls,
            }));
        }
    }

    serde_json::json!({
        "action": action,
        "requestId": request_id,
        "sourceLang": plan.source_lang,
        "targetLang": plan.target_lang,
        "isPivot": plan.is_pivot,
        "wasmGlueUrl": format!("{BERGAMOT_SCHEME}://wasm/bergamot-translator-worker.js"),
        "wasmBinaryUrl": format!("{BERGAMOT_SCHEME}://wasm/bergamot-translator-worker.wasm"),
        "models": models_json,
    })
}

pub fn build_models_ready_js(
    plan: &TranslationPlan,
    action: &str,
    request_id: &str,
    selection_popup_enabled: bool,
) -> String {
    let cfg_js = build_configure_translator_js(
        &plan.source_lang,
        &plan.target_lang,
        selection_popup_enabled,
    );
    let manifest = build_plan_manifest_json(plan, action, request_id);
    format!(
        "{cfg_js} if (window.__TakeTranslator && window.__TakeTranslator.onModelsReady) {{ \
            window.__TakeTranslator.onModelsReady({manifest}); \
        }}"
    )
}

pub fn build_download_progress_js(progress: &DownloadProgress) -> String {
    let payload = serde_json::json!({
        "currentFile": progress.current_file,
        "downloadedBytes": progress.downloaded_bytes,
        "totalBytes": progress.total_bytes,
        "fraction": progress.fraction,
        "formattedDownloaded": format_bytes(progress.downloaded_bytes),
        "formattedTotal": format_bytes(progress.total_bytes),
    });
    format!(
        "if (window.__TakeTranslator && window.__TakeTranslator.onDownloadProgress) {{ \
            window.__TakeTranslator.onDownloadProgress({payload}); \
        }}"
    )
}

pub fn build_download_error_js(error_msg: &str) -> String {
    let payload = serde_json::json!({
        "error": error_msg,
    });
    format!(
        "if (window.__TakeTranslator && window.__TakeTranslator.onDownloadError) {{ \
            window.__TakeTranslator.onDownloadError({payload}); \
        }}"
    )
}

pub fn build_translate_page_js(
    source_lang: &str,
    target_lang: &str,
    selection_popup_enabled: bool,
) -> String {
    let src = sanitize_lang_code(source_lang, "auto");
    let tgt = sanitize_lang_code(target_lang, "ru");
    let cfg_js = build_configure_translator_js(&src, &tgt, selection_popup_enabled);
    format!(
        "{cfg_js} if (window.__TakeTranslator) {{ window.__TakeTranslator.translatePage('{src}', '{tgt}'); }}"
    )
}

pub fn build_restore_page_js() -> &'static str {
    "if (window.__TakeTranslator) { window.__TakeTranslator.restorePage(); }"
}

pub const TRANSLATOR_HOST_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>TakeWeb Bergamot Host</title>
</head>
<body>
<script>
(function() {
    'use strict';

    const GEMM_TO_FALLBACK_FUNCTIONS_MAP = {
        int8_prepare_a: 'int8PrepareAFallback',
        int8_prepare_b: 'int8PrepareBFallback',
        int8_prepare_b_from_transposed: 'int8PrepareBFromTransposedFallback',
        int8_prepare_b_from_quantized_transposed: 'int8PrepareBFromQuantizedTransposedFallback',
        int8_prepare_bias: 'int8PrepareBiasFallback',
        int8_multiply_and_add_bias: 'int8MultiplyAndAddBiasFallback',
        int8_select_columns_of_b: 'int8SelectColumnsOfBFallback'
    };

    const MODEL_CONFIG = [
        'beam-size: 1',
        'normalize: 1.0',
        'word-penalty: 0',
        'cpu-threads: 0',
        'gemm-precision: int8shiftAlphaAll',
        'skip-cost: true',
        'alignment: soft',
        'quiet: true',
        'quiet-translation: true',
        'max-length-break: 128',
        'mini-batch-words: 1024',
        'workspace: 128',
        'max-length-factor: 2.0',
        ''
    ].join('\n');

    function postHostIpc(payload) {
        try {
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.takeTranslatorHost) {
                window.webkit.messageHandlers.takeTranslatorHost.postMessage(JSON.stringify(payload));
            }
        } catch (err) {
            console.error('[BergamotHost] IPC post failed:', err);
        }
    }

    function loadScript(src) {
        return new Promise((resolve, reject) => {
            const s = document.createElement('script');
            s.src = src;
            s.async = true;
            s.onload = () => {
                s.remove();
                resolve();
            };
            s.onerror = () => {
                s.remove();
                reject(new Error('Failed to load script: ' + src));
            };
            document.head.appendChild(s);
        });
    }

    function decodeBase64ToUint8Array(b64) {
        const binStr = atob(b64);
        const len = binStr.length;
        const bytes = new Uint8Array(len);
        for (let i = 0; i < len; i++) {
            bytes[i] = binStr.charCodeAt(i);
        }
        return bytes;
    }

    function prepareAlignedMemoryFromBase64(Module, b64, alignment) {
        const binStr = atob(b64);
        const len = binStr.length;
        const memory = new Module.AlignedMemory(len, alignment);
        const view = memory.getByteArrayView();
        for (let i = 0; i < len; i++) {
            view[i] = binStr.charCodeAt(i);
        }
        return memory;
    }

    let runtimePromise = null;
    let wasmModule = null;
    let blockingService = null;
    const loadedModels = new Map();
    const loadingModels = new Map();
    let jobQueue = Promise.resolve();

    function ensureRuntime() {
        if (blockingService && wasmModule) {
            return Promise.resolve();
        }
        if (runtimePromise) {
            return runtimePromise;
        }
        runtimePromise = (async () => {
            await loadScript('take://translator/wasm_payload.js');
            const wasmB64 = window.__BERGAMOT_WASM_B64__;
            delete window.__BERGAMOT_WASM_B64__;
            if (!wasmB64) {
                throw new Error('WASM binary payload missing');
            }
            const wasmBytes = decodeBase64ToUint8Array(wasmB64);

            await new Promise((resolve, reject) => {
                const Module = {
                    wasmBinary: wasmBytes,
                    instantiateWasm(info, accept) {
                        try {
                            const wasmGemm = Object.fromEntries(
                                Object.entries(GEMM_TO_FALLBACK_FUNCTIONS_MAP).map(([key, name]) => [
                                    key,
                                    (...args) => Module.asm[name](...args)
                                ])
                            );
                            WebAssembly.instantiate(wasmBytes, {
                                ...info,
                                wasm_gemm: wasmGemm
                            }).then((result) => {
                                accept(result.instance || result);
                            }).catch(reject);
                        } catch (err) {
                            reject(err);
                        }
                        return {};
                    },
                    onRuntimeInitialized() {
                        try {
                            blockingService = new Module.BlockingService({ cacheSize: 16384 });
                            wasmModule = Module;
                            resolve();
                        } catch (err) {
                            reject(err);
                        }
                    },
                    onAbort(what) {
                        reject(new Error('Bergamot WASM aborted: ' + what));
                    }
                };
                window.Module = Module;
                loadScript('take://translator/bergamot-translator-worker.js').catch(reject);
            });
        })().catch((err) => {
            runtimePromise = null;
            throw err;
        });

        return runtimePromise;
    }

    async function ensureModel(pair) {
        if (loadedModels.has(pair)) {
            return loadedModels.get(pair);
        }
        if (loadingModels.has(pair)) {
            return await loadingModels.get(pair);
        }
        const p = (async () => {
            await ensureRuntime();
            await loadScript('take://translator/models/' + pair + '.js');
            const store = window.__BERGAMOT_MODELS_B64__ || {};
            const data = store[pair];
            if (store[pair]) {
                delete store[pair];
            }
            if (!data || !data.model || !data.lex || !Array.isArray(data.vocabs) || data.vocabs.length === 0) {
                throw new Error('Model payload missing for ' + pair);
            }
            const modelMem = prepareAlignedMemoryFromBase64(wasmModule, data.model, 256);
            const lexMem = prepareAlignedMemoryFromBase64(wasmModule, data.lex, 64);
            const vocabs = new wasmModule.AlignedMemoryList();
            for (let i = 0; i < data.vocabs.length; i++) {
                const vocabMem = prepareAlignedMemoryFromBase64(wasmModule, data.vocabs[i], 64);
                vocabs.push_back(vocabMem);
            }

            const model = new wasmModule.TranslationModel(
                MODEL_CONFIG,
                modelMem,
                lexMem,
                vocabs,
                null
            );
            loadedModels.set(pair, model);
            loadingModels.delete(pair);
            return model;
        })().catch((err) => {
            loadingModels.delete(pair);
            throw err;
        });

        loadingModels.set(pair, p);
        return await p;
    }

    async function executeJob(job) {
        await ensureRuntime();
        for (let i = 0; i < job.pairs.length; i++) {
            await ensureModel(job.pairs[i]);
        }

        const texts = job.texts || [];
        if (texts.length === 0) {
            return [];
        }

        const input = new wasmModule.VectorString();
        const options = new wasmModule.VectorResponseOptions();
        for (let i = 0; i < texts.length; i++) {
            input.push_back(String(texts[i]));
            options.push_back({ alignment: false, html: false, qualityScores: false });
        }

        let responses = null;
        try {
            if (job.isPivot && job.pairs.length === 2) {
                const m1 = loadedModels.get(job.pairs[0]);
                const m2 = loadedModels.get(job.pairs[1]);
                responses = blockingService.translateViaPivoting(m1, m2, input, options);
            } else {
                const m1 = loadedModels.get(job.pairs[0]);
                responses = blockingService.translate(m1, input, options);
            }

            const out = [];
            for (let i = 0; i < texts.length; i++) {
                out.push(responses.get(i).getTranslatedText());
            }
            return out;
        } finally {
            if (responses) {
                responses.delete();
            }
            input.delete();
            options.delete();
        }
    }

    window.__bergamotHost = {
        runJob(job) {
            jobQueue = jobQueue.then(async () => {
                try {
                    const translations = await executeJob(job);
                    postHostIpc({
                        type: 'jobDone',
                        requestId: job.requestId,
                        translations
                    });
                } catch (err) {
                    console.error('[BergamotHost] Job failed:', err);
                    postHostIpc({
                        type: 'jobError',
                        requestId: job.requestId,
                        error: (err && err.message) ? err.message : String(err)
                    });
                }
            });
        },
        evictModels() {
            loadedModels.forEach((model) => {
                try { model.delete(); } catch (_) {}
            });
            loadedModels.clear();
        }
    };

    postHostIpc({ type: 'hostReady' });
})();
</script>
</body>
</html>
"#;

pub fn build_wasm_payload_js() -> Option<Vec<u8>> {
    let wasm_path = wasm_runtime_dir().join("bergamot-translator-worker.wasm");
    let bytes = fs::read(wasm_path).ok()?;
    let b64 = glib::base64_encode(&bytes);
    Some(format!("window.__BERGAMOT_WASM_B64__ = \"{b64}\";\n").into_bytes())
}

pub fn build_model_bundle_js(pair: &str) -> Option<Vec<u8>> {
    let spec = find_model_spec(pair)?;
    let dir = model_pair_dir(spec.pair);

    let mut model_b64 = String::new();
    let mut lex_b64 = String::new();
    let mut vocab_b64s: Vec<String> = Vec::new();

    for file in spec.files {
        let bytes = fs::read(dir.join(file.filename)).ok()?;
        let encoded = glib::base64_encode(&bytes).to_string();
        match file.role {
            BergamotFileRole::Model => model_b64 = encoded,
            BergamotFileRole::Lex => lex_b64 = encoded,
            BergamotFileRole::Vocab | BergamotFileRole::SrcVocab => {
                vocab_b64s.insert(0, encoded);
            }
            BergamotFileRole::TrgVocab => {
                vocab_b64s.push(encoded);
            }
            _ => {}
        }
    }

    if model_b64.is_empty() || lex_b64.is_empty() || vocab_b64s.is_empty() {
        return None;
    }

    let vocabs_json = serde_json::to_string(&vocab_b64s).ok()?;
    let pair_json = serde_json::to_string(spec.pair).ok()?;

    let js = format!(
        "window.__BERGAMOT_MODELS_B64__ = window.__BERGAMOT_MODELS_B64__ || {{}};\n\
         window.__BERGAMOT_MODELS_B64__[{pair_json}] = {{\n\
           model: \"{model_b64}\",\n\
           lex: \"{lex_b64}\",\n\
           vocabs: {vocabs_json}\n\
         }};\n"
    );
    Some(js.into_bytes())
}

pub fn build_host_run_job_js(
    request_id: &str,
    pairs: &[String],
    is_pivot: bool,
    texts: &[String],
) -> String {
    let job = serde_json::json!({
        "requestId": request_id,
        "pairs": pairs,
        "isPivot": is_pivot,
        "texts": texts,
    });
    format!(
        "if (window.__bergamotHost && window.__bergamotHost.runJob) {{ \
            window.__bergamotHost.runJob({job}); \
        }}"
    )
}

pub fn build_batch_result_js(
    request_id: &str,
    translations: Option<&[String]>,
    error: Option<&str>,
) -> String {
    let payload = serde_json::json!({
        "requestId": request_id,
        "translations": translations,
        "error": error,
    });
    format!(
        "if (window.__TakeTranslator && window.__TakeTranslator.onBatchResult) {{ \
            window.__TakeTranslator.onBatchResult({payload}); \
        }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_translator_script_embedded_and_local_only() {
        assert!(!PAGE_TRANSLATOR_SCRIPT.is_empty());
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("__TakeTranslator"));
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("IntersectionObserver"));
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("MutationObserver"));
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("WeakMap"));
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("translateBatch"));
        assert!(PAGE_TRANSLATOR_SCRIPT.contains("onBatchResult"));
        assert!(TRANSLATOR_HOST_HTML.contains("BlockingService"));
        assert!(TRANSLATOR_HOST_HTML.contains("GEMM_TO_FALLBACK_FUNCTIONS_MAP"));
        assert!(!PAGE_TRANSLATOR_SCRIPT.contains("translate.googleapis.com"));
        assert!(!PAGE_TRANSLATOR_SCRIPT.contains("translate.yandex.net"));
        assert!(!PAGE_TRANSLATOR_SCRIPT.contains("api-free.deepl.com"));
    }

    #[test]
    fn test_sanitize_lang_code_prevents_injection() {
        assert_eq!(sanitize_lang_code("en", "auto"), "en");
        assert_eq!(sanitize_lang_code("RU'; alert(1);//", "auto"), "rualert");
        assert_eq!(sanitize_lang_code("12345", "auto"), "auto");
        assert_eq!(sanitize_lang_code("zh-Hans", "auto"), "zh");
    }

    #[test]
    fn test_build_translate_and_restore_js() {
        let js = build_translate_page_js("en", "ru", true);
        assert!(js.contains("translatePage('en', 'ru')"));
        assert!(js.contains("selectionPopupEnabled: true"));

        let restore = build_restore_page_js();
        assert!(restore.contains("restorePage()"));
    }

    #[test]
    fn test_bergamot_models_dir_uses_takeweb_path() {
        let path = bergamot_models_dir();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("takeweb"));
        assert!(path_str.ends_with("models/bergamot"));
    }

    #[test]
    fn test_resolve_translation_plan_direct_and_pivot() {
        let direct = resolve_translation_plan("en", "ru").expect("direct en-ru plan");
        assert!(!direct.is_pivot);
        assert_eq!(direct.pairs, vec!["en-ru".to_string()]);
        assert!(direct.total_bytes > 20_000_000);

        let pivot = resolve_translation_plan("es", "ru").expect("pivot es->en->ru plan");
        assert!(pivot.is_pivot);
        assert_eq!(pivot.pairs, vec!["es-en".to_string(), "en-ru".to_string()]);
        assert!(pivot.total_bytes > 40_000_000);

        let same = resolve_translation_plan("ru", "ru");
        assert!(same.is_none());
    }

    #[test]
    fn test_check_magic_bytes_rejects_html_and_lfs() {
        assert!(check_magic_bytes(
            b"\0asm\x01\x00\x00\x00",
            BergamotFileRole::WasmBinary
        ));
        assert!(!check_magic_bytes(
            b"<!DOCTYPE html><html>",
            BergamotFileRole::WasmBinary
        ));
        assert!(!check_magic_bytes(
            b"version https://git-lfs.github.com/spec/v1\n",
            BergamotFileRole::Model
        ));
    }
}
