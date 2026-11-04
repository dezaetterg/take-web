//! Custom TakeWeb error page generator.
//!
//! Renders a modern, responsive Adwaita-styled HTML error page when network
//! or DNS failures occur (`connect_load_failed`), with diagnostic tips and a
//! prominent "Перезагрузить" button.

pub fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

pub fn human_readable_error(error: &glib::Error) -> (&'static str, &'static str) {
    let msg = error.message().to_lowercase();
    let domain = error.domain().as_str();

    if msg.contains("resolve") || msg.contains("not found") || domain.contains("resolver") {
        (
            crate::i18n::tr("error_page.server_not_found_title"),
            crate::i18n::tr("error_page.server_not_found_desc"),
        )
    } else if msg.contains("refused") || msg.contains("connect") {
        (
            crate::i18n::tr("error_page.connection_refused_title"),
            crate::i18n::tr("error_page.connection_refused_desc"),
        )
    } else if msg.contains("timeout") || msg.contains("timed out") {
        (
            crate::i18n::tr("error_page.timeout_title"),
            crate::i18n::tr("error_page.timeout_desc"),
        )
    } else if msg.contains("tls") || msg.contains("ssl") || msg.contains("cert") {
        (
            crate::i18n::tr("error_page.tls_error_title"),
            crate::i18n::tr("error_page.tls_error_desc"),
        )
    } else if msg.contains("offline") || msg.contains("network") {
        (
            crate::i18n::tr("error_page.network_down_title"),
            crate::i18n::tr("error_page.network_down_desc"),
        )
    } else {
        (
            crate::i18n::tr("error_page.generic_title"),
            crate::i18n::tr("error_page.generic_desc"),
        )
    }
}

