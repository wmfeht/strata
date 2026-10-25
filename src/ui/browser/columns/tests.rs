// SPDX-License-Identifier: MIT

use super::*;
use crate::ui::browser::BrowserView;
use crate::ui::browser_modes::BrowserMode;
use crate::ui::preferences::PreferenceManager;

/// Call after creating a `BrowserView`: its exhaustive preference fixture seeds
/// `reduce_motion = true`.
fn animations_on() {
    crate::ui::motion::set_reduce_motion(false);
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(true);
    }
}

fn animations_off() {
    crate::ui::motion::set_reduce_motion(true);
}

fn pump_until(condition: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn pump_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct Columns {
    view: BrowserView,
    window: gtk::Window,
    _root: tempfile::TempDir,
}

impl Columns {
    fn new(depths: usize, window_width: i32) -> Self {
        PreferenceManager::seed_saved_preferences_for_test();
        let root = tempfile::tempdir().expect("columns fixture");
        let mut path = root.path().to_path_buf();
        let mut locations = Vec::new();
        for depth in 0..depths {
            path = path.join(format!("level-{depth}"));
            std::fs::create_dir_all(&path).expect("column directory");
            std::fs::write(path.join("note.txt"), b"note").expect("column entry");
            locations.push(Location::local(&path));
        }
        let view = BrowserView::new(
            Rc::new(crate::adapters::LocalFileSource),
            crate::ui::browser::PeekBehavior::default(),
        );
        view.set_view_mode(BrowserMode::Columns);
        let window = gtk::Window::builder()
            .default_width(window_width)
            .default_height(650)
            .child(&view.widget())
            .build();
        window.present();
        view.browser().navigate(Location::local(root.path()));
        pump_until(|| column_loaded(&view, 0), "root column load");
        for (depth, location) in locations.iter().take(depths.saturating_sub(1)).enumerate() {
            view.browser().descend(depth, location.clone());
            pump_until(|| column_loaded(&view, depth + 1), "descended column load");
        }
        pump_for(Duration::from_millis(40));
        Self {
            view,
            window,
            _root: root,
        }
    }

    fn shell(&self, depth: usize) -> gtk::Box {
        self.view
            .state
            .columns
            .borrow()
            .get(depth)
            .map(|column| column.shell.clone())
            .expect("column shell")
    }

    fn scroller(&self) -> gtk::ScrolledWindow {
        self.view.state.scroller.clone()
    }

    fn adjustment(&self) -> gtk::Adjustment {
        self.view.state.scroller.hadjustment()
    }
}

impl Drop for Columns {
    fn drop(&mut self) {
        PreferenceManager::shared().release_bindings_within(&self.view.widget());
        self.view.browser().clear_observer();
        self.window.destroy();
    }
}

fn column_loaded(view: &BrowserView, depth: usize) -> bool {
    view.browser()
        .column_snapshot(depth)
        .is_some_and(|column| !column.loading)
}

fn resize_gesture(scroller: &gtk::ScrolledWindow) -> gtk::GestureDrag {
    let controllers = scroller.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| {
            controllers
                .item(index)
                .and_then(|controller| controller.downcast::<gtk::GestureDrag>().ok())
        })
        .find(|gesture| gesture.name().as_deref() == Some("column-resize"))
        .expect("column-resize gesture")
}

fn shell_edge(scroller: &gtk::ScrolledWindow, shell: &gtk::Box) -> (f64, f64) {
    pump_until(
        || {
            shell
                .compute_bounds(scroller)
                .is_some_and(|bounds| bounds.width() > 0.0)
        },
        "column allocation",
    );
    let bounds = shell.compute_bounds(scroller).expect("column bounds");
    (
        f64::from(bounds.x() + bounds.width()) - 0.5,
        f64::from(bounds.y()) + 4.0,
    )
}

fn column_pane(shell: &gtk::Box) -> gtk::Widget {
    shell
        .first_child()
        .and_downcast::<gtk::Overlay>()
        .and_then(|overlay| overlay.child())
        .expect("column pane")
}

