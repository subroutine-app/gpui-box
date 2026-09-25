//! Navigation feedback must not move the destination or its label. Measure
//! both: the semantic wrapper alone does not expose a child's press inset.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Bounds, Modifiers, MouseButton, Pixels, SharedString, TestAppContext, div, prelude::*, px,
};
use gpui_kit::prelude::*;
use gpui_kit::theme::Space;
use gpui_kit_testkit::harness::Harness;

#[derive(Clone, Copy, Debug)]
enum Navigation {
    Sidebar,
    Tabs,
    Capsules,
}

const DESTINATIONS: [(&str, &str); 3] = [
    ("general", "General"),
    ("appearance", "Appearance"),
    ("shortcuts", "Keyboard shortcuts"),
];
const IDS: [&str; 3] = ["nav.general", "nav.appearance", "nav.shortcuts"];
const LABELS: [&str; 3] = [
    "nav.general.label",
    "nav.appearance.label",
    "nav.shortcuts.label",
];
type Selection = Rc<RefCell<SharedString>>;
type Calls = Rc<RefCell<Vec<SharedString>>>;

fn navigation(
    cx: &mut TestAppContext,
    kind: Navigation,
    size: ControlSize,
    reduced: bool,
) -> (Harness, Selection, Calls) {
    let selected = Rc::new(RefCell::new(SharedString::from("general")));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let current = selected.clone();
    let reports = calls.clone();
    let harness = Harness::new(
        cx,
        move |cx| {
            gpui_kit::install(cx);
            // The default reduced-motion visual lane would hide the press bug.
            cx.set_reduce_motion(reduced);
        },
        move |_, _| {
            let selected = current.borrow().clone();
            let reports = reports.clone();
            let on_select = move |id, _: &mut gpui::Window, _: &mut gpui::App| {
                reports.borrow_mut().push(id);
            };
            let navigation = match kind {
                Navigation::Sidebar => Sidebar::new("nav")
                    .section(
                        SidebarSection::new("categories")
                            .items(DESTINATIONS.map(|(id, label)| SidebarItem::new(id, label))),
                    )
                    .active(selected)
                    .control_size(size)
                    .on_select(on_select)
                    .into_any_element(),
                Navigation::Tabs | Navigation::Capsules => Tabs::new("nav")
                    .when(matches!(kind, Navigation::Capsules), Tabs::capsules)
                    .tabs(DESTINATIONS.map(|(id, label)| TabItem::new(id, label)))
                    .selected(selected)
                    .control_size(size)
                    .on_select(on_select)
                    .into_any_element(),
            };
            div()
                .w(px(700.0))
                .h(px(300.0))
                .p(px(20.0))
                .child(navigation)
                .into_any_element()
        },
    );
    (harness, selected, calls)
}

#[derive(Debug, PartialEq)]
struct Geometry {
    controls: [Bounds<Pixels>; 3],
    labels: [Bounds<Pixels>; 3],
    container: Bounds<Pixels>,
}

fn geometry(harness: &mut Harness) -> Geometry {
    let controls = IDS.map(|id| harness.bounds(id).expect("destination bounds"));
    let container = harness.bounds("nav").expect("navigation bounds");
    let labels = LABELS.map(|id| harness.context().debug_bounds(id).expect("label bounds"));
    Geometry {
        controls,
        labels,
        container,
    }
}

fn assert_geometry(harness: &mut Harness, expected: &Geometry, phase: &str) {
    let actual = geometry(harness);
    // Check labels first: a paint-time inset can leave the outer measurement
    // unchanged. Also compare full bounds to catch selection-induced resizing.
    assert_eq!(actual.labels, expected.labels, "label geometry: {phase}");
    assert_eq!(
        actual.controls, expected.controls,
        "control geometry: {phase}"
    );
    assert_eq!(
        actual.container, expected.container,
        "container geometry: {phase}"
    );
}

