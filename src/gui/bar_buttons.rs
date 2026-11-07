//! Bar page: custom buttons (emoji or picture; sound, command and app on a
//! left or right click). The bar calls `mangowm-mod bar button run`, so
//! action edits need no restart; the live update skips unchanged files.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use anyhow::Context;
use gtk::gio;
use gtk::glib::{self, clone};

use mangowm_mod::bar::buttons::{import_picture, BarButton, Click, ClickAction};
use mangowm_mod::bar::model::{Backend, BarConfig, SECTIONS};

use super::BarPage;

const SECTION_NAMES: &[&str] = &["Left", "Center", "Right", "Not in the bar"];

pub struct Buttons {
    group: adw::PreferencesGroup,
    add: gtk::Button,
    rows: RefCell<Vec<gtk::Widget>>,
    /// What the rows were built for.
    built_for: RefCell<String>,
    /// Expand this button's row on the next rebuild (a new button).
    open: RefCell<Option<String>>,
    /// Installed apps: (desktop id, name), sorted by name.
    apps: Vec<(String, String)>,
    /// "None" + the app names, shared by every app row.
    app_model: gtk::StringList,
    building: Cell<bool>,
}

fn installed_apps() -> Vec<(String, String)> {
    let mut apps: Vec<(String, String)> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .filter_map(|a| Some((a.id()?.to_string(), a.display_name().to_string())))
        .collect();
    apps.sort_by_key(|(_, n)| n.to_lowercase());
    apps.dedup_by(|a, b| a.0 == b.0);
    apps
}

impl Buttons {
    pub fn new(page: &adw::PreferencesPage) -> Self {
        let add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add a button")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Custom Buttons")
            .description(
                "An emoji or picture in the bar that plays a sound, runs a command and/or \
                 opens an app. Move them like any module above",
            )
            .header_suffix(&add)
            .build();
        page.add(&group);
        let apps = installed_apps();
        let names: Vec<&str> = std::iter::once("None")
            .chain(apps.iter().map(|(_, n)| n.as_str()))
            .collect();
        let app_model = gtk::StringList::new(&names);
        Self {
            group,
            add,
            rows: RefCell::new(Vec::new()),
            built_for: RefCell::new(String::new()),
            open: RefCell::new(None),
            apps,
            app_model,
            building: Cell::new(false),
        }
    }
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn click_name(click: Click) -> &'static str {
    match click {
        Click::Left => "Left click",
        Click::Right => "Right click",
    }
}

impl BarPage {
    pub(super) fn connect_buttons(self: &Rc<Self>) {
        self.buttons.add.connect_clicked(clone!(
            #[weak(rename_to = st)]
            self,
            move |_| {
                let mut new_id = None;
                st.edit(|bar| {
                    let button = BarButton {
                        emoji: "⭐".to_string(),
                        right_same: true,
                        ..Default::default()
                    };
                    new_id = Some(bar.add_button(button, "right")?);
                    Ok(())
                });
                if let Some(id) = new_id {
                    *st.buttons.open.borrow_mut() = Some(id.clone());
                    st.buttons.built_for.borrow_mut().clear();
                    st.load();
                }
            }
        ));
    }

    /// Change one button with `f`, then reload the page.
    fn edit_button(
        self: &Rc<Self>,
        id: &str,
        f: impl FnOnce(&mut BarButton) -> anyhow::Result<()>,
    ) {
        if self.buttons.building.get() {
            return;
        }
        self.edit(|bar| f(bar.button_mut(id).context("this button was removed")?));
    }

