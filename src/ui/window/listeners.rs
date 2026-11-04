use super::menu::is_web_url;
use super::{Inner, TRACKERS_URL};
use crate::privacy::session_guard::PrivateSessionGuard;
use crate::ui::app_menu::AppMenu;
use crate::ui::history_popover::HistoryPopover;
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::EventControllerKey;
use libadwaita as adw;
use std::rc::Rc;
use webkit6::prelude::*;

pub(super) fn setup_window_listeners(
    inner: &Rc<Inner>,
    tab_bar: &adw::TabBar,
    app: &adw::Application,
) {
    {
        let weak_shield = Rc::downgrade(inner);
        inner
            .shield_popover
            .set_on_toggle_protection(move |domain, active| {
                if let Some(this) = weak_shield.upgrade() {
                    let is_exception = !active;
                    let private = this.is_private_mode.get()
                        || this.current_tab().is_some_and(|t| t.is_private);
                    if !private {
                        if let Err(e) = this.db.set_protection_exception(domain, is_exception) {
                            log::warn!("failed to set protection exception: {e}");
                        }
                    }
                    if let Some(tab) = this.current_tab() {
                        if let Some(ucm) = tab.web_view.user_content_manager() {
                            if is_exception {
                                ucm.remove_filter_by_id("take_default_filters");
                            } else if let Some(ref filter) =
                                *this.engine.borrow().content_filter().borrow()
                            {
                                ucm.add_filter(filter);
                            }
                        }
                        tab.web_view.reload();
                    }
                }
            });
    }
    {
        let weak_clear = Rc::downgrade(inner);
        inner.shield_popover.set_on_clear_site_data(move |domain| {
            if let Some(this) = weak_clear.upgrade() {
                let domain_owned = domain.to_string();
                if let Some(tab) = this.current_tab() {
                    let js = "try { localStorage.clear(); sessionStorage.clear(); } catch(e){}";
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );

                    let engine = this.engine.borrow();
                    let session = engine.network_manager().session();
                    if let Some(dm) = session.website_data_manager() {
                        let view = tab.web_view.clone();
                        let dm_clone = dm.clone();
                        dm.fetch(
                            webkit6::WebsiteDataTypes::ALL,
                            None::<&gio::Cancellable>,
                            move |res| {
                                if let Ok(data_list) = res {
                                    let matched: Vec<&webkit6::WebsiteData> = data_list
                                        .iter()
                                        .filter(|d| {
                                            d.name()
                                                .map(|n| {
                                                    n.contains(&domain_owned)
                                                        || domain_owned.contains(n.as_str())
                                                })
                                                .unwrap_or(false)
                                        })
                                        .collect();
                                    if !matched.is_empty() {
                                        let (tx, rx) = tokio::sync::oneshot::channel();
                                        let view_clone = view.clone();
                                        glib::spawn_future_local(async move {
                                            if rx.await.is_ok() {
                                                view_clone.reload();
                                            }
                                        });
                                        dm_clone.remove(
                                            webkit6::WebsiteDataTypes::ALL,
                                            &matched,
                                            None::<&gio::Cancellable>,
                                            move |_| {
                                                let _ = tx.send(());
                                            },
                                        );
                                    } else {
                                        view.reload();
                                    }
                                } else {
                                    view.reload();
                                }
                            },
                        );
                    } else {
                        tab.web_view.reload();
                    }
                    this.toast(&crate::i18n::tr_fmt(
                        "app.domain_cleared",
                        &[("domain", domain)],
                    ));
                }
            }
        });
    }
    {
        let weak_priv = Rc::downgrade(inner);
        inner.shield_popover.set_on_open_privacy_settings(move || {
            if let Some(this) = weak_priv.upgrade() {
                this.open_tab(TRACKERS_URL, None, true);
            }
        });
        let weak_shield_map = Rc::downgrade(inner);
        inner.shield_popover.popover().connect_map(move |_| {
            if let Some(this) = weak_shield_map.upgrade() {
                this.refresh_shield_popover();
            }
        });
    }

    // Connect Translate Popover actions (local Bergamot WASM page translator)
    {
        let weak_tr = Rc::downgrade(inner);
        inner
            .translate_popover
            .set_on_translate(move |source_lang, target_lang| {
                if let Some(this) = weak_tr.upgrade() {
                    if let Some(tab) = this.current_tab() {
                        let url = tab.logical_url();
                        if !is_web_url(&url) && !url.starts_with("file://") {
                            this.toast(crate::i18n::tr("translate_status_internal_disabled"));
                            return;
                        }
                        let js = crate::browser::translator::build_translate_page_js(
                            source_lang,
                            target_lang,
                            true,
                        );
                        tab.web_view.evaluate_javascript(
                            &js,
                            None,
                            None,
                            None::<&gio::Cancellable>,
                            |_| {},
                        );
                        this.toast(crate::i18n::tr("translate_status_translating"));
                    }
                }
            });
    }
    {
        let weak_cancel_tr = Rc::downgrade(inner);
        inner.translate_popover.set_on_cancel_download(move || {
            if let Some(this) = weak_cancel_tr.upgrade() {
                if let Some(cancellable) = this.translate_download_cancellable.borrow_mut().take() {
                    cancellable.cancel();
                }
                if let Some(tab) = this.current_tab() {
                    let js = crate::browser::translator::build_download_error_js(crate::i18n::tr(
                        "translate_status_cancelled",
                    ));
                    tab.web_view.evaluate_javascript(
                        &js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
                this.toast(crate::i18n::tr("translate_status_cancelled"));
            }
        });
    }
    {
        let weak_del_tr = Rc::downgrade(inner);
        inner
            .translate_popover
            .set_on_delete_model(move |source_lang, target_lang| {
                if let Some(this) = weak_del_tr.upgrade() {
                    if let Some(plan) = crate::browser::translator::resolve_translation_plan(
                        source_lang,
                        target_lang,
                    ) {
                        for pair in &plan.pairs {
                            if let Err(err) = crate::browser::translator::delete_model_pair(pair) {
                                log::warn!("Failed to delete Bergamot model {pair}: {err}");
                            }
                        }
                        this.evict_translator_host_models();
                        this.translate_popover.update_model_status_ui();
                        this.refresh_settings_pages();
                        this.toast(crate::i18n::tr("translate_model_deleted_toast"));
                    }
                }
            });
    }
    {
        let weak_restore_tr = Rc::downgrade(inner);
        inner.translate_popover.set_on_restore(move || {
            if let Some(this) = weak_restore_tr.upgrade() {
                if let Some(tab) = this.current_tab() {
                    let js = crate::browser::translator::build_restore_page_js();
                    tab.web_view.evaluate_javascript(
                        js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                    this.toast(crate::i18n::tr("translate_status_restored"));
                }
            }
        });
    }
    {
        let weak_check_auto = Rc::downgrade(inner);
        inner
            .translate_popover
            .set_check_auto_translate_fn(move |src_lang| {
                if let Some(this) = weak_check_auto.upgrade() {
                    this.db.is_auto_translate_enabled(src_lang).is_some()
                        || this.db.is_any_auto_translate_enabled()
                } else {
                    false
                }
            });
    }
    {
        let weak_auto_tr = Rc::downgrade(inner);
        inner
            .translate_popover
            .set_on_toggle_auto(move |src, tgt, enabled| {
                if let Some(this) = weak_auto_tr.upgrade() {
                    if !this.is_private_mode.get() {
                        let _ = this.db.set_setting(
                            "translate_auto_enabled",
                            if enabled { "true" } else { "false" },
                        );
                        let _ = this.db.set_setting("translate_auto_target_lang", tgt);
                        let _ = this.db.set_auto_translate_language(
                            src,
                            if enabled { Some(tgt) } else { None },
                        );
                        if enabled {
                            let _ = this.db.set_auto_translate_language("auto", Some(tgt));
                        } else {
                            let _ = this.db.set_auto_translate_language("auto", None);
                        }
                    }
                    if enabled {
                        if let Some(tab) = this.current_tab() {
                            let url = tab.logical_url();
                            *tab.auto_translated_for_uri.borrow_mut() = None;
                            this.trigger_auto_translate_if_needed(&tab, &url);
                            if tab.detected_lang.borrow().is_none() {
                                let eff = this.translate_popover.effective_source_code();
                                this.maybe_trigger_auto_translate(&tab, &eff, &url);
                            }
                        }
                    }
                }
            });
    }
    {
        let weak_pop = Rc::downgrade(inner);
        inner.translate_popover.popover().connect_show(move |_| {
            if let Some(this) = weak_pop.upgrade() {
                if let Some(tab) = this.current_tab() {
                    let detected = tab.detected_lang.borrow().clone();
                    this.translate_popover
                        .set_detected_language(detected.as_deref());
                }
                this.translate_popover.update_model_status_ui();
            }
        });
    }

    // Connect restore banner action
    {
        let weak_restore = Rc::downgrade(inner);
        inner.restore_banner.connect_button_clicked(move |banner| {
            if let Some(this) = weak_restore.upgrade() {
                banner.set_revealed(false);
                banner.set_visible(false);
                if let Some(session) = this.last_crashed_session.borrow().clone() {
                    this.restore_session(&session);
                    this.toast(crate::i18n::tr("app.session_restored"));
                }
            }
        });
    }

    // Connect crash banner reload button
    {
        let weak_crash = Rc::downgrade(inner);
        inner.crash_banner.connect_button_clicked(move |banner| {
            if let Some(this) = weak_crash.upgrade() {
                banner.set_revealed(false);
                banner.set_visible(false);
                if let Some(tab) = this.current_tab() {
                    tab.clear_crashed();
                    log::info!("Reloading crashed tab #{}", tab.id);
                    tab.web_view.reload();
                }
            }
        });
    }

    // Setup system lock & sleep listener for private session and wake theme refresh
    {
        let weak_inner = Rc::downgrade(inner);
        let guard = Rc::clone(&inner.session_guard);
        let weak_wake = Rc::downgrade(inner);
        PrivateSessionGuard::setup_system_lock_listener(
            move || {
                guard.set_locked(true);
                if let Some(this) = weak_inner.upgrade() {
                    if this.is_private_mode.get() && guard.has_password() {
                        log::info!("System locked or sleeping: auto-exiting private mode");
                        this.exit_private_browsing();
                    }
                }
            },
            move || {
                if let Some(this) = weak_wake.upgrade() {
                    let saved = this
                        .db
                        .get_setting("theme")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "dark".to_string());
                    let is_dark = match saved.as_str() {
                        "light" => false,
                        "dark" => true,
                        _ => adw::StyleManager::default().is_dark(),
                    };
                    this.sync_theme_classes(is_dark);
                    this.window.queue_draw();
                }
            },
        );
    }

    // Connect Tab Overview & Floating Dock
    {
        inner.wire_tab_overview_signals(&inner.tab_overview.borrow());

        let weak_normal = Rc::downgrade(inner);
        inner
            .overview_dock
            .normal_tabs_button
            .connect_clicked(move |_| {
                if let Some(this) = weak_normal.upgrade() {
                    this.exit_private_browsing();
                }
            });

        let weak_priv = Rc::downgrade(inner);
        inner
            .overview_dock
            .private_tabs_button
            .connect_clicked(move |_| {
                if let Some(this) = weak_priv.upgrade() {
                    this.enter_private_browsing();
                }
            });
    }

    // Initialize history popover
    let history_popover = {
        let weak = Rc::downgrade(inner);
        HistoryPopover::new(
            Rc::new({
                let weak = weak.clone();
                move |url| {
                    if let Some(this) = weak.upgrade() {
                        this.open_in_current_tab(url);
                    }
                }
            }),
            Rc::new({
                let weak = weak.clone();
                move |url| {
                    if let Some(this) = weak.upgrade() {
                        this.open_tab(url, None, false);
                    }
                }
            }),
            Rc::new({
                let weak = weak.clone();
                move |query| {
                    if let Some(this) = weak.upgrade() {
                        this.load_history_for_popover(query)
                    } else {
                        Vec::new()
                    }
                }
            }),
            Rc::new({
                let weak = weak.clone();
                move || {
                    if let Some(this) = weak.upgrade() {
                        this.clear_history_from_popover();
                    }
                }
            }),
        )
    };
    history_popover
        .popover()
        .set_parent(&inner.header.container);
    *inner.history_popover.borrow_mut() = Some(history_popover);

    // Initialize app burger menu
    let app_menu = {
        let weak = Rc::downgrade(inner);
        AppMenu::new(Rc::new(move |action| {
            if let Some(this) = weak.upgrade() {
                this.handle_app_menu_action(action);
            }
        }))
    };
    inner
        .header
        .burger_button
        .set_popover(Some(&app_menu.popover));
    *inner.app_menu.borrow_mut() = Some(app_menu);

    // Window-level key events (Escape for fullscreen, Alt/Super to dismiss open popovers on switch)
    let win_key_ctrl = EventControllerKey::new();
    win_key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let weak = Rc::downgrade(inner);
    win_key_ctrl.connect_key_pressed(move |_, keyval, _, modifiers| {
        let Some(this) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        // Alt-Tab or Super (app switcher) dismisses floating popovers
        if (keyval == Key::Tab && modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK))
            || keyval == Key::Super_L
            || keyval == Key::Super_R
        {
            this.close_all_popovers();
        }

        if keyval == Key::Escape {
            this.close_all_popovers();
            if this.window.is_fullscreen() {
                this.leave_fullscreen();
                if let Some(tab) = this.current_tab() {
                    tab.web_view.evaluate_javascript(
                        "if (document.fullscreenElement) { document.exitFullscreen(); }",
                        None::<&str>,
                        None::<&str>,
                        None::<&gio::Cancellable>,
                        |_| (),
                    );
                }
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    inner.window.add_controller(win_key_ctrl);

    let weak_unmap = Rc::downgrade(inner);
    inner.window.connect_unmap(move |_| {
        if let Some(this) = weak_unmap.upgrade() {
            this.close_all_popovers();
        }
    });

    inner.connect_tab_view(tab_bar);
    inner.connect_header();
    inner.connect_sidebar();
    inner.register_tab_actions();
    inner.register_window_actions(app);

    let weak_tracker = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_tracker_block_listener(move |domain, rule| {
            if let Some(this) = weak_tracker.upgrade() {
                this.record_tracker_blocked(domain, rule);
                this.privacy_stats_changed();
            }
        });

    let weak_fp = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_fingerprint_listener(move |vector, count| {
            if let Some(this) = weak_fp.upgrade() {
                this.record_fingerprint_defended(vector, count);
                this.privacy_stats_changed();
            }
        });

    let weak_settings = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_settings_change_listener(move |key, value| {
            if let Some(this) = weak_settings.upgrade() {
                this.on_setting_changed(key, value);
            }
        });

    let weak_translate_ipc = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_translate_listener(move |json_str| {
            let Some(this) = weak_translate_ipc.upgrade() else {
                return;
            };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(json_str) else {
                return;
            };
            let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match msg_type {
                "ensureModels" => {
                    let source_lang = payload
                        .get("sourceLang")
                        .and_then(|v| v.as_str())
                        .unwrap_or("en")
                        .to_string();
                    let target_lang = payload
                        .get("targetLang")
                        .and_then(|v| v.as_str())
                        .unwrap_or("ru")
                        .to_string();
                    let action = payload
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("page")
                        .to_string();
                    let request_id = payload
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("req")
                        .to_string();
                    let url = payload.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let target_tab = this.find_tab_by_url_or_current(url);
                    this.start_bergamot_translation_flow_for_tab(
                        target_tab,
                        &source_lang,
                        &target_lang,
                        &action,
                        &request_id,
                    );
                }
                "translateBatch" => {
                    let request_id = payload
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if request_id.is_empty() {
                        return;
                    }
                    let is_pivot = payload
                        .get("isPivot")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let pairs: Vec<String> = payload
                        .get("pairs")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let texts: Vec<String> = payload
                        .get("texts")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(tab) = this.current_tab() {
                        this.translator_inflight_requests
                            .borrow_mut()
                            .insert(request_id.clone(), tab.web_view.clone());
                        let js = crate::browser::translator::build_host_run_job_js(
                            &request_id,
                            &pairs,
                            is_pivot,
                            &texts,
                        );
                        this.dispatch_to_translator_host(js);
                    }
                }
                "pageLanguageDetected" => {
                    let lang = payload
                        .get("lang")
                        .and_then(|v| v.as_str())
                        .unwrap_or("en")
                        .to_string();
                    let url = payload
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(tab) = this.find_tab_by_url_or_current(&url) {
                        *tab.detected_lang.borrow_mut() = Some(lang.clone());
                        this.maybe_trigger_auto_translate(&tab, &lang, &url);
                    }
                }
                "translationFinished" => {
                    this.translate_popover.mark_translated();
                    this.toast(crate::i18n::tr("translate_status_done"));
                }
                _ => {}
            }
        });

    let weak_host_ipc = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_translator_host_listener(move |json_str| {
            let Some(this) = weak_host_ipc.upgrade() else {
                return;
            };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(json_str) else {
                return;
            };
            let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match msg_type {
                "hostReady" => {
                    this.translator_host_ready.set(true);
                    let jobs: Vec<String> = this
                        .translator_pending_jobs
                        .borrow_mut()
                        .drain(..)
                        .collect();
                    if let Some(ref view) = *this.translator_view.borrow() {
                        for js in jobs {
                            view.evaluate_javascript(
                                &js,
                                None,
                                None,
                                None::<&gio::Cancellable>,
                                |_| {},
                            );
                        }
                    }
                }
                "jobDone" => {
                    let request_id = payload
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let translations: Vec<String> = payload
                        .get("translations")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(view) = this
                        .translator_inflight_requests
                        .borrow_mut()
                        .remove(request_id)
                    {
                        let js = crate::browser::translator::build_batch_result_js(
                            request_id,
                            Some(&translations),
                            None,
                        );
                        view.evaluate_javascript(
                            &js,
                            None,
                            None,
                            None::<&gio::Cancellable>,
                            |_| {},
                        );
                    }
                }
                "jobError" => {
                    let request_id = payload
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let err_msg = payload
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Bergamot WASM error");
                    this.translate_popover.show_download_error(err_msg);
                    if let Some(view) = this
                        .translator_inflight_requests
                        .borrow_mut()
                        .remove(request_id)
                    {
                        let js = crate::browser::translator::build_batch_result_js(
                            request_id,
                            None,
                            Some(err_msg),
                        );
                        view.evaluate_javascript(
                            &js,
                            None,
                            None,
                            None::<&gio::Cancellable>,
                            |_| {},
                        );
                    }
                }
                _ => {}
            }
        });

    let weak_reader_ipc = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_reader_mode_listener(move |active, _url| {
            if let Some(this) = weak_reader_ipc.upgrade() {
                this.on_reader_mode_changed(active);
            }
        });

    let weak_action_ipc = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_action_listener(move |json_str| {
            let Some(this) = weak_action_ipc.upgrade() else {
                return;
            };
            let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else {
                return;
            };
            let action = val.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if action == "add_favorite" {
                let title = val
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let url = val.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
                if !title.is_empty() && !url.is_empty() {
                    let full_url = if !url.contains("://") {
                        format!("https://{url}")
                    } else {
                        url.to_string()
                    };
                    if let Err(err) = this.db.add_bookmark(
                        title,
                        &full_url,
                        Some(crate::storage::database::FAVORITES_FOLDER),
                    ) {
                        log::warn!("failed to add favorite bookmark: {err}");
                    } else {
                        this.toast(crate::i18n::tr("app.bookmark_added"));
                        for tab in this.tabs.borrow().iter() {
                            if let Some(uri) = tab.web_view.uri() {
                                if uri.starts_with("take://start") || uri.starts_with("about:start")
                                {
                                    this.push_start_page_data(&tab.web_view);
                                }
                            }
                        }
                        this.library_changed();
                    }
                }
            }
        });

    let weak_media_ipc = Rc::downgrade(inner);
    inner
        .engine
        .borrow()
        .setup_media_listener(move |event, title, artist, url| {
            if let Some(this) = weak_media_ipc.upgrade() {
                if let Some(ref mpris) = *this.mpris_controller.borrow() {
                    let status = match event {
                        "play" => mpris_server::PlaybackStatus::Playing,
                        "pause" => mpris_server::PlaybackStatus::Paused,
                        _ => mpris_server::PlaybackStatus::Stopped,
                    };
                    mpris.update_playback(status, title, artist, url);
                }
            }
        });

    // Load & apply saved theme setting
    let saved_theme = inner
        .db
        .get_setting("theme")
        .unwrap_or(None)
        .unwrap_or_else(|| "dark".to_string());
    inner.apply_theme_setting(&saved_theme);

    let weak_theme = Rc::downgrade(inner);
    adw::StyleManager::default().connect_dark_notify(move |mgr| {
        if let Some(this) = weak_theme.upgrade() {
            let saved = this
                .db
                .get_setting("theme")
                .ok()
                .flatten()
                .unwrap_or_else(|| "dark".to_string());
            if saved == "system" || saved == "auto" {
                let is_dark = mgr.is_dark();
                this.sync_theme_classes(is_dark);
            }
        }
    });

    // Load and apply persisted browser settings
    if let Ok(Some(v)) = inner.db.get_setting("omnibox_suggestions") {
        inner.omnibox_suggestions_enabled.set(v != "false");
    }
    if let Ok(Some(v)) = inner.db.get_setting("open_tabs_related") {
        inner.open_tabs_related.set(v != "false");
    }
    if !inner.is_app_mode.get() {
        if let Ok(Some(v)) = inner.db.get_setting("sidebar_default_visible") {
            let show = v != "false";
            inner.sidebar_default_visible.set(show);
            super::menu::set_sidebar_visible(&inner.split_view, show);
        }
    }
    if let Ok(Some(v)) = inner.db.get_setting("close_window_with_last_tab") {
        inner.close_window_with_last_tab.set(v != "false");
    }
    if let Ok(Some(dir)) = inner.db.get_setting("download_dir") {
        inner
            .download_manager
            .set_download_dir(Some(std::path::PathBuf::from(dir)));
    }
    if let Ok(Some(ask)) = inner.db.get_setting("ask_download_location") {
        inner.download_manager.set_ask_destination(ask == "true");
    }

    // Apply WebKit engine settings
    {
        let engine = inner.engine.borrow();
        if let Ok(Some(v)) = inner.db.get_setting("hardware_acceleration") {
            engine.set_hardware_acceleration(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("smooth_scrolling") {
            engine.set_smooth_scrolling(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("enable_javascript") {
            engine.set_javascript_enabled(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("block_autoplay") {
            engine.set_block_autoplay(v == "true");
        }
        if let Ok(Some(v)) = inner.db.get_setting("webrtc_protection") {
            engine.set_webrtc_protection(v == "true");
        }
        if let Ok(Some(v)) = inner.db.get_setting("shield_adblock") {
            engine.set_adblock_enabled(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("shield_fingerprint") {
            engine.set_anti_fingerprint(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("shield_itp") {
            engine.network_manager().set_itp_enabled(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("shield_https_only") {
            engine.set_https_only(v != "false");
        }
        if let Ok(Some(v)) = inner.db.get_setting("cookie_policy") {
            engine.network_manager().set_cookie_policy(&v);
        }
    }
}
