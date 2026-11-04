//! Reader Mode (Режим чтения)
//! Extracts core article text from complex web pages, strips advertising and distracting sidebars,
//! and presents clean readable content with dark/sepia/light theme support.

pub const READER_MODE_SCRIPT: &str = include_str!("../../resources/scripts/reader_mode.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reader_mode_script_embedded() {
        assert!(!READER_MODE_SCRIPT.is_empty());
        assert!(READER_MODE_SCRIPT.contains("__TakeReaderMode"));
        assert!(READER_MODE_SCRIPT.contains("take-reader-article"));
        assert!(READER_MODE_SCRIPT.contains("enterReaderMode"));
        assert!(READER_MODE_SCRIPT.contains("exitReaderMode"));
        // Verifies that images and embedded media are stripped in reader mode
        assert!(READER_MODE_SCRIPT.contains("'img'"));
        assert!(READER_MODE_SCRIPT.contains("display: none !important;"));
    }
}
