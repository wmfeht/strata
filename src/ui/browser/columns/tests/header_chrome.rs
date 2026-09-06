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

#[test]
fn miller_header_actions_follow_the_active_column() {
    assert!(!miller_header_actions_expanded(0, Some(2), Some(2)));
    assert!(!miller_header_actions_expanded(1, Some(2), Some(2)));
    assert!(miller_header_actions_expanded(2, Some(2), Some(2)));
    assert!(!miller_header_actions_expanded(0, Some(2), Some(1)));
    assert!(miller_header_actions_expanded(1, Some(1), None));
    assert!(miller_header_actions_expanded(0, None, Some(0)));
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "column chrome did not settle");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn chrome_state(view: &BrowserView, depth: usize) -> (bool, bool, String) {
    let columns = view.state.columns.borrow();
    let column = &columns[depth];
    (
        column.header_actions.is_visible(),
        column.header_actions_overflow.is_visible(),
        column.header_actions_overflow.text().to_string(),
    )
}

#[test]
fn inactive_miller_columns_collapse_header_actions_instantly() {
    gtk_test(
        "ui::browser::columns::tests::header_chrome::inactive_miller_columns_collapse_header_actions_instantly",
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
                .default_width(1100)
                .default_height(500)
                .build();
            window.present();
            let browser = view.browser();
            let root = crate::model::Location::local(fixture.path());
            let alpha_location = crate::model::Location::local(&alpha);
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
            wait_until(|| view.state.columns.borrow().len() == 3);

            let collapsed = chrome_state(&view, 0);
            let middle = chrome_state(&view, 1);
            let active = chrome_state(&view, 2);
            assert!(!collapsed.0 && collapsed.1 && collapsed.2 == "…");
            assert!(!middle.0 && middle.1 && middle.2 == "…");
            assert!(active.0 && !active.1);

            view.state.hovered_column.set(Some(0));
            view.state.refresh_destination_style();
            assert!(!chrome_state(&view, 0).0 && chrome_state(&view, 0).1);
            assert!(!chrome_state(&view, 1).0 && chrome_state(&view, 1).1);
            assert!(chrome_state(&view, 2).0 && !chrome_state(&view, 2).1);

            view.state.hovered_column.set(None);
            view.state.refresh_destination_style();
            assert!(!chrome_state(&view, 0).0 && chrome_state(&view, 0).1);
            assert!(chrome_state(&view, 2).0 && !chrome_state(&view, 2).1);

            view.state.columns.borrow()[1].list.grab_focus();
            wait_until(|| view.state.focused_column_depth() == Some(1));
            view.state.refresh_destination_style();
            assert!(!chrome_state(&view, 0).0 && chrome_state(&view, 0).1);
            assert!(chrome_state(&view, 1).0 && !chrome_state(&view, 1).1);
            assert!(!chrome_state(&view, 2).0 && chrome_state(&view, 2).1);
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path()
            );
            assert!(browser.column_snapshot(2).is_some());

            browser.clear_observer();
            window.destroy();
        },
    );
}