#[test]
fn reveal_column_does_not_move_an_already_visible_active_column() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::reveal_column_does_not_move_an_already_visible_active_column",
        || {
            let fixture = Columns::new(6, 700);
            animations_off();
            fixture.view.browser().set_active_column(5);
            let adjustment = fixture.adjustment();
            pump_until(|| adjustment.page_size() > 0.0, "viewport sizing");
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            pump_for(Duration::from_millis(40));
            let before = adjustment.value();
            fixture.view.state.reveal_column(fixture.shell(5));
            pump_for(Duration::from_millis(200));
            assert_eq!(
                adjustment.value(),
                before,
                "an already-visible active column must not be dragged"
            );
        },
    );
}

#[test]
fn a_resize_during_a_column_entry_does_not_strand_its_animation() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::a_resize_during_a_column_entry_does_not_strand_its_animation",
        || {
            PreferenceManager::seed_saved_preferences_for_test();
            let root = tempfile::tempdir().expect("entry fixture");
            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                crate::ui::browser::PeekBehavior::default(),
            );
            view.set_view_mode(BrowserMode::Columns);
            animations_on();
            let window = gtk::Window::builder()
                .default_width(900)
                .default_height(650)
                .child(&view.widget())
                .build();
            window.present();
            view.browser().navigate(Location::local(root.path()));
            let shell = view
                .state
                .columns
                .borrow()
                .first()
                .map(|column| column.shell.clone())
                .expect("column shell");
            let pane = column_pane(&shell);
            let scroller = view.state.scroller.clone();
            let edge = shell_edge(&scroller, &shell);
            assert!(pane.has_css_class("column-entering"));
            let gesture = resize_gesture(&scroller);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            gesture.emit_by_name::<()>("drag-end", &[&0.0f64, &0.0f64]);
            pump_until(
                || !pane.has_css_class("column-entering"),
                "the entry animation to end despite the resize",
            );
            window.destroy();
            view.browser().clear_observer();
        },
    );
}

#[test]
fn a_closing_column_leaves_after_its_animation_and_takes_no_input() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::a_closing_column_leaves_after_its_animation_and_takes_no_input",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let exiting = fixture.shell(1);
            fixture.view.browser().close_column(1);
            assert!(
                exiting.parent().is_some(),
                "the column stays while its exit animation plays"
            );
            let scroller = fixture.scroller();
            let bounds = exiting.compute_bounds(&scroller).expect("exiting bounds");
            let left = bounds.x().max(0.0);
            let right = (bounds.x() + bounds.width()).min(scroller.width() as f32);
            assert!(
                right - left > 2.0,
                "part of the closing column is on screen"
            );
            let picked = scroller.pick(
                f64::from((left + right) / 2.0),
                f64::from(bounds.y() + bounds.height() / 2.0),
                gtk::PickFlags::DEFAULT,
            );
            assert!(
                picked.is_none_or(|picked| !picked.is_ancestor(&exiting) && picked != exiting),
                "a closing column takes no clicks or drops meant for the column now at its depth"
            );
            pump_until(
                || exiting.parent().is_none(),
                "the column to leave once its animation ends",
            );
        },
    );
}

#[test]
fn switching_to_a_sibling_closes_the_old_child_without_an_exit_animation() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::switching_to_a_sibling_closes_the_old_child_without_an_exit_animation",
        || {
            let fixture = Columns::new(3, 900);
            animations_on();
            let old_child = fixture.shell(1);
            let old_grandchild = fixture.shell(2);
            let sibling = fixture._root.path().join("sibling-0");
            std::fs::create_dir_all(&sibling).expect("sibling directory");
            fixture.view.browser().descend(0, Location::local(&sibling));
            pump_until(
                || {
                    fixture.view.browser().location_at(1) == Some(Location::local(&sibling))
                        && column_loaded(&fixture.view, 1)
                },
                "sibling column load",
            );
            assert!(
                old_child.parent().is_none(),
                "the replaced child goes at once so its replacement takes its place"
            );
            assert!(
                old_grandchild.parent().is_some(),
                "deeper columns of the replaced branch shrink away so the strip slides"
            );
            pump_until(
                || old_grandchild.parent().is_none(),
                "the deeper column's exit animation to finish",
            );
        },
    );
}

