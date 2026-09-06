// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{
    test_support::gtk_test,
    ui::{
        browser::{BrowserView, PeekBehavior},
        browser_modes::BrowserMode,
    },
};
use gtk::glib;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "column header chrome fixture did not settle"
        );
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn open_nested_columns(
    view: &BrowserView,
    fixture: &tempfile::TempDir,
) -> (Location, Location, Location) {
    let alpha = fixture.path().join("alpha");
    let beta = alpha.join("beta");
    std::fs::create_dir_all(&beta).expect("nested folders");
    std::fs::write(beta.join("notes.txt"), b"body").expect("nested file");

    let browser = view.browser();
    let root = Location::local(alpha.parent().expect("fixture root"));
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
    (root, alpha_location, beta_location)
}

fn header_states(view: &BrowserView) -> Vec<(bool, bool)> {
    view.state
        .columns
        .borrow()
        .iter()
        .map(|column| {
            (
                column.header_actions_revealer.reveals_child(),
                column.header_overflow_revealer.reveals_child(),
            )
        })
        .collect()
}

#[test]
fn inactive_miller_columns_collapse_header_actions_until_hover_or_focus() {
    gtk_test(
        "ui::browser::columns::tests::header_chrome::inactive_miller_columns_collapse_header_actions_until_hover_or_focus",
        || {
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
            let fixture = tempfile::tempdir().expect("directory fixture");
            let (_root, alpha_location, beta_location) = open_nested_columns(&view, &fixture);
            let browser = view.browser();

            view.state.hovered_column.set(None);
            view.state.refresh_destination_style();
            wait_until(|| view.state.columns.borrow().len() == 3);

            let states = header_states(&view);
            assert_eq!(states.len(), 3);
            assert_eq!(states[0], (false, true), "root column starts collapsed");
            assert_eq!(states[1], (false, true), "middle column starts collapsed");
            assert_eq!(
                states[2],
                (true, false),
                "active column keeps the full icons bar"
            );
            assert_eq!(
                view.state.location_entry.text().as_str(),
                beta_location.display_path()
            );

            view.state.hovered_column.set(Some(0));
            view.state.refresh_destination_style();
            let states = header_states(&view);
            assert_eq!(states[0], (true, false), "hover rolls out the icons bar");
            assert_eq!(states[1], (false, true));
            assert_eq!(
                states[2],
                (true, false),
                "active column stays expanded during hover elsewhere"
            );

            view.state.hovered_column.set(None);
            view.state.refresh_destination_style();
            let states = header_states(&view);
            assert_eq!(
                states[0],
                (false, true),
                "inactive column collapses after hover ends"
            );
            assert_eq!(states[2], (true, false));

            view.state.columns.borrow()[1].list.grab_focus();
            wait_until(|| view.state.focused_column_depth() == Some(1));
            view.state.refresh_destination_style();
            let states = header_states(&view);
            assert_eq!(states[0], (false, true));
            assert_eq!(
                states[1],
                (true, false),
                "focused column expands without hover"
            );
            assert_eq!(
                states[2],
                (false, true),
                "previous active column collapses after focus moves"
            );
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path(),
                "address bar still follows the focused column"
            );
            assert_eq!(browser.active_depth(), Some(1));
            assert!(browser.column_snapshot(2).is_some());

            browser.clear_observer();
            window.destroy();
        },
    );
}
