// SPDX-License-Identifier: GPL-3.0-or-later

//! Compress and extract dialogs for the browser view.
//!
//! These methods live on [`ViewState`] and start work through the shared
//! [`crate::app::Browser`] controller; they must not create a separate archive
//! pipeline. A [`crate::app::BrowserEvent::ArchivePasswordRequired`] event
//! reopens [`ViewState::show_extract_password_dialog`].
//!
//! # Entry points
//!
//! - [`ViewState::show_compress_dialog`]
//! - [`ViewState::extract_entry`]
//! - [`ViewState::show_extract_to_dialog`]
//! - [`ViewState::show_extract_password_dialog`]

use crate::adapters::gio_file_for_location;
use crate::model::{FileEntry, Location};
use crate::services::{ArchiveAction, ArchiveFormat, TransferConflict, validate_basename};
use crate::ui::browser::ViewState;
use crate::ui::browser::destination::{
    folder_input_path, resolve_destination_path, setup_transfer_search,
};
use crate::ui::browser::entry::{entry_kind_summary, item_count_label};
use crate::ui::browser::inline_edit::update_basename_validation;
use crate::ui::browser::paths::compact_display_path;
use crate::ui::controls::{
    ModalTone, form_entry, form_label, form_password_entry, message_dialog_description,
    message_dialog_layout, modal_layout, segmented_control,
};
use crate::ui::modal::{
    ModalHost, dismiss_modal_layer, modal_layer, show_error_dialog, submit_on_enter,
};
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

/// Basename used when creating the archive, with `format`'s extension removed.
///
/// Typing `backup.zip` while [`ArchiveFormat::Zip`] is selected yields `backup`,
/// so the committed file is `backup.zip` rather than `backup.zip.zip`. Matching
/// is ASCII case-insensitive (`backup.ZIP` also yields `backup`). Suffixes
/// that do not match [`ArchiveFormat::extension`] are left intact.
fn normalized_archive_name(name: &str, format: ArchiveFormat) -> String {
    let suffix = format!(".{}", format.extension());
    let Some(idx) = name.len().checked_sub(suffix.len()) else {
        return name.to_owned();
    };
    // `str::get` returns `None` when `idx` is not a char boundary, so a
    // CJK or emoji name shorter than, or not ending in, the suffix cannot panic.
    match name.get(idx..) {
        Some(tail) if tail.eq_ignore_ascii_case(&suffix) => name[..idx].to_owned(),
        _ => name.to_owned(),
    }
}

/// Whether `destination` already contains a child named `archive_name`.
///
/// Collision checks use the final filename, including the format extension.
fn archive_has_collision(destination: &Location, archive_name: &str) -> bool {
    gio_file_for_location(destination)
        .child(archive_name)
        .query_exists(None::<&gio::Cancellable>)
}

impl ViewState {
    /// Builds shared chrome for a compress or extract dialog.
    ///
    /// Returns the modal body, confirm button, and a dismiss callback. When no
    /// [`ModalHost`] overlay exists, returns inert widgets so callers can still
    /// wire handlers without showing a dialog.
    ///
    /// # Arguments
    ///
    /// * `title` - Dialog heading
    /// * `subtitle` - Secondary line under the heading
    /// * `confirm_label` - Confirm button text
    /// * `block_dismiss` - When `Some` and it returns `true`, backdrop clicks do
    ///   not close the dialog (used for dirty forms). Escape and cancel/close
    ///   still dismiss.
    fn build_archive_modal(
        self: &Rc<Self>,
        title: &str,
        subtitle: &str,
        confirm_label: &str,
        block_dismiss: Option<Rc<dyn Fn() -> bool>>,
        on_cancel: Option<Rc<dyn Fn()>>,
    ) -> (gtk::Box, gtk::Button, Rc<dyn Fn()>) {
        let Some(ModalHost {
            overlay: window_overlay,
            blurred_root,
        }) = ModalHost::blurred_for(&self.overlay)
        else {
            return (gtk::Box::default(), gtk::Button::default(), Rc::new(|| {}));
        };

        let layout = modal_layout(
            crate::assets::icons::FILE_ARCHIVE,
            title,
            subtitle,
            confirm_label,
        );
        let layer = modal_layer(
            &layout.content,
            &window_overlay,
            blurred_root.clone(),
            block_dismiss,
        );
        window_overlay.add_overlay(&layer);

        let dismiss: Rc<dyn Fn()> = Rc::new({
            let layer = layer.clone();
            let overlay = window_overlay.clone();
            let root = blurred_root.clone();
            move || dismiss_modal_layer(&layer, &overlay, root.as_ref())
        });
        let cancel: Rc<dyn Fn()> = Rc::new({
            let dismiss = dismiss.clone();
            let on_cancel = on_cancel.clone();
            move || {
                if let Some(on_cancel) = &on_cancel {
                    on_cancel();
                }
                dismiss();
            }
        });
        let dismiss_for_cancel = cancel.clone();
        layout.cancel.connect_clicked(move |_| dismiss_for_cancel());
        let dismiss_for_close = cancel.clone();
        layout.close.connect_clicked(move |_| dismiss_for_close());
        let escape = gtk::EventControllerKey::new();
        let dismiss_for_escape = cancel;
        escape.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                dismiss_for_escape();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        layer.add_controller(escape);
        (layout.body, layout.confirm, dismiss)
    }