#[test]
fn close_column_skips_exit_animation_when_animations_disabled() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::close_column_skips_exit_animation_when_animations_disabled",
        || {
            let fixture = Columns::new(3, 900);
            animations_off();
            let exiting = fixture.shell(1);
            fixture.view.browser().close_column(1);
            assert!(
                exiting.parent().is_none(),
                "with animations disabled the column is removed immediately"
            );
        },
    );
}

#[test]
fn a_finished_edge_drag_resizes_every_open_column() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::a_finished_edge_drag_resizes_every_open_column",
        || {
            let fixture = Columns::new(3, 1400);
            animations_off();
            let dragged = fixture.shell(1);
            let others = [fixture.shell(0), fixture.shell(2)];
            let before: Vec<_> = others.iter().map(gtk::Box::width_request).collect();
            let gesture = resize_gesture(&fixture.scroller());
            let edge = shell_edge(&fixture.scroller(), &dragged);
            gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
            gesture.emit_by_name::<()>("drag-update", &[&80.0f64, &0.0f64]);
            assert_eq!(
                others
                    .iter()
                    .map(gtk::Box::width_request)
                    .collect::<Vec<_>>(),
                before,
                "only the dragged column follows the pointer"
            );
            gesture.emit_by_name::<()>("drag-end", &[&80.0f64, &0.0f64]);
            for column in &others {
                assert_eq!(
                    column.width_request(),
                    dragged.width_request(),
                    "the other open columns take the dragged width once the drag ends"
                );
            }
        },
    );
}

#[test]
fn double_clicking_an_edge_autofits_the_column_and_saves_its_width() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::double_clicking_an_edge_autofits_the_column_and_saves_its_width",
        || {
            for animated in [true, false] {
                let fixture = Columns::new(1, 900);
                if animated {
                    animations_on();
                } else {
                    animations_off();
                }
                let shell = fixture.shell(0);
                let before = shell.width_request();
                let gesture = resize_gesture(&fixture.scroller());
                let edge = shell_edge(&fixture.scroller(), &shell);
                gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
                gesture.emit_by_name::<()>("drag-begin", &[&edge.0, &edge.1]);
                if animated {
                    pump_for(COLUMN_TRANSITION + Duration::from_millis(60));
                }
                let fitted = shell.width_request();
                assert_ne!(fitted, before, "the column fits its content");
                let scale = PreferenceManager::shared().interface_scale();
                assert_eq!(
                    PreferenceManager::shared().browser_column_width(),
                    Some(((f64::from(fitted) / scale).round() as i32).max(COLUMN_WIDTH)),
                    "the saved width is the one the column ends at"
                );
            }
        },
    );
}

#[test]
fn reveal_shows_a_fitting_column_whole_and_leaves_a_filling_one_alone() {
    let cases = [
        ((3210.0, 3510.0), 3212.0, 300.0, 3210.0),
        ((1000.0, 1702.0), 1001.0, 700.0, 1001.0),
        ((1000.0, 1702.0), 900.0, 700.0, 1000.0),
    ];
    for ((left, right), current, page_size, expected) in cases {
        let span = ColumnSpan {
            left,
            right,
            trailing: 0.0,
        };
        assert_eq!(
            span.reveal_target(current, page_size, 0.0, 4000.0),
            expected,
            "{left}..{right} seen from {current} in {page_size}"
        );
    }
}

fn hit_names(view: &BrowserView) -> std::collections::BTreeSet<String> {
    if view.view_mode() == BrowserMode::Columns {
        return view
            .state
            .columns
            .borrow()
            .first()
            .map(|column| {
                column
                    .search_results
                    .borrow()
                    .iter()
                    .map(|item| item.name.clone())
                    .collect()
            })
            .unwrap_or_default();
    }
    view.state
        .mode_views
        .borrow()
        .pane_searches()
        .iter()
        .filter_map(|search| search.results())
        .flatten()
        .map(|item| item.name)
        .collect()
}