    /// Rebuild the button rows when the buttons or their places changed.
    pub(super) fn load_buttons(self: &Rc<Self>, bar: &BarConfig) {
        let places: Vec<Option<&str>> = bar
            .buttons
            .iter()
            .map(|b| bar.button_section(&b.id))
            .collect();
        let sig = format!(
            "{:?} {:?} {}",
            bar.backend,
            places,
            serde_json::to_string(&bar.buttons).unwrap_or_default()
        );
        if *self.buttons.built_for.borrow() == sig {
            return;
        }
        *self.buttons.built_for.borrow_mut() = sig;
        let open = self.buttons.open.take();
        let expanded: Vec<String> = self
            .buttons
            .rows
            .borrow()
            .iter()
            .filter_map(|r| r.downcast_ref::<adw::ExpanderRow>())
            .filter(|r| r.is_expanded())
            .map(|r| r.widget_name().to_string())
            .collect();
        for r in self.buttons.rows.borrow_mut().drain(..) {
            self.buttons.group.remove(&r);
        }
        self.buttons.building.set(true);
        let mut rows: Vec<gtk::Widget> = Vec::new();
        for b in &bar.buttons {
            let exp = self.button_row(bar, b);
            exp.set_expanded(expanded.contains(&b.id) || open.as_deref() == Some(&b.id));
            // the rows are built when the button is first opened
            if exp.is_expanded() {
                self.fill_button(&exp, bar, b);
            } else {
                let filled = Cell::new(false);
                let id = b.id.clone();
                exp.connect_expanded_notify(clone!(
                    #[weak(rename_to = st)]
                    self,
                    move |exp| {
                        if exp.is_expanded() && !filled.replace(true) {
                            let bar = st.bar();
                            if let Some(b) = bar.button(&id) {
                                st.buttons.building.set(true);
                                st.fill_button(exp, &bar, b);
                                st.buttons.building.set(false);
                            }
                        }
                    }
                ));
            }
            self.buttons.group.add(&exp);
            rows.push(exp.upcast());
        }
        if rows.is_empty() {
            let empty = adw::ActionRow::builder()
                .title("No buttons")
                .css_classes(["empty-row"])
                .build();
            self.buttons.group.add(&empty);
            rows.push(empty.upcast());
        }
        self.buttons.building.set(false);
        *self.buttons.rows.borrow_mut() = rows;
    }

    fn button_row(self: &Rc<Self>, bar: &BarConfig, b: &BarButton) -> adw::ExpanderRow {
        let id = b.id.clone();
        let place = bar.button_section(&id);
        let right = if b.right_same {
            "same".to_string()
        } else {
            b.right.summary()
        };
        let title = if b.label.is_empty() {
            format!("{}  Button {}", b.text(), id)
        } else {
            format!("{}  {}", b.text(), b.label)
        };
        let exp = adw::ExpanderRow::builder()
            .title(title)
            .subtitle(format!(
                "{} · left: {} · right: {}",
                place
                    .and_then(|p| SECTIONS.iter().position(|s| *s == p))
                    .map(|i| SECTION_NAMES[i])
                    .unwrap_or("Not in the bar"),
                b.left.summary(),
                right
            ))
            .use_markup(false)
            .build();
        exp.set_widget_name(&id);
        exp
    }

    /// The rows inside a button's expander.
    fn fill_button(self: &Rc<Self>, exp: &adw::ExpanderRow, bar: &BarConfig, b: &BarButton) {
        let id = b.id.clone();
        let place = bar.button_section(&id);
        exp.add_row(&self.icon_row(bar, b));

        let label = adw::EntryRow::builder()
            .title("Label (tooltip in Waybar; the text when there is no icon)")
            .show_apply_button(true)
            .build();
        label.set_text(&b.label);
        label.connect_apply(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |r| {
                let text = r.text().trim().to_string();
                st.edit_button(&id, |b| {
                    b.label = text;
                    Ok(())
                });
            }
        ));
        exp.add_row(&label);