    /// Starts compression, prompting to replace when the target already exists.
    ///
    /// Uses [`TransferConflict::FailIfExists`] when the name is free. On a
    /// collision, shows a replace confirmation instead of overwriting. If that
    /// prompt cannot be hosted, the operation is not started.
    ///
    /// # Arguments
    ///
    /// * `entries` - Items to include in the archive
    /// * `destination` - Directory that will receive the archive
    /// * `archive_name` - Basename without the format extension
    /// * `format` - Archive format to write
    fn start_compression(
        self: &Rc<Self>,
        entries: Vec<FileEntry>,
        destination: Location,
        archive_name: String,
        format: ArchiveFormat,
    ) {
        let final_name = format.archive_filename(&archive_name);
        let sources = entries
            .iter()
            .map(|entry| entry.location.clone())
            .collect::<Vec<_>>();
        if !archive_has_collision(&destination, &final_name) {
            self.browser.archive(
                destination,
                ArchiveAction::Compress {
                    sources,
                    archive_name,
                    format,
                    conflict: TransferConflict::FailIfExists,
                },
            );
            return;
        }
        let Some(ModalHost {
            overlay: window_overlay,
            blurred_root,
        }) = ModalHost::blurred_for(&self.overlay)
        else {
            return;
        };
        let layout = message_dialog_layout(
            crate::assets::icons::FILE_ARCHIVE,
            "File already exists",
            &final_name,
            "Replace",
            ModalTone::Danger,
        );
        layout.body.append(&message_dialog_description(&format!(
            "An archive named “{final_name}” already exists in {}. Replacing it will overwrite its contents.",
            compact_display_path(&destination)
        )));
        let content = layout.content;
        let close = layout.close;
        let cancel = layout.cancel;
        let replace = layout.confirm;
        let layer = modal_layer(&content, &window_overlay, blurred_root.clone(), None);
        window_overlay.add_overlay(&layer);

        for button in [&close, &cancel] {
            let dismissed_layer = layer.clone();
            let dismissed_overlay = window_overlay.clone();
            let dismissed_root = blurred_root.clone();
            let browser = self.browser.clone();
            button.connect_clicked(move |_| {
                dismiss_modal_layer(
                    &dismissed_layer,
                    &dismissed_overlay,
                    dismissed_root.as_ref(),
                );
                browser.focus_active();
            });
        }

        let replaced_layer = layer.clone();
        let replaced_overlay = window_overlay.clone();
        let replaced_root = blurred_root.clone();
        let browser = self.browser.clone();
        replace.connect_clicked(move |_| {
            dismiss_modal_layer(&replaced_layer, &replaced_overlay, replaced_root.as_ref());
            browser.archive(
                destination.clone(),
                ArchiveAction::Compress {
                    sources: sources.clone(),
                    archive_name: archive_name.clone(),
                    format,
                    conflict: TransferConflict::ReplaceExisting,
                },
            );
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let escaped_layer = layer.clone();
        let escaped_overlay = window_overlay;
        let escaped_root = blurred_root;
        let enter_replace = replace.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                dismiss_modal_layer(&escaped_layer, &escaped_overlay, escaped_root.as_ref());
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                enter_replace.emit_clicked();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        layer.add_controller(keys);
        replace.grab_focus();
    }

    /// Opens the compress dialog for the selected `entries`.
    ///
    /// Returns immediately when the selection is empty or any entry is not a
    /// native path; archive creation is a local operation. The destination is
    /// the first entry's parent, then the active location, then the home
    /// directory. Compress does not offer a password: `exarch_core` cannot write
    /// encrypted archives. The typed name is normalized with
    /// [`normalized_archive_name`] and checked with [`validate_basename`] before
    /// [`Self::start_compression`].
    pub(super) fn show_compress_dialog(self: &Rc<Self>, entries: Vec<FileEntry>) {
        if entries.is_empty()
            || entries
                .iter()
                .any(|entry| entry.location.native_path().is_none())
        {
            return;
        }
        let destination = entries[0]
            .location
            .parent()
            .or_else(|| self.browser.active_location())
            .unwrap_or_else(|| Location::local(glib::home_dir()));

        let default_name = if entries.len() == 1 {
            entries[0].display_name.clone()
        } else {
            "archive".to_owned()
        };

        let title = format!("Compress {}", item_count_label(entries.len()));
        let subtitle = entry_kind_summary(&entries);

        let name_entry = form_entry();
        name_entry.set_text(&default_name);
        name_entry.connect_changed(|field| {
            update_basename_validation(field);
        });
        let compress_default_name = default_name.clone();
        let dirty_name = name_entry.clone();
        let (body, confirm, dismiss) = self.build_archive_modal(
            &title,
            &subtitle,
            "Compress",
            Some(Rc::new(move || dirty_name.text() != compress_default_name)),
            None,
        );

        let name_label = form_label("Archive name");
        body.append(&name_label);
        body.append(&name_entry);

        let format_label = form_label("Format");
        let (format_control, format_options) = segmented_control(&["ZIP", "TAR.GZ", "TAR"], 0);
        let selected_format = Rc::new(Cell::new(ArchiveFormat::Zip));
        body.append(&format_label);
        body.append(&format_control);

        for (option, format) in format_options.into_iter().zip([
            ArchiveFormat::Zip,
            ArchiveFormat::TarGz,
            ArchiveFormat::Tar,
        ]) {
            let selected_format = selected_format.clone();
            option.connect_toggled(move |option| {
                if option.is_active() {
                    selected_format.set(format);
                }
            });
        }

        let state = Rc::downgrade(self);
        let confirm_entries = entries.clone();
        let confirm_destination = destination.clone();
        let name_for_confirm = name_entry.clone();
        let format_for_confirm = selected_format.clone();
        let dismiss_for_confirm = dismiss.clone();
        confirm.connect_clicked(move |_| {
            let name = name_for_confirm.text().to_string();
            let format = format_for_confirm.get();
            let archive_name = normalized_archive_name(&name, format);
            if let Err(message) = validate_basename(&archive_name) {
                name_for_confirm.add_css_class("error");
                name_for_confirm.set_tooltip_text(Some(message));
                name_for_confirm.grab_focus();
                return;
            }
            dismiss_for_confirm();
            if let Some(state) = state.upgrade() {
                state.start_compression(
                    confirm_entries.clone(),
                    confirm_destination.clone(),
                    archive_name,
                    format,
                );
            }
        });
        submit_on_enter(&body, &confirm);
        name_entry.grab_focus();
    }

    /// Extracts `entry` into its parent directory ("Extract here").
    ///
    /// Returns immediately when the archive is not a native path. Archives
    /// without a parent show an error instead of extracting. The first attempt
    /// is sent without a password; a later
    /// [`crate::app::BrowserEvent::ArchivePasswordRequired`] reopens
    /// [`Self::show_extract_password_dialog`].
    pub(super) fn extract_entry(self: &Rc<Self>, entry: FileEntry) {
        if entry.location.native_path().is_none() {
            return;
        }
        let Some(parent) = entry.location.parent() else {
            show_error_dialog(
                &self.overlay,
                "Cannot extract",
                "This archive has no parent directory.",
            );
            return;
        };
        self.browser.archive(
            parent,
            ArchiveAction::Extract {
                archive: entry.location,
                password: None,
            },
        );
    }

    /// Opens the "Extract to" folder picker for `entry`.
    ///
    /// Returns immediately when the archive is not a native path. Confirm
    /// creates the typed destination if it does not exist, then extracts into
    /// that folder and navigates there when the operation finishes. Password
    /// retry uses [`crate::app::BrowserEvent::ArchivePasswordRequired`], the
    /// same as [`Self::extract_entry`].
    pub(super) fn show_extract_to_dialog(self: &Rc<Self>, entry: FileEntry) {
        if entry.location.native_path().is_none() {
            return;
        }
        let base = entry
            .location
            .parent()
            .and_then(|p| p.native_path().map(Path::to_path_buf))
            .unwrap_or_else(glib::home_dir);
        let field = form_entry();
        field.set_hexpand(true);
        field.set_placeholder_text(Some("Search for a folder…"));
        field.set_text(&folder_input_path(&base));
        field.set_position(-1);
        let extract_initial_text = folder_input_path(&base);
        let dirty_field = field.clone();
        let (body, confirm, dismiss) = self.build_archive_modal(
            "Extract to",
            &entry.display_name,
            "Extract here",
            Some(Rc::new(move || dirty_field.text() != extract_initial_text)),
            None,
        );
        let field_label = form_label("Destination folder");
        body.append(&field_label);
        body.append(&field);

        let suggestions = gtk::Box::new(gtk::Orientation::Vertical, 2);
        suggestions.add_css_class("transfer-suggestions");
        let suggestion_scroll = gtk::ScrolledWindow::builder()
            .child(&suggestions)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(150)
            .max_content_height(220)
            .propagate_natural_height(true)
            .build();
        suggestion_scroll.add_css_class("transfer-suggestion-scroll");
        body.append(&suggestion_scroll);
        let error = gtk::Label::new(None);
        error.add_css_class("form-message");
        error.add_css_class("error");
        error.set_wrap(true);
        error.set_xalign(0.0);
        error.set_visible(false);
        body.append(&error);

        let generation = Rc::new(Cell::new(0_u64));
        let suggestions_box = suggestions.clone();
        let extract_error = error.clone();
        setup_transfer_search(
            &field,
            &suggestions_box,
            &generation,
            base.clone(),
            self.browser.preferences().show_hidden,
            move |field| {
                field.remove_css_class("error");
                extract_error.set_visible(false);
            },
        );

        let extract_state = self.clone();
        let confirm_field = field.clone();
        let confirm_error = error.clone();
        let confirm_base = base.clone();
        let extract_entry = entry.clone();
        let dismiss_for_confirm = dismiss.clone();
        confirm.connect_clicked(move |_| {
            let path =
                resolve_destination_path(&confirm_field.text(), &confirm_base, &glib::home_dir());
            if path.exists() && !path.is_dir() {
                confirm_error.set_text("The destination exists, but it is not a folder.");
                confirm_error.set_visible(true);
                confirm_field.add_css_class("error");
                confirm_field.grab_focus();
                return;
            }
            if !path.exists()
                && let Err(e) = std::fs::create_dir_all(&path)
            {
                confirm_error.set_text(&format!("Could not create folder: {e}"));
                confirm_error.set_visible(true);
                confirm_field.add_css_class("error");
                return;
            }
            let dest = Location::local(path);
            extract_state.pending_navigate.replace(Some(dest.clone()));
            extract_state.browser.archive(
                dest,
                ArchiveAction::Extract {
                    archive: extract_entry.location.clone(),
                    password: None,
                },
            );
            dismiss_for_confirm();
        });

        submit_on_enter(&body, &confirm);
        field.grab_focus();
    }

    /// Prompts for a password after a password-capable extract failed.
    ///
    /// Shown when extract reports [`crate::app::BrowserEvent::ArchivePasswordRequired`].
    /// An empty field retries `archive` into `destination` with no password.
    pub(super) fn show_extract_password_dialog(
        self: &Rc<Self>,
        archive: Location,
        destination: Location,
    ) {
        let subtitle = archive
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| compact_display_path(&archive));
        let password_entry = form_password_entry();
        password_entry.set_show_peek_icon(true);
        let dirty_password = password_entry.clone();
        let drop_pending_navigate = {
            let state = self.clone();
            Rc::new(move || {
                state.pending_navigate.take();
            }) as Rc<dyn Fn()>
        };
        let (body, confirm, dismiss) = self.build_archive_modal(
            "Extract",
            &subtitle,
            "Extract",
            Some(Rc::new(move || !dirty_password.text().is_empty())),
            Some(drop_pending_navigate),
        );

        let password_label = form_label("Password");
        body.append(&password_label);
        body.append(&password_entry);

        let browser = self.browser.clone();
        let password_for_confirm = password_entry.clone();
        let dismiss_for_confirm = dismiss.clone();
        confirm.connect_clicked(move |_| {
            let pw = password_for_confirm.text().to_string();
            let password = if pw.is_empty() { None } else { Some(pw) };
            dismiss_for_confirm();
            browser.archive(
                destination.clone(),
                ArchiveAction::Extract {
                    archive: archive.clone(),
                    password,
                },
            );
        });
        submit_on_enter(&body, &confirm);
        password_entry.grab_focus();
    }
}

#[cfg(test)]
mod tests;
