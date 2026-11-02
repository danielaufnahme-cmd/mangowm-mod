//! Floating "unsaved changes" bar that slides up from the bottom.

use adw::prelude::*;

pub struct ApplyBar {
    pub widget: gtk::Revealer,
    label: gtk::Label,
    pub apply: gtk::Button,
    pub discard: gtk::Button,
}

impl ApplyBar {
    pub fn new() -> Self {
        let card = gtk::Box::builder()
            .spacing(6)
            .css_classes(["apply-bar"])
            .build();
        card.append(
            &gtk::Image::builder()
                .icon_name("document-edit-symbolic")
                .css_classes(["apply-bar-icon"])
                .build(),
        );
        let label = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["apply-bar-label"])
            .build();
        card.append(&label);
        let discard = gtk::Button::builder()
            .label("Discard")
            .tooltip_text("Drop all staged changes")
            .css_classes(["flat"])
            .build();
        let apply = gtk::Button::builder()
            .label("Apply")
            .tooltip_text("Write mango-mod.conf and reload MangoWM")
            .css_classes(["suggested-action"])
            .build();
        card.append(&discard);
        card.append(&apply);

        let widget = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(180)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(18)
            .child(&card)
            .build();
        Self {
            widget,
            label,
            apply,
            discard,
        }
    }

    /// Show the bar with `n` staged changes; hide it at 0.
    pub fn set_count(&self, n: usize) {
        self.label.set_label(&if n == 1 {
            "1 staged change".to_string()
        } else {
            format!("{} staged changes", n)
        });
        self.widget.set_reveal_child(n > 0);
    }
}
