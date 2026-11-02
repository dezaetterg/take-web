use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Orientation};

pub const NORMAL_TABS_LABEL: &str = "Обычные вкладки";
pub const PRIVATE_TABS_LABEL: &str = "Приватные вкладки";

pub struct TabOverviewDock {
    pub container: GtkBox,
    pub normal_tabs_button: Button,
    pub private_tabs_button: Button,
}

impl TabOverviewDock {
    pub fn new() -> Self {
        let container = GtkBox::new(Orientation::Horizontal, 4);
        container.add_css_class("echo-overview-dock");
        container.set_halign(gtk4::Align::Center);
        container.set_valign(gtk4::Align::End);
        container.set_margin_bottom(24);
        container.set_visible(false);

        let normal_tabs_button = Button::builder()
            .icon_name("web-browser-symbolic")
            .label(NORMAL_TABS_LABEL)
            .css_classes(["echo-dock-btn", "echo-dock-normal", "echo-dock-active"])
            .tooltip_text("Переключиться на обычный режим вкладок")
            .build();

        let private_tabs_button = Button::builder()
            .icon_name("view-conceal-symbolic")
            .label(PRIVATE_TABS_LABEL)
            .css_classes(["echo-dock-btn", "echo-dock-private"])
            .tooltip_text("Переключиться в приватный режим (Инкогнито)")
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
            self.normal_tabs_button.remove_css_class("echo-dock-active");
            self.private_tabs_button.add_css_class("echo-dock-active");
        } else {
            self.normal_tabs_button.add_css_class("echo-dock-active");
            self.private_tabs_button
                .remove_css_class("echo-dock-active");
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
        assert_eq!(NORMAL_TABS_LABEL, "Обычные вкладки");
        assert_eq!(PRIVATE_TABS_LABEL, "Приватные вкладки");
    }
}