fn press_and_release(cx: &mut TestAppContext, kind: Navigation) {
    for size in [ControlSize::Sm, ControlSize::Md] {
        for reduced in [false, true] {
            let (mut harness, _, calls) = navigation(cx, kind, size, reduced);
            let resting = geometry(&mut harness);
            // Both the already-current destination and an inactive one.
            for id in [IDS[0], IDS[1]] {
                let at = harness.point_in(id);
                harness
                    .context()
                    .simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
                assert!(calls.borrow().is_empty(), "press alone is not selection");
                assert_geometry(&mut harness, &resting, "press");
                harness.advance(Duration::from_millis(80));
                assert_geometry(&mut harness, &resting, "held");

                harness
                    .context()
                    .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
                assert_geometry(&mut harness, &resting, "release before host acceptance");
                assert_eq!(calls.borrow().len(), 1, "release reports exactly once");
                calls.borrow_mut().clear();
                assert!(harness.node(id).expect("destination").focused);
                harness.advance(Duration::from_millis(300));
                assert_geometry(&mut harness, &resting, "settled release");
            }
            match kind {
                // The sidebar has one tab stop; arrows move within its roving order.
                Navigation::Sidebar => harness.keystrokes("down"),
                Navigation::Tabs | Navigation::Capsules => {
                    harness.update(|window, cx| window.focus_next(cx));
                }
            }
            assert!(calls.borrow().is_empty(), "focus alone is not selection");
            assert!(harness.node(IDS[2]).expect("keyboard destination").focused);
            harness.update(|window, _| assert!(window.focus_is_visible()));
            assert_geometry(&mut harness, &resting, "keyboard focus ring");
            harness.keystrokes("enter");
            assert_eq!(calls.borrow_mut().pop().as_deref(), Some("shortcuts"));
            assert_geometry(&mut harness, &resting, "keyboard activation");
        }
    }
}

fn switch_selection(cx: &mut TestAppContext, kind: Navigation) {
    for size in [ControlSize::Sm, ControlSize::Md] {
        for reduced in [false, true] {
            let (mut harness, selected, calls) = navigation(cx, kind, size, reduced);
            let resting = geometry(&mut harness);
            for (id, key) in [
                (IDS[1], "appearance"),
                (IDS[2], "shortcuts"),
                (IDS[0], "general"),
            ] {
                harness.click(id);
                assert_eq!(calls.borrow_mut().pop().as_deref(), Some(key));
                // Accept the reported intent as a host would; only now should
                // the current place change, with no reflow of any destination.
                harness.update(|_, cx| {
                    *selected.borrow_mut() = SharedString::from(key);
                    cx.refresh_windows();
                });
                assert_geometry(&mut harness, &resting, "host accepted selection");
                let node = harness.node(id).expect("current destination");
                match kind {
                    Navigation::Sidebar => assert!(node.selected),
                    Navigation::Tabs | Navigation::Capsules => assert_eq!(node.checked, Some(true)),
                }
                harness.advance(Duration::from_millis(80));
                assert_geometry(&mut harness, &resting, "selection in flight");
                harness.advance(Duration::from_millis(300));
                assert_geometry(&mut harness, &resting, "settled selection");
            }
        }
    }
}

#[gpui::test]
fn sidebar_press_and_release_keep_bounds_and_label_origins(cx: &mut TestAppContext) {
    press_and_release(cx, Navigation::Sidebar);
}

#[gpui::test]
fn tabs_press_and_release_keep_bounds_and_label_origins(cx: &mut TestAppContext) {
    press_and_release(cx, Navigation::Tabs);
}

#[gpui::test]
fn capsules_press_and_release_keep_bounds_and_label_origins(cx: &mut TestAppContext) {
    press_and_release(cx, Navigation::Capsules);
}

#[gpui::test]
fn sidebar_active_switch_keeps_bounds_and_label_origins(cx: &mut TestAppContext) {
    switch_selection(cx, Navigation::Sidebar);
}

#[gpui::test]
fn tabs_active_switch_keeps_bounds_and_label_origins(cx: &mut TestAppContext) {
    switch_selection(cx, Navigation::Tabs);
}

