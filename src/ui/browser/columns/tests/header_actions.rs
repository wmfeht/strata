// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    model::Location,
    test_support::gtk_test,
    ui::{
        browser::{BrowserView, PeekBehavior},
        browser_modes::BrowserMode,
    },
};
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "column header fixture did not settle"
        );
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn header_visibility(view: &BrowserView) -> Vec<bool> {
    view.state
        .columns
        .borrow()
        .iter()
        .map(|column| column.header_actions.is_visible())
        .collect()
}

fn wait_for_loaded_column(browser: &crate::app::Browser, depth: usize) {
    wait_until(|| {
        browser
            .column_snapshot(depth)
            .is_some_and(|snapshot| !snapshot.loading)
    });
}

fn press_column_background(view: &BrowserView, depth: usize) {
    let surface = view.state.columns.borrow()[depth]
        .presentation
        .stack
        .clone();
    wait_until(|| surface.height() > 100);
    let controllers = surface.observe_controllers();
    let gesture = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index).and_downcast::<gtk::GestureClick>())
        .find(|gesture| gesture.button() == 1)
        .expect("background focus gesture");
    gesture.emit_by_name::<()>(
        "pressed",
        &[&1i32, &30.0f64, &(f64::from(surface.height()) - 40.0)],
    );
}

#[test]
fn only_the_focused_miller_column_shows_header_actions() {
    gtk_test(
        "ui::browser::columns::tests::header_actions::only_the_focused_miller_column_shows_header_actions",
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
            wait_for_loaded_column(&browser, 0);
            wait_until(|| header_visibility(&view) == [true]);

            browser.select(0, 0);
            browser.enter_focused_directory();
            wait_for_loaded_column(&browser, 1);
            browser.select(1, 0);
            browser.enter_focused_directory();
            wait_for_loaded_column(&browser, 2);
            wait_until(|| header_visibility(&view) == [false, false, true]);
            assert_eq!(
                view.state.location_entry.text().as_str(),
                beta_location.display_path()
            );

            browser.focus_parent();
            wait_until(|| header_visibility(&view) == [false, true, false]);
            assert_eq!(browser.active_depth(), Some(1));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path()
            );

            browser.focus_parent();
            wait_until(|| header_visibility(&view) == [true, false, false]);
            assert_eq!(browser.active_depth(), Some(0));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                root.display_path()
            );

            view.state.columns.borrow()[1].list.grab_focus();
            wait_until(|| header_visibility(&view) == [false, true, false]);
            assert_eq!(view.state.focused_column_depth(), Some(1));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                alpha_location.display_path()
            );

            view.state.columns.borrow()[2].list.grab_focus();
            wait_until(|| header_visibility(&view) == [false, false, true]);
            assert_eq!(view.state.focused_column_depth(), Some(2));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                beta_location.display_path()
            );

            press_column_background(&view, 0);
            wait_until(|| header_visibility(&view) == [true, false, false]);
            assert_eq!(browser.active_depth(), Some(0));
            assert_eq!(
                view.state.location_entry.text().as_str(),
                root.display_path()
            );
            assert!(browser.column_snapshot(2).is_some());

            browser.clear_observer();
            window.destroy();
        },
    );
}
