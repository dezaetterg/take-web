use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Orientation};

pub fn normal_tabs_label() -> &'static str {
    crate::i18n::tr("tab_overview.normal_tabs")
}

pub fn private_tabs_label() -> &'static str {
    crate::i18n::tr("tab_overview.private_tabs")
}

pub struct TabOverviewDock {
    pub container: GtkBox,
    pub normal_tabs_button: Button,
    pub private_tabs_button: Button,
}

impl TabOverviewDock {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Horizontal, 4);
        container.add_css_class("take-overview-dock");
        container.set_halign(gtk4::Align::Center);
        container.set_valign(gtk4::Align::End);
        container.set_margin_bottom(24);
        container.set_visible(false);

        let normal_tabs_button = Button::builder()
            .icon_name("web-browser-symbolic")
            .label(normal_tabs_label())
            .css_classes(["take-dock-btn", "take-dock-normal", "take-dock-active"])
            .tooltip_text(crate::i18n::tr("tab_overview.switch_to_normal"))
            .build();

        let private_tabs_button = Button::builder()
            .icon_name("view-conceal-symbolic")
            .label(private_tabs_label())
            .css_classes(["take-dock-btn", "take-dock-private"])
            .tooltip_text(crate::i18n::tr("tab_overview.switch_to_private"))
            .build();

        container.append(&normal_tabs_button);
        container.append(&private_tabs_button);

        Self {
            container,
            normal_tabs_button,
            private_tabs_button,
        }
    }

    pub fn set_private_mode(&self, is_private: bool) {
        if is_private {
            self.normal_tabs_button.remove_css_class("take-dock-active");
            self.private_tabs_button.add_css_class("take-dock-active");
        } else {
            self.normal_tabs_button.add_css_class("take-dock-active");
            self.private_tabs_button
                .remove_css_class("take-dock-active");
        }
    }

    pub fn set_visible(&self, visible: bool) {
        self.container.set_visible(visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dock_constants() {
        crate::i18n::set_language(crate::i18n::UiLanguage::Ru);
        assert_eq!(normal_tabs_label(), "Обычные вкладки");
        assert_eq!(private_tabs_label(), "Приватные вкладки");
        crate::i18n::set_language(crate::i18n::UiLanguage::En);
        assert_eq!(normal_tabs_label(), "Normal Tabs");
        assert_eq!(private_tabs_label(), "Private Tabs");
    }
}