fn search_view(mode: BrowserMode, folder: &std::path::Path) -> (BrowserView, gtk::Window) {
    let view = BrowserView::new(
        Rc::new(crate::adapters::LocalFileSource),
        crate::ui::browser::PeekBehavior::default(),
    );
    view.set_view_mode(mode);
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(&view.widget())
        .build();
    window.present();
    view.browser().navigate(Location::local(folder));
    pump_until(|| column_loaded(&view, 0), "folder load");
    (view, window)
}

fn close_search_view(view: &BrowserView, window: &gtk::Window) {
    PreferenceManager::shared().release_bindings_within(&view.widget());
    view.browser().clear_observer();
    window.destroy();
}

#[test]
fn search_hits_leave_only_once_a_worker_proves_them_gone() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::search_hits_leave_only_once_a_worker_proves_them_gone",
        || {
            use crate::adapters::{FsOp, FsQuirks, Quirks};
            PreferenceManager::shared().set_filter_include_subfolders(true);
            for mode in [BrowserMode::Columns, BrowserMode::List] {
                let fixture = tempfile::tempdir().expect("search fixture");
                let deeper = fixture.path().join("nested/deeper");
                std::fs::create_dir_all(&deeper).expect("nested folders");
                let names = ["needle-kept.txt", "needle-gone.txt", "needle-flaky.txt"];
                for name in names {
                    std::fs::write(deeper.join(name), b"hit").expect("search hit");
                }
                let (view, window) = search_view(mode, fixture.path());
                assert!(view.show_filter_with_query("needle"));
                let all: std::collections::BTreeSet<String> =
                    names.iter().map(|name| (*name).to_owned()).collect();
                pump_until(|| hit_names(&view) == all, "every hit");

                for name in ["needle-gone.txt", "needle-flaky.txt"] {
                    std::fs::remove_file(deeper.join(name)).expect("remove a hit");
                }
                let gate = crate::test_support::Gate::default();
                let scope = FsQuirks::install(
                    fixture.path(),
                    Quirks {
                        fail: vec![(FsOp::Stat, "needle-flaky.txt", 0, rustix::io::Errno::IO)],
                        before: vec![(FsOp::Stat, "needle-kept.txt", 0, gate.racer())],
                        ..Quirks::default()
                    },
                );
                view.state.prune_stale_search_results();
                pump_until(|| gate.reached(), "the presence check reaches a worker");
                let ticked = Rc::new(Cell::new(false));
                let tick = ticked.clone();
                glib::timeout_add_local_once(Duration::from_millis(5), move || tick.set(true));
                pump_until(|| ticked.get(), "the window keeps running");
                assert_eq!(hit_names(&view), all, "{mode:?}: hits stay while parked");

                gate.release();
                let kept: std::collections::BTreeSet<String> =
                    ["needle-kept.txt", "needle-flaky.txt"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                pump_until(|| hit_names(&view) == kept, "the gone hit leaves");
                pump_for(Duration::from_millis(50));
                assert_eq!(
                    hit_names(&view),
                    kept,
                    "{mode:?}: only a proven absence prunes"
                );
                drop(scope);
                close_search_view(&view, &window);
            }
        },
    );
}

#[test]
fn search_asks_past_hits_proven_gone_until_present_ones_fill_the_view() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::search_asks_past_hits_proven_gone_until_present_ones_fill_the_view",
        || {
            let limit = crate::services::SEARCH_RESULT_LIMIT;
            PreferenceManager::shared().set_filter_include_subfolders(true);
            for mode in [BrowserMode::Columns, BrowserMode::List] {
                let fixture = tempfile::tempdir().expect("search fixture");
                let deeper = fixture.path().join("nested/deeper");
                std::fs::create_dir_all(&deeper).expect("nested folders");
                let gone: Vec<_> = (0..limit * 3 / 2)
                    .map(|index| deeper.join(format!("needle-{index:03}")))
                    .collect();
                for path in &gone {
                    std::fs::write(path, b"hit").expect("top-ranked hit");
                }
                for index in 0..limit * 2 {
                    std::fs::write(deeper.join(format!("needle-{index:03}-kept.txt")), b"hit")
                        .expect("lower-ranked hit");
                }
                let (view, window) = search_view(mode, fixture.path());
                assert!(view.show_filter_with_query("needle"));
                pump_until(
                    || {
                        let names = hit_names(&view);
                        names.len() == limit && names.iter().all(|name| !name.ends_with(".txt"))
                    },
                    "the top-ranked hits",
                );

                for path in &gone {
                    std::fs::remove_file(path).expect("remove a hit");
                }
                assert!(view.show_filter_with_query("needl"));
                pump_until(
                    || {
                        let names = hit_names(&view);
                        names.len() == limit && names.iter().all(|name| name.ends_with("-kept.txt"))
                    },
                    "present hits fill the view",
                );
                close_search_view(&view, &window);
            }
        },
    );
}