        let section = adw::ComboRow::builder()
            .title("Section")
            .model(&gtk::StringList::new(SECTION_NAMES))
            .build();
        section.set_selected(
            place
                .and_then(|p| SECTIONS.iter().position(|s| *s == p))
                .unwrap_or(3) as u32,
        );
        section.connect_selected_notify(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |r| {
                if st.buttons.building.get() {
                    return;
                }
                let target = SECTIONS.get(r.selected() as usize).copied();
                let id = id.clone();
                glib::idle_add_local_once(move || {
                    st.edit(|bar| bar.set_button_section(&id, target));
                });
            }
        ));
        exp.add_row(&section);

        for row in self.action_rows(b, Click::Left) {
            exp.add_row(&row);
        }
        let same = adw::SwitchRow::builder()
            .title("Right click does the same as left click")
            .active(b.right_same)
            .build();
        same.connect_active_notify(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |r| {
                let on = r.is_active();
                st.edit_button(&id, |b| {
                    b.right_same = on;
                    Ok(())
                });
            }
        ));
        exp.add_row(&same);
        if !b.right_same {
            for row in self.action_rows(b, Click::Right) {
                exp.add_row(&row);
            }
        }

        let remove = adw::ButtonRow::builder()
            .title("Remove Button")
            .css_classes(["destructive-action"])
            .build();
        remove.connect_activated(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |_| st.confirm_remove_button(&id)
        ));
        exp.add_row(&remove);
    }

    /// Icon: an emoji from the chooser, or a picture (copied to the data dir).
    fn icon_row(self: &Rc<Self>, bar: &BarConfig, b: &BarButton) -> adw::ActionRow {
        let id = b.id.clone();
        let subtitle = if b.picture.is_empty() {
            "Emoji".to_string()
        } else if bar.backend == Backend::Mangobar {
            format!(
                "Picture {} · Mangobar cannot draw pictures: it shows {} instead. \
                 Pictures need Waybar",
                file_name(&b.picture),
                if b.emoji.is_empty() {
                    "the label"
                } else {
                    "the emoji"
                }
            )
        } else {
            format!("Picture {}", file_name(&b.picture))
        };
        let row = adw::ActionRow::builder()
            .title("Icon")
            .subtitle(subtitle)
            .use_markup(false)
            .build();
        if b.picture.is_empty() {
            row.add_suffix(
                &gtk::Label::builder()
                    .label(b.text())
                    .css_classes(["title-2"])
                    .valign(gtk::Align::Center)
                    .build(),
            );
        } else {
            let img = gtk::Image::from_file(&b.picture);
            img.set_pixel_size(24);
            row.add_suffix(&img);
        }

        let chooser = gtk::EmojiChooser::new();
        chooser.connect_emoji_picked(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |_, emoji| {
                let emoji = emoji.to_string();
                let id = id.clone();
                // after the popover closes: the rows are rebuilt
                glib::idle_add_local_once(move || {
                    st.edit_button(&id, |b| {
                        b.emoji = emoji;
                        b.picture.clear();
                        Ok(())
                    });
                });
            }
        ));
        let pick = gtk::MenuButton::builder()
            .icon_name("face-smile-symbolic")
            .tooltip_text("Pick an emoji")
            .valign(gtk::Align::Center)
            .popover(&chooser)
            .css_classes(["flat", "circular"])
            .build();
        row.add_suffix(&pick);

        let picture = icon_button("image-x-generic-symbolic", "Use a picture (Waybar)");
        picture.connect_clicked(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |_| st.choose_picture(&id)
        ));
        row.add_suffix(&picture);
        if !b.picture.is_empty() {
            let clear = icon_button("edit-clear-symbolic", "Back to the emoji");
            clear.connect_clicked(clone!(
                #[weak(rename_to = st)]
                self,
                #[strong]
                id,
                move |_| {
                    let id = id.clone();
                    glib::idle_add_local_once(move || {
                        st.edit_button(&id, |b| {
                            b.picture.clear();
                            Ok(())
                        });
                    });
                }
            ));
            row.add_suffix(&clear);
        }
        row
    }

    /// Sound, command and app rows of one click.
    fn action_rows(self: &Rc<Self>, b: &BarButton, click: Click) -> Vec<gtk::Widget> {
        let id = b.id.clone();
        let action = match click {
            Click::Left => b.left.clone(),
            Click::Right => b.right.clone(),
        };
        let name = click_name(click);
        let mut rows: Vec<gtk::Widget> = Vec::new();

        let sound = adw::ActionRow::builder()
            .title(format!("{}: sound", name))
            .subtitle(if action.sound.is_empty() {
                "None".to_string()
            } else {
                file_name(&action.sound)
            })
            .use_markup(false)
            .build();
        let choose = icon_button("audio-x-generic-symbolic", "Choose a sound file");
        choose.connect_clicked(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |_| st.choose_sound(&id, click)
        ));
        sound.add_suffix(&choose);
        if !action.sound.is_empty() {
            let clear = icon_button("edit-clear-symbolic", "No sound");
            clear.connect_clicked(clone!(
                #[weak(rename_to = st)]
                self,
                #[strong]
                id,
                move |_| {
                    let id = id.clone();
                    glib::idle_add_local_once(move || {
                        st.edit_button(&id, |b| {
                            click_action(b, click).sound.clear();
                            Ok(())
                        });
                    });
                }
            ));
            sound.add_suffix(&clear);
        }
        rows.push(sound.upcast());

        let command = adw::EntryRow::builder()
            .title(format!("{}: shell command", name))
            .show_apply_button(true)
            .build();
        command.set_text(&action.command);
        command.connect_apply(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |r| {
                let text = r.text().trim().to_string();
                st.edit_button(&id, |b| {
                    click_action(b, click).command = text;
                    Ok(())
                });
            }
        ));
        rows.push(command.upcast());

        let app = adw::ComboRow::builder()
            .title(format!("{}: open app", name))
            .model(&self.buttons.app_model)
            .enable_search(true)
            .expression(gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                None::<gtk::Expression>,
                "string",
            ))
            .build();
        let current = self
            .buttons
            .apps
            .iter()
            .position(|(a, _)| *a == action.app)
            .map(|i| i + 1)
            .unwrap_or(0);
        if current == 0 && !action.app.is_empty() {
            app.set_subtitle(&format!("{} (not installed)", action.app));
        }
        app.set_selected(current as u32);
        let ids: Vec<String> = self.buttons.apps.iter().map(|(a, _)| a.clone()).collect();
        app.connect_selected_notify(clone!(
            #[weak(rename_to = st)]
            self,
            #[strong]
            id,
            move |r| {
                if st.buttons.building.get() {
                    return;
                }
                let picked = match r.selected() as usize {
                    0 => String::new(),
                    i => ids.get(i - 1).cloned().unwrap_or_default(),
                };
                let id = id.clone();
                glib::idle_add_local_once(move || {
                    st.edit_button(&id, |b| {
                        click_action(b, click).app = picked;
                        Ok(())
                    });
                });
            }
        ));
        rows.push(app.upcast());
        rows
    }

    fn window(&self) -> Option<gtk::Window> {
        self.page.root().and_downcast::<gtk::Window>()
    }

    fn choose_picture(self: &Rc<Self>, id: &str) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Pictures"));
        filter.add_pixbuf_formats();
        let dialog = file_dialog("Button Picture", &filter);
        let (st, id) = (self.clone(), id.to_string());
        dialog.open(self.window().as_ref(), gio::Cancellable::NONE, move |res| {
            let Some(path) = res.ok().and_then(|f| f.path()) else {
                return;
            };
            match import_picture(&st.ctx.data_dir, &id, &path) {
                Ok(copy) => st.edit_button(&id, |b| {
                    b.picture = copy.display().to_string();
                    Ok(())
                }),
                Err(e) => st.ctx.toast(&format!("{:#}", e)),
            }
        });
    }

    fn choose_sound(self: &Rc<Self>, id: &str, click: Click) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Sounds"));
        filter.add_mime_type("audio/*");
        let dialog = file_dialog("Button Sound", &filter);
        let (st, id) = (self.clone(), id.to_string());
        dialog.open(self.window().as_ref(), gio::Cancellable::NONE, move |res| {
            let Some(path) = res.ok().and_then(|f| f.path()) else {
                return;
            };
            st.edit_button(&id, |b| {
                click_action(b, click).sound = path.display().to_string();
                Ok(())
            });
        });
    }

    fn confirm_remove_button(self: &Rc<Self>, id: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Remove Button?")
            .body("The button and its actions are removed from the bar.")
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("remove", "Remove");
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        let (st, id) = (self.clone(), id.to_string());
        dialog.connect_response(Some("remove"), move |_, _| {
            st.edit(|bar| {
                bar.remove_button(&id);
                Ok(())
            });
        });
        dialog.present(Some(&self.page));
    }
}

fn click_action(b: &mut BarButton, click: Click) -> &mut ClickAction {
    match click {
        Click::Left => &mut b.left,
        Click::Right => &mut b.right,
    }
}

fn file_dialog(title: &str, filter: &gtk::FileFilter) -> gtk::FileDialog {
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(filter);
    gtk::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .default_filter(filter)
        .build()
}
