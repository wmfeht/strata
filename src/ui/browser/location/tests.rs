// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{
    model::Location,
    test_support::gtk_test,
    ui::{
        browser::{BrowserView, PeekBehavior},
        browser_modes::BrowserMode,
    },
};
use gtk::{gio, glib};
use std::rc::Rc;
use std::time::{Duration, Instant};

#[test]
fn password_storage_selection_maps_to_gio_values() {
    assert_eq!(password_save_for_selection(0), gio::PasswordSave::Never);
    assert_eq!(
        password_save_for_selection(1),
        gio::PasswordSave::ForSession
    );
    assert_eq!(
        password_save_for_selection(2),
        gio::PasswordSave::Permanently
    );
    assert_eq!(password_save_for_selection(99), gio::PasswordSave::Never);
}

#[test]
fn location_input_credentials_are_one_shot_and_never_saved() {
    let (location, credentials) = credentials_from_location_input("smb://alice:secret@host/share")
        .expect("credential URI should parse");
    let credentials = credentials.expect("credentials should be separated");

    assert_eq!(location, "smb://alice@host/share");
    assert_eq!(credentials.username, "alice");
    assert_eq!(credentials.password, "secret");
    assert_eq!(credentials.save, gio::PasswordSave::Never);
}

#[test]
fn remote_permission_denials_are_treated_as_authentication_failures() {
    let denied = glib::Error::new(gio::IOErrorEnum::PermissionDenied, "Permission denied");
    let smb_denied = glib::Error::new(
        gio::IOErrorEnum::Failed,
        "Failed to mount Windows share: Permission denied",
    );
    let remote = Location::uri("smb://host/share");
    assert!(mount_error_is_authentication_failure(&remote, &denied));
    assert!(mount_error_is_authentication_failure(&remote, &smb_denied,));
    assert!(!mount_error_is_authentication_failure(
        &Location::local("/root"),
        &denied,
    ));
}

#[test]
fn cancelling_the_credential_prompt_produces_no_error_message() {
    let location = Location::uri("smb://host/share");
    for kind in [gio::IOErrorEnum::Cancelled, gio::IOErrorEnum::FailedHandled] {
        let error = glib::Error::new(kind, "cancelled by the user");
        assert_eq!(mount_failure_message(&location, &error), None);
    }
}

#[test]
fn a_missing_backend_reports_which_package_to_install() {
    let location = Location::uri("smb://host/share");
    let error = glib::Error::new(gio::IOErrorEnum::NotSupported, "no handler for smb");
    let message = mount_failure_message(&location, &error).expect("should report a message");
    assert!(message.contains("gvfs-smb"));
}

#[test]
fn a_genuine_mount_failure_still_reports_an_error() {
    let location = Location::uri("smb://host/share");
    let error = glib::Error::new(gio::IOErrorEnum::HostNotFound, "no route to host");
    let message = mount_failure_message(&location, &error).expect("should report a message");
    assert!(message.contains("no route to host"));
}

#[test]
fn authentication_failure_without_a_backend_prompt_gets_login_fields() {
    let location = Location::uri("smb://host/share");
    let details = MountPromptDetails::fallback(&location);
    assert!(details.message.contains("smb://host/share"));
    assert!(details.flags.contains(gio::AskPasswordFlags::NEED_USERNAME));
    assert!(details.flags.contains(gio::AskPasswordFlags::NEED_DOMAIN));
    assert!(details.flags.contains(gio::AskPasswordFlags::NEED_PASSWORD));
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "location fixture did not settle");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn current_breadcrumb_label(breadcrumbs: &gtk::Box) -> String {
    let mut child = breadcrumbs.first_child();
    let mut current = String::new();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class("current-breadcrumb")
            && let Some(label) = widget.first_child().and_downcast::<gtk::Label>()
        {
            current = label.text().to_string();
        }
    }
    current
}

#[test]
fn columns_address_bar_follows_the_focused_miller_column() {
    gtk_test(
        "ui::browser::location::tests::columns_address_bar_follows_the_focused_miller_column",
        || {
            let fixture = tempfile::tempdir().expect("directory fixture");
            let alpha = fixture.path().join("alpha");
            let beta = alpha.join("beta");
            std::fs::create_dir_all(&beta).expect("nested folders");
            std::fs::write(beta.join("notes.txt"), b"body").expect("nested file");

            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                PeekBehavior::default(),
            );
            view.set_view_mode(BrowserMode::Columns);
            let window = gtk::Window::builder()
                .child(&view.widget())
                .default_width(1000)
                .default_height(500)
                .build();
            window.present();
            let browser = view.browser();
            let root = Location::local(fixture.path());
            let alpha_location = Location::local(&alpha);
            let beta_location = Location::local(&beta);
            browser.navigate(root.clone());
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| !snapshot.loading && snapshot.count >= 1)
            });
            browser.select(0, 0);
            browser.enter_focused_directory();
            wait_until(|| {
                browser
                    .column_snapshot(1)
                    .is_some_and(|snapshot| !snapshot.loading && snapshot.count >= 1)
            });
            browser.select(1, 0);
            browser.enter_focused_directory();
            wait_until(|| {
                browser
                    .column_snapshot(2)
                    .is_some_and(|snapshot| !snapshot.loading)
            });

            assert_eq!(
                view.state.location_entry.text().as_str(),
                beta_location.display_path()
            );
            assert_eq!(current_breadcrumb_label(&view.state.breadcrumbs), "beta");

            browser.focus_parent();
            assert_eq!(browser.active_depth(), Some(1));
            assert!(browser.column_snapshot(2).is_some());
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path()
            );
            assert_eq!(current_breadcrumb_label(&view.state.breadcrumbs), "alpha");

            browser.focus_parent();
            assert_eq!(browser.active_depth(), Some(0));
            assert!(browser.column_snapshot(2).is_some());
            assert_eq!(
                view.state.location_entry.text().as_str(),
                root.display_path()
            );
            assert_eq!(
                current_breadcrumb_label(&view.state.breadcrumbs),
                root.display_name()
            );

            view.state.columns.borrow()[1].list.grab_focus();
            wait_until(|| view.state.focused_column_depth() == Some(1));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path()
            );
            assert_eq!(current_breadcrumb_label(&view.state.breadcrumbs), "alpha");
            assert!(browser.column_snapshot(2).is_some());

            view.state.columns.borrow()[2].list.grab_focus();
            wait_until(|| view.state.focused_column_depth() == Some(2));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                beta_location.display_path()
            );
            assert_eq!(current_breadcrumb_label(&view.state.breadcrumbs), "beta");

            browser.clear_observer();
            window.destroy();
        },
    );
}