#[gpui::test]
fn capsules_active_switch_keeps_bounds_and_label_origins(cx: &mut TestAppContext) {
    switch_selection(cx, Navigation::Capsules);
}

#[gpui::test]
fn ide_shell_tab_borders_do_not_add_to_horizontal_padding(cx: &mut TestAppContext) {
    let scene = gpui_kit::scenes::find("ide-shell").expect("IDE shell exhibit");
    let mut harness = Harness::new(cx, gpui_kit::install, scene.build);
    for theme in ["studio-dark", "studio-light"] {
        harness.update(|_, cx| {
            assert!(gpui_kit::theme::activate_theme(theme, cx));
        });
        let metrics = harness.update(|_, cx| cx.theme().control.get(ControlSize::Sm));
        for (id, label) in [
            (
                "scene.shell.left.tabs.files",
                "scene.shell.left.tabs.files.label",
            ),
            (
                "scene.shell.left.tabs.search",
                "scene.shell.left.tabs.search.label",
            ),
        ] {
            let tab = harness.bounds(id).expect("dock tab");
            let label = harness.context().debug_bounds(label).expect("dock label");
            assert_eq!(
                label.left() - tab.left(),
                px(metrics.padding_x + metrics.icon_size + metrics.gap),
                "border belongs inside the padding budget: {id} in {theme}"
            );
        }
    }
}

#[gpui::test]
fn narrow_tabs_fit_the_content_and_padding_budget_without_pushing_the_body(
    cx: &mut TestAppContext,
) {
    let width = Rc::new(Cell::new(400.0));
    let selected = Rc::new(Cell::new("files"));
    let frame_width = width.clone();
    let current = selected.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
        div()
            .column()
            .w(px(frame_width.get()))
            .child(
                Tabs::new("narrow")
                    .small()
                    .tabs([
                        TabItem::new("files", "Files"),
                        TabItem::new("search", "Search"),
                    ])
                    .selected(current.get())
                    .on_select(|_, _, _| {}),
            )
            .child(div().h(px(100.0)).debug_selector(|| "workspace".into()))
            .into_any_element()
    });
    for theme in ["studio-dark", "studio-light"] {
        harness.update(|_, cx| {
            width.set(400.0);
            selected.set("files");
            assert!(gpui_kit::theme::activate_theme(theme, cx));
            cx.refresh_windows();
        });
        harness.frame();
        let label_width: f32 = ["narrow.files.label", "narrow.search.label"]
            .into_iter()
            .map(|id| {
                f32::from(
                    harness
                        .context()
                        .debug_bounds(id)
                        .expect("label")
                        .size
                        .width,
                )
            })
            .sum();
        let budget = harness.update(|_, cx| {
            let theme = cx.theme();
            // Two labels, two sides per tab, one gap, and the strip's own
            // border + padding. Per-tab borders must not add another width.
            label_width
                + 4.0 * theme.control.get(ControlSize::Sm).padding_x
                + theme.space(Space::Xs)
                + 6.0 * theme.borders.hairline
        });
        let height = harness.bounds("narrow").expect("wide strip").size.height;
        harness.update(|_, cx| {
            width.set(budget);
            cx.refresh_windows();
        });
        let mut geometry = None;
        for active in ["files", "search", "files"] {
            harness.update(|_, cx| {
                selected.set(active);
                cx.refresh_windows();
            });
            let files = harness.bounds("narrow.files").expect("Files");
            let search = harness.bounds("narrow.search").expect("Search");
            let strip = harness.bounds("narrow").expect("narrow strip");
            let body = harness.context().debug_bounds("workspace").expect("body");
            assert_eq!(files.top(), search.top(), "one row in {theme}");
            assert!(search.right() <= strip.right(), "both tabs fit");
            assert_eq!(strip.size.height, height, "no extra header row");
            assert_eq!(body.top(), strip.bottom(), "body follows the single row");
            let current = [files, search, strip, body];
            if let Some(expected) = geometry {
                assert_eq!(
                    current, expected,
                    "selection keeps the header and body fixed"
                );
            }
            geometry = Some(current);
        }
    }
}