fn widget_text(widget: &gtk::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        return Some(label.text().to_string());
    }
    widget
        .downcast_ref::<gtk::Inscription>()
        .and_then(|label| label.text())
        .as_deref()
        .map(ToString::to_string)
}

fn find_mapped_text(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if widget.is_mapped() && widget_text(widget).as_deref() == Some(name) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_mapped_text(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn primary_click(widget: &gtk::Widget) -> Option<gtk::GestureClick> {
    let mut current = Some(widget.clone());
    while let Some(widget) = current {
        let controllers = widget.observe_controllers();
        for index in 0..controllers.n_items() {
            if let Some(click) = controllers
                .item(index)
                .and_then(|controller| controller.downcast::<gtk::GestureClick>().ok())
                && click.button() == 1
                && click.propagation_phase() == gtk::PropagationPhase::Capture
            {
                return Some(click);
            }
        }
        current = widget.parent();
    }
    None
}

fn click_named(root: &gtk::Widget, name: &str, presses: i32) -> bool {
    let Some(label) = find_mapped_text(root, name) else {
        return false;
    };
    let Some(click) = primary_click(&label) else {
        return false;
    };
    click.emit_by_name::<()>("pressed", &[&presses, &8.0f64, &8.0f64]);
    click.emit_by_name::<()>("released", &[&presses, &8.0f64, &8.0f64]);
    true
}

fn activate_named_list(root: &gtk::Widget, name: &str) -> bool {
    let Some(label) = find_mapped_text(root, name) else {
        return false;
    };
    let mut current = label.parent();
    while let Some(widget) = current {
        if let Some(list) = widget.downcast_ref::<gtk::ListView>() {
            let Some(selection) = list.model() else {
                return false;
            };
            let Some(position) =
                (0..selection.n_items()).find(|position| selection.is_selected(*position))
            else {
                return false;
            };
            list.emit_by_name::<()>("activate", &[&position]);
            return true;
        }
        current = widget.parent();
    }
    false
}

struct ClickPreferenceRestore {
    previews: bool,
    subfolders: bool,
    clicks: [(BrowserMode, crate::ui::browser_modes::ClickActivation); 3],
}

impl Drop for ClickPreferenceRestore {
    fn drop(&mut self) {
        let preferences = PreferenceManager::shared();
        preferences.set_single_click_previews(self.previews);
        preferences.set_filter_include_subfolders(self.subfolders);
        for (mode, activation) in self.clicks {
            preferences.set_click_activation(mode, activation);
        }
    }
}

fn preview_names(events: &[crate::app::BrowserEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            crate::app::BrowserEvent::PreviewRequested { entry, .. } => {
                Some(entry.display_name.clone())
            }
            _ => None,
        })
        .collect()
}

fn opened_names(events: &[crate::app::BrowserEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            crate::app::BrowserEvent::OpenRequested { location } => {
                location.native_path().and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
            }
            _ => None,
        })
        .collect()
}

