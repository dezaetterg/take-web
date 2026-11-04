use super::Inner;
use crate::ui::tab::BrowserTab;
use gtk4::prelude::*;
use std::rc::Rc;
use webkit6::prelude::*;

impl Inner {
    pub(super) fn ensure_translator_view(&self) {
        if self.translator_view.borrow().is_some() {
            return;
        }
        let view = self.engine.borrow().create_translator_view();
        view.set_size_request(1, 1);
        view.set_halign(gtk4::Align::Start);
        view.set_valign(gtk4::Align::Start);
        view.set_opacity(0.0);
        view.set_can_target(false);
        view.set_can_focus(false);
        self.content_overlay.add_overlay(&view);
        *self.translator_view.borrow_mut() = Some(view);
    }

    pub(super) fn dispatch_to_translator_host(&self, js: String) {
        self.ensure_translator_view();
        if self.translator_host_ready.get() {
            if let Some(ref view) = *self.translator_view.borrow() {
                view.evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
            }
        } else {
            self.translator_pending_jobs.borrow_mut().push(js);
        }
    }

    pub(super) fn evict_translator_host_models(&self) {
        if let Some(ref view) = *self.translator_view.borrow() {
            view.evaluate_javascript(
                "if (window.__bergamotHost && window.__bergamotHost.evictModels) { window.__bergamotHost.evictModels(); }",
                None,
                None,
                None::<&gio::Cancellable>,
                |_| {},
            );
        }
    }

    pub(super) fn start_bergamot_translation_flow_for_tab(
        self: &Rc<Self>,
        tab_opt: Option<Rc<BrowserTab>>,
        source_lang: &str,
        target_lang: &str,
        action: &str,
        request_id: &str,
    ) {
        let tab = tab_opt.or_else(|| self.current_tab());
        let Some(tab) = tab else {
            return;
        };
        let Some(plan) =
            crate::browser::translator::resolve_translation_plan(source_lang, target_lang)
        else {
            return;
        };

        if let Some(prev) = self.translate_download_cancellable.borrow_mut().take() {
            prev.cancel();
        }
        let cancellable = gio::Cancellable::new();
        *self.translate_download_cancellable.borrow_mut() = Some(cancellable.clone());

        let weak_this = Rc::downgrade(self);
        let web_view = tab.web_view.clone();
        let action_owned = action.to_string();
        let request_id_owned = request_id.to_string();

        glib::spawn_future_local(async move {
            let weak_progress = weak_this.clone();
            let view_progress = web_view.clone();
            let res = crate::browser::translator::ensure_plan_downloaded(
                &plan,
                &cancellable,
                move |progress| {
                    if let Some(win) = weak_progress.upgrade() {
                        win.translate_popover.show_download_progress(&progress);
                    }
                    let js = crate::browser::translator::build_download_progress_js(&progress);
                    view_progress.evaluate_javascript(
                        &js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                },
            )
            .await;

            let Some(win) = weak_this.upgrade() else {
                return;
            };

            match res {
                Ok(()) => {
                    win.ensure_translator_view();
                    win.translate_popover.mark_loading_wasm();
                    win.refresh_settings_pages();
                    let js = crate::browser::translator::build_models_ready_js(
                        &plan,
                        &action_owned,
                        &request_id_owned,
                        true,
                    );
                    web_view.evaluate_javascript(
                        &js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
                Err(err) => {
                    let msg = err.to_string();
                    win.translate_popover.show_download_error(&msg);
                    win.toast(&msg);
                    let js = crate::browser::translator::build_download_error_js(&msg);
                    web_view.evaluate_javascript(
                        &js,
                        None,
                        None,
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
        });
    }
}