/// Generates a standalone, beautiful HTML error page.
pub fn render_error_page(failing_uri: &str, error: &glib::Error) -> String {
    let (title, description) = human_readable_error(error);
    let escaped_url = escape_html(failing_uri);
    let escaped_err_msg = escape_html(error.message());
    let domain = error.domain().as_str();
    let code = error.code();
    let lang = crate::i18n::current_language_code();
    let badge_network_error = crate::i18n::tr("error_page.badge_network_error");
    let btn_reload = crate::i18n::tr("error_page.btn_reload");
    let btn_connecting = crate::i18n::tr("error_page.btn_connecting");
    let btn_back = crate::i18n::tr("error_page.btn_back");
    let suggestions_title = crate::i18n::tr("error_page.suggestions_title");
    let suggestion_1 = crate::i18n::tr("error_page.suggestion_1");
    let suggestion_2 = crate::i18n::tr("error_page.suggestion_2");
    let suggestion_3 = crate::i18n::tr("error_page.suggestion_3");
    let details_title = crate::i18n::tr("error_page.details");
    let error_code_label = crate::i18n::tr("error_page.error_code");

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} — TakeWeb</title>
  <style>
    :root {{
      --bg-gradient: radial-gradient(circle at 50% 20%, #202029 0%, #111116 100%);
      --card-bg: rgba(255, 255, 255, 0.04);
      --card-border: rgba(255, 255, 255, 0.08);
      --card-shadow: 0 24px 64px rgba(0, 0, 0, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2);
      --text-main: #f4f4f5;
      --text-muted: #a1a1aa;
      --accent: #3584e4;
      --accent-hover: #1c71d8;
      --accent-glow: rgba(53, 132, 228, 0.25);
      --badge-bg: rgba(224, 27, 36, 0.15);
      --badge-text: #ff7b63;
      --badge-border: rgba(224, 27, 36, 0.3);
      --code-bg: rgba(0, 0, 0, 0.3);
      --btn-sec-bg: rgba(255, 255, 255, 0.08);
      --btn-sec-hover: rgba(255, 255, 255, 0.14);
      --btn-sec-text: #e4e4e7;
    }}

    @media (prefers-color-scheme: light) {{
      :root {{
        --bg-gradient: radial-gradient(circle at 50% 20%, #f6f8fb 0%, #e8edf5 100%);
        --card-bg: rgba(255, 255, 255, 0.85);
        --card-border: rgba(0, 0, 0, 0.08);
        --card-shadow: 0 20px 50px rgba(0, 0, 0, 0.08), 0 2px 6px rgba(0, 0, 0, 0.04);
        --text-main: #18181b;
        --text-muted: #52525b;
        --accent: #1c71d8;
        --accent-hover: #165bb0;
        --accent-glow: rgba(28, 113, 216, 0.2);
        --badge-bg: rgba(224, 27, 36, 0.1);
        --badge-text: #c01c28;
        --badge-border: rgba(224, 27, 36, 0.2);
        --code-bg: rgba(0, 0, 0, 0.05);
        --btn-sec-bg: rgba(0, 0, 0, 0.06);
        --btn-sec-hover: rgba(0, 0, 0, 0.12);
        --btn-sec-text: #27272a;
      }}
    }}

    [data-theme="dark"] {{
      --bg-gradient: radial-gradient(circle at 50% 20%, #202029 0%, #111116 100%);
      --card-bg: rgba(255, 255, 255, 0.04);
      --card-border: rgba(255, 255, 255, 0.08);
      --card-shadow: 0 24px 64px rgba(0, 0, 0, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2);
      --text-main: #f4f4f5;
      --text-muted: #a1a1aa;
      --accent: #3584e4;
      --accent-hover: #1c71d8;
      --accent-glow: rgba(53, 132, 228, 0.25);
      --badge-bg: rgba(224, 27, 36, 0.15);
      --badge-text: #ff7b63;
      --badge-border: rgba(224, 27, 36, 0.3);
      --code-bg: rgba(0, 0, 0, 0.3);
      --btn-sec-bg: rgba(255, 255, 255, 0.08);
      --btn-sec-hover: rgba(255, 255, 255, 0.14);
      --btn-sec-text: #e4e4e7;
    }}

    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }}

    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Cantarell, Ubuntu, "Helvetica Neue", Arial, sans-serif;
      background: var(--bg-gradient);
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 24px;
      color: var(--text-main);
      overflow-x: hidden;
    }}

    .container {{
      max-width: 580px;
      width: 100%;
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 20px;
      padding: 44px 36px;
      box-shadow: var(--card-shadow);
      backdrop-filter: blur(28px);
      -webkit-backdrop-filter: blur(28px);
      text-align: center;
      animation: fadeIn 0.35s cubic-bezier(0.16, 1, 0.3, 1);
    }}

    @keyframes fadeIn {{
      from {{
        opacity: 0;
        transform: translateY(16px) scale(0.98);
      }}
      to {{
        opacity: 1;
        transform: translateY(0) scale(1);
      }}
    }}

    .icon-wrapper {{
      width: 72px;
      height: 72px;
      margin: 0 auto 20px;
      border-radius: 50%;
      background: var(--badge-bg);
      border: 1px solid var(--badge-border);
      display: flex;
      align-items: center;
      justify-content: center;
      box-shadow: 0 0 24px var(--badge-bg);
    }}

    .icon-wrapper svg {{
      width: 36px;
      height: 36px;
      stroke: var(--badge-text);
      stroke-width: 2;
      stroke-linecap: round;
      stroke-linejoin: round;
      fill: none;
    }}

    .badge {{
      display: inline-block;
      padding: 4px 12px;
      border-radius: 999px;
      font-size: 12px;
      font-weight: 600;
      letter-spacing: 0.4px;
      text-transform: uppercase;
      background: var(--badge-bg);
      color: var(--badge-text);
      border: 1px solid var(--badge-border);
      margin-bottom: 14px;
    }}

    h1 {{
      font-size: 24px;
      font-weight: 700;
      line-height: 1.3;
      margin-bottom: 12px;
      letter-spacing: -0.3px;
    }}

    p.description {{
      font-size: 15px;
      line-height: 1.55;
      color: var(--text-muted);
      margin-bottom: 22px;
    }}

    .url-pill {{
      display: inline-flex;
      align-items: center;
      max-width: 100%;
      padding: 8px 14px;
      background: var(--code-bg);
      border: 1px solid var(--card-border);
      border-radius: 10px;
      font-family: ui-monospace, SFMono-Regular, "Cascadia Code", "Liberation Mono", Menlo, monospace;
      font-size: 13px;
      color: var(--text-main);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-bottom: 28px;
    }}

    .actions {{
      display: flex;
      gap: 12px;
      justify-content: center;
      margin-bottom: 30px;
    }}

    .btn {{
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      padding: 12px 24px;
      border-radius: 12px;
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
      transition: all 0.18s ease;
      border: none;
      outline: none;
    }}

    .btn-primary {{
      background: var(--accent);
      color: #ffffff;
      box-shadow: 0 4px 14px var(--accent-glow);
    }}

    .btn-primary:hover {{
      background: var(--accent-hover);
      transform: translateY(-1px);
      box-shadow: 0 6px 20px var(--accent-glow);
    }}

    .btn-primary:active {{
      transform: translateY(0);
    }}

    .btn-secondary {{
      background: var(--btn-sec-bg);
      color: var(--btn-sec-text);
    }}

    .btn-secondary:hover {{
      background: var(--btn-sec-hover);
    }}

    .btn svg {{
      width: 16px;
      height: 16px;
      stroke: currentColor;
      stroke-width: 2.2;
      fill: none;
    }}

    .suggestions {{
      text-align: left;
      border-top: 1px solid var(--card-border);
      padding-top: 20px;
      font-size: 13px;
      color: var(--text-muted);
      line-height: 1.6;
    }}

    .suggestions h2 {{
      font-size: 13px;
      font-weight: 600;
      color: var(--text-main);
      margin-bottom: 8px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }}

    .suggestions ul {{
      padding-left: 20px;
      margin-bottom: 14px;
    }}

    details {{
      text-align: left;
      font-size: 12px;
      color: var(--text-muted);
      margin-top: 14px;
    }}

    summary {{
      cursor: pointer;
      user-select: none;
      font-weight: 500;
      outline: none;
      color: var(--text-muted);
      transition: color 0.15s;
    }}

    summary:hover {{
      color: var(--text-main);
    }}

    pre {{
      margin-top: 8px;
      padding: 10px 12px;
      background: var(--code-bg);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      font-family: ui-monospace, SFMono-Regular, monospace;
      font-size: 11px;
      color: var(--text-muted);
      white-space: pre-wrap;
      word-break: break-all;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="icon-wrapper">
      <svg viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="2" y1="12" x2="22" y2="12"></line>
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path>
        <line x1="4.93" y1="4.93" x2="19.07" y2="19.07" stroke="#ff7b63" stroke-width="2.5"></line>
      </svg>
    </div>

    <div class="badge">{badge_network_error}</div>
    <h1>{title}</h1>
    <p class="description">{description}</p>

    <div class="url-pill" title="{escaped_url}">{escaped_url}</div>

    <div class="actions">
      <button id="reload-btn" class="btn btn-primary" onclick="retryLoad('{escaped_url}')">
        <svg viewBox="0 0 24 24">
          <polyline points="23 4 23 10 17 10"></polyline>
          <polyline points="1 20 1 14 7 14"></polyline>
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"></path>
        </svg>
        <span>{btn_reload}</span>
      </button>

      <button class="btn btn-secondary" onclick="window.history.back()">
        <span>{btn_back}</span>
      </button>
    </div>

    <div class="suggestions">
      <h2>{suggestions_title}</h2>
      <ul>
        <li>{suggestion_1}</li>
        <li>{suggestion_2}</li>
        <li>{suggestion_3}</li>
      </ul>
    </div>

    <details>
      <summary>{details_title}</summary>
      <pre>{error_code_label} {code} ({domain})\n{escaped_err_msg}</pre>
    </details>
  </div>

  <script>
    function retryLoad(url) {{
      const btn = document.getElementById('reload-btn');
      if (btn) {{
        btn.disabled = true;
        btn.querySelector('span').textContent = '{btn_connecting}';
      }}
      if (url && url.startsWith('http')) {{
        window.location.href = url;
      }} else {{
        window.location.reload();
      }}
    }}

    window.addEventListener('keydown', (e) => {{
      if (e.key === 'F5' || (e.ctrlKey && e.key === 'r')) {{
        e.preventDefault();
        retryLoad('{escaped_url}');
      }}
    }});
  </script>
</body>
</html>
"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(
            escape_html("https://example.com/search?q=1&foo=<bar>\"'"),
            "https://example.com/search?q=1&amp;foo=&lt;bar&gt;&quot;&#39;"
        );
    }

    #[test]
    fn test_human_readable_error_categories() {
        let dns_err = glib::Error::new(
            glib::FileError::Noent,
            "Cannot resolve hostname example.com",
        );
        let (title, _) = human_readable_error(&dns_err);
        assert!(!title.is_empty());
        assert!(
            title
                == crate::i18n::tr_lang(
                    "error_page.server_not_found_title",
                    crate::i18n::UiLanguage::En
                )
                || title
                    == crate::i18n::tr_lang(
                        "error_page.server_not_found_title",
                        crate::i18n::UiLanguage::Ru
                    )
        );

        let refused_err = glib::Error::new(glib::FileError::Noent, "Connection refused");
        let (title, _) = human_readable_error(&refused_err);
        assert!(
            title
                == crate::i18n::tr_lang(
                    "error_page.connection_refused_title",
                    crate::i18n::UiLanguage::En
                )
                || title
                    == crate::i18n::tr_lang(
                        "error_page.connection_refused_title",
                        crate::i18n::UiLanguage::Ru
                    )
        );

        let timeout_err = glib::Error::new(glib::FileError::Noent, "Operation timed out");
        let (title, _) = human_readable_error(&timeout_err);
        assert!(
            title == crate::i18n::tr_lang("error_page.timeout_title", crate::i18n::UiLanguage::En)
                || title
                    == crate::i18n::tr_lang(
                        "error_page.timeout_title",
                        crate::i18n::UiLanguage::Ru
                    )
        );

        let tls_err = glib::Error::new(
            glib::FileError::Noent,
            "TLS certificate verification failed",
        );
        let (title, _) = human_readable_error(&tls_err);
        assert!(
            title
                == crate::i18n::tr_lang("error_page.tls_error_title", crate::i18n::UiLanguage::En)
                || title
                    == crate::i18n::tr_lang(
                        "error_page.tls_error_title",
                        crate::i18n::UiLanguage::Ru
                    )
        );
    }

    #[test]
    fn test_render_error_page_contains_elements() {
        let err = glib::Error::new(glib::FileError::Noent, "Network unreachable");
        let html = render_error_page("https://example.com/test", &err);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("https://example.com/test"));
        assert!(html.contains("id=\"reload-btn\""));
        assert!(html.contains("retryLoad"));
        assert!(
            html.contains(crate::i18n::tr_lang(
                "error_page.btn_reload",
                crate::i18n::UiLanguage::En
            )) || html.contains(crate::i18n::tr_lang(
                "error_page.btn_reload",
                crate::i18n::UiLanguage::Ru
            ))
        );
        assert!(
            html.contains(crate::i18n::tr_lang(
                "error_page.badge_network_error",
                crate::i18n::UiLanguage::En
            )) || html.contains(crate::i18n::tr_lang(
                "error_page.badge_network_error",
                crate::i18n::UiLanguage::Ru
            ))
        );
    }
}