#[test]
fn filtered_file_click_previews_and_double_click_opens() {
    crate::test_support::gtk_test(
        "ui::browser::columns::tests::filtered_file_click_previews_and_double_click_opens",
        || {
            use crate::ui::browser_modes::{ClickActivation, ClickCount};

            let preferences = PreferenceManager::shared();
            let previous_previews = preferences.single_click_previews();
            let previous_subfolders = preferences.filter_include_subfolders();
            let previous_clicks = [BrowserMode::Columns, BrowserMode::Icons, BrowserMode::List]
                .map(|mode| (mode, preferences.click_activation(mode)));
            preferences.set_single_click_previews(true);
            preferences.set_filter_include_subfolders(false);
            for mode in [BrowserMode::Columns, BrowserMode::Icons, BrowserMode::List] {
                preferences.set_click_activation(
                    mode,
                    ClickActivation {
                        files: ClickCount::Two,
                        folders: ClickCount::Two,
                    },
                );
            }
            let _restore = ClickPreferenceRestore {
                previews: previous_previews,
                subfolders: previous_subfolders,
                clicks: previous_clicks,
            };
            for mode in [BrowserMode::List, BrowserMode::Icons, BrowserMode::Columns] {
                let root = tempfile::tempdir().expect("filter fixture");
                std::fs::write(root.path().join("preview-note.txt"), "preview me\n").expect("file");
                std::fs::create_dir(root.path().join("docs-folder")).expect("folder");
                std::fs::write(root.path().join("other.txt"), "other\n").expect("decoy");
                let (view, window) = search_view(mode, root.path());
                let events = Rc::new(RefCell::new(Vec::new()));
                let recorded = events.clone();
                view.browser().observe(move |event| match event {
                    crate::app::BrowserEvent::PreviewRequested { .. }
                    | crate::app::BrowserEvent::OpenRequested { .. } => {
                        recorded.borrow_mut().push(event.clone());
                    }
                    _ => {}
                });

                assert!(view.show_filter_with_query("preview-note"));
                let file_hits = ["preview-note.txt".to_owned()]
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                pump_until(|| hit_names(&view) == file_hits, "the filtered file");
                pump_until(
                    || {
                        find_mapped_text(&view.widget(), "preview-note.txt")
                            .is_some_and(|label| primary_click(&label).is_some())
                    },
                    "the filtered file row",
                );
                assert!(
                    click_named(&view.widget(), "preview-note.txt", 1),
                    "{mode:?} single click"
                );
                pump_until(
                    || {
                        view.selected_search_results().is_some_and(|entries| {
                            entries.len() == 1 && entries[0].display_name == "preview-note.txt"
                        })
                    },
                    "the filtered file to be selected",
                );
                assert_eq!(
                    preview_names(&events.borrow()),
                    ["preview-note.txt"],
                    "{mode:?} single click previews"
                );
                assert!(
                    opened_names(&events.borrow()).is_empty(),
                    "{mode:?} single click opened {:?}",
                    opened_names(&events.borrow())
                );

                events.borrow_mut().clear();
                let opened = if mode == BrowserMode::Columns {
                    activate_named_list(&view.widget(), "preview-note.txt")
                } else {
                    click_named(&view.widget(), "preview-note.txt", 2)
                };
                assert!(opened, "{mode:?} double click");
                assert_eq!(
                    opened_names(&events.borrow()),
                    ["preview-note.txt"],
                    "{mode:?} double click opens once"
                );
                assert!(
                    preview_names(&events.borrow()).is_empty(),
                    "{mode:?} double click previewed again"
                );

                events.borrow_mut().clear();
                assert!(view.show_filter_with_query("docs-folder"));
                let folder_hits = ["docs-folder".to_owned()]
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                pump_until(|| hit_names(&view) == folder_hits, "the filtered folder");
                pump_until(
                    || find_mapped_text(&view.widget(), "docs-folder").is_some(),
                    "the filtered folder row",
                );
                assert!(
                    click_named(&view.widget(), "docs-folder", 1),
                    "{mode:?} folder click"
                );
                pump_until(
                    || {
                        view.browser().active_location().is_some_and(|location| {
                            location
                                .native_path()
                                .is_some_and(|path| path.ends_with("docs-folder"))
                        })
                    },
                    "the filtered folder to open",
                );
                assert!(
                    opened_names(&events.borrow()).is_empty(),
                    "{mode:?} folder click launched a file"
                );
                close_search_view(&view, &window);
            }
        },
    );
}
