//! Backups made before every apply, with restore (`mangowm-mod backup`).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use mangowm_mod::core::backup::BackupSummary;

use crate::ctx::Ctx;
use crate::logic::short_time;

fn restore(ctx: &Ctx, id: &str) {
    let res = {
        let mut mgr = ctx.mgr.borrow_mut();
        mgr.backups.restore(id).and_then(|files| {
            let status = mgr.ipc.reload();
            mgr.reload()?;
            Ok((files, status))
        })
    };
    match res {
        Ok((files, status)) => ctx.toast(&format!(
            "Restored backup {} ({} files) · {}",
            id,
            files.len(),
            status
        )),
        Err(e) => ctx.toast(&format!("Restore failed: {:#}", e)),
    }
    ctx.changed();
}

fn confirm(ctx: &Ctx, parent: &gtk::Widget, b: &BackupSummary) {
    let dialog = adw::AlertDialog::builder()
        .heading("Restore Backup?")
        .body(format!(
            "These files are overwritten with the copies from {}:\n{}",
            short_time(&b.created),
            b.files.join("\n")
        ))
        .default_response("cancel")
        .close_response("cancel")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("restore", "Restore");
    dialog.set_response_appearance("restore", adw::ResponseAppearance::Destructive);
    let (ctx, id) = (ctx.clone(), b.id.clone());
    dialog.connect_response(Some("restore"), move |_, _| restore(&ctx, &id));
    dialog.present(Some(parent));
}

pub fn page(ctx: &Ctx) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Backups")
        .description("config.conf and mango-mod.conf are copied here before every apply")
        .build();
    let group = adw::PreferencesGroup::new();
    page.add(&group);
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let shown: Rc<RefCell<Vec<String>>> = Rc::default();

    let refresh = {
        let ctx = ctx.clone();
        move || {
            let list = ctx.mgr.borrow().backups.list();
            let ids: Vec<String> = list.iter().map(|b| b.id.clone()).collect();
            if *shown.borrow() == ids && !rows.borrow().is_empty() {
                return;
            }
            for r in rows.borrow_mut().drain(..) {
                group.remove(&r);
            }
            group.set_description(Some(&match list.len() {
                1 => "1 backup".to_string(),
                n => format!("{} backups", n),
            }));
            let mut new_rows: Vec<gtk::Widget> = Vec::new();
            for b in &list {
                let files: Vec<&str> = b
                    .files
                    .iter()
                    .map(|f| f.rsplit('/').next().unwrap_or(f))
                    .collect();
                let row = adw::ActionRow::builder()
                    .title(short_time(&b.created))
                    .subtitle(format!("{} · {} · {}", b.label, files.join(", "), b.id))
                    .use_markup(false)
                    .build();
                row.add_prefix(
                    &gtk::Image::builder()
                        .icon_name("document-open-recent-symbolic")
                        .css_classes(["backup-icon"])
                        .build(),
                );
                let btn = gtk::Button::builder()
                    .label("Restore")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                let (c, s) = (ctx.clone(), b.clone());
                btn.connect_clicked(move |w| confirm(&c, w.upcast_ref(), &s));
                row.add_suffix(&btn);
                group.add(&row);
                new_rows.push(row.upcast());
            }
            if list.is_empty() {
                let row = adw::ActionRow::builder()
                    .title("No backups yet. Apply creates one first")
                    .css_classes(["empty-row"])
                    .build();
                group.add(&row);
                new_rows.push(row.upcast());
            }
            *rows.borrow_mut() = new_rows;
            *shown.borrow_mut() = ids;
        }
    };
    refresh();
    ctx.on_change(refresh);
    page
}
