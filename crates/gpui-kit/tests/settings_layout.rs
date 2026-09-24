//! Settings keep the caller's control and intent, not a second row-level action.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{Bounds, Entity, Modifiers, Pixels, TestAppContext, div, prelude::*, px};
use gpui_kit::foundation::{LayoutDirection, set_layout_direction};
use gpui_kit::prelude::*;
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_testkit::harness::Harness;
use gpui_kit_theme::{Space, TypeScale};

const LABEL: &str = "Automatically preserve workspace changes across application restarts";
const DESCRIPTION: &str = "Keep the last verified workspace available when a refresh cannot complete. This description must wrap without crowding the setting name or pushing the action outside its row.";

type Calls<T> = Rc<RefCell<Vec<T>>>;

fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) {
    let tolerance = px(0.1);
    assert!(
        inner.left() >= outer.left() - tolerance,
        "{inner:?} outside {outer:?}"
    );
    assert!(
        inner.right() <= outer.right() + tolerance,
        "{inner:?} outside {outer:?}"
    );
    assert!(
        inner.top() >= outer.top() - tolerance,
        "{inner:?} outside {outer:?}"
    );
    assert!(
        inner.bottom() <= outer.bottom() + tolerance,
        "{inner:?} outside {outer:?}"
    );
}

#[gpui::test]
fn wrapping_names_leave_intrinsic_switches_and_long_actions_room(cx: &mut TestAppContext) {
    for width in [420.0, 720.0] {
        for direction in [LayoutDirection::LeftToRight, LayoutDirection::RightToLeft] {
            let mut harness = Harness::new(
                cx,
                move |cx| {
                    gpui_kit::install(cx);
                    set_layout_direction(direction, cx);
                },
                move |_, _| {
                    div()
                        .w(px(width))
                        .child(
                            SettingsSection::new("settings", "General")
                                .row(
                                    SettingsRow::new("save", LABEL)
                                        .description(DESCRIPTION)
                                        .switch(Switch::new("save.switch").on_change(|_, _, _| {})),
                                )
                                .row(
                                    SettingsRow::new("reset", "Workspace storage")
                                        .description(DESCRIPTION)
                                        .control(
                                            Button::new("reset.action")
                                                .label("Restore workspace preferences to defaults")
                                                .on_click(|_, _| {}),
                                        ),
                                ),
                        )
                        .into_any_element()
                },
            );
            for id in ["save", "reset"] {
                let row = harness.bounds(id).expect("fixture is rendered");
                let names = harness
                    .bounds(&format!("{id}.names"))
                    .expect("fixture is rendered");
                let label = harness
                    .bounds(&format!("{id}.label"))
                    .expect("fixture is rendered");
                let description = harness
                    .bounds(&format!("{id}.description"))
                    .expect("fixture is rendered");
                let field = harness
                    .bounds(&format!("{id}.field"))
                    .expect("fixture is rendered");
                inside(names, row);
                inside(label, names);
                inside(description, names);
                inside(field, row);
                assert!(description.top() > label.bottom());
                assert!(
                    field.top() >= names.bottom()
                        || field.right() <= names.left()
                        || field.left() >= names.right()
                );
            }
            let switch = harness.bounds("save.switch").expect("fixture is rendered");
            let field = harness.bounds("save.field").expect("fixture is rendered");
            assert_eq!(
                field.size.width, switch.size.width,
                "no generic field width for switches"
            );
            let action = harness.bounds("reset.action").expect("fixture is rendered");
            inside(
                action,
                harness.bounds("reset.field").expect("fixture is rendered"),
            );
            assert!(
                action.size.width > px(180.0),
                "long action must not be squeezed into the old fixed column"
            );
            let trailing = if direction.is_ltr() {
                field.right()
            } else {
                field.left()
            };
            let action_trailing = if direction.is_ltr() {
                action.right()
            } else {
                action.left()
            };
            assert_eq!(trailing, action_trailing);
        }
    }
}

#[gpui::test]
fn explicit_name_minimum_wraps_controls_instead_of_shrinking_names(cx: &mut TestAppContext) {
    for width in [420.0, 720.0] {
        let mut harness = Harness::new(cx, gpui_kit::install, move |_, cx| {
            div()
                .w(px(width))
                .child(
                    SettingsSection::new("settings", "General")
                        .label_width(px(240.0))
                        .row(
                            SettingsRow::new("inherited", LABEL)
                                .description(DESCRIPTION)
                                .control(
                                    div()
                                        .w_full()
                                        .h(px(28.0))
                                        .semantic_in(cx, NodeSpec::new("input", Role::Input)),
                                ),
                        )
                        .row(
                            SettingsRow::new("override", LABEL)
                                .label_width(px(300.0))
                                .control_width(px(220.0))
                                .control(div().w_full().h(px(28.0))),
                        ),
                )
                .into_any_element()
        });
        for (id, minimum) in [("inherited", 240.0), ("override", 300.0)] {
            let row = harness.bounds(id).expect("fixture is rendered");
            let names = harness
                .bounds(&format!("{id}.names"))
                .expect("fixture is rendered");
            let field = harness
                .bounds(&format!("{id}.field"))
                .expect("fixture is rendered");
            assert!(names.size.width >= px(minimum));
            inside(names, row);
            inside(field, row);
            if width == 420.0 {
                assert!(
                    field.top() > names.bottom(),
                    "narrow row stacks instead of clipping"
                );
            } else {
                assert!(field.left() > names.right(), "wide row keeps two columns");
            }
        }
        assert_eq!(
            harness
                .bounds("override.field")
                .expect("fixture is rendered")
                .size
                .width,
            px(220.0)
        );
        assert!(
            harness
                .bounds("input")
                .expect("fixture is rendered")
                .size
                .width
                >= px(180.0)
        );
    }
}

#[gpui::test]
fn stacked_composite_uses_available_width_and_readable_type(cx: &mut TestAppContext) {
    for width in [240.0, 420.0, 720.0] {
        let mut harness = Harness::new(cx, gpui_kit::install, move |_, cx| {
            div()
                .w(px(width))
                .child(
                    SettingsRow::new("composite", LABEL)
                        .description(DESCRIPTION)
                        .stacked()
                        .control(
                            div()
                                .flex()
                                .flex_wrap()
                                .w_full()
                                .gap(px(12.0))
                                .child(
                                    Button::new("first")
                                        .label("Restore preferences")
                                        .on_click(|_, _| {}),
                                )
                                .child(
                                    Button::new("second")
                                        .label("Export workspace settings")
                                        .on_click(|_, _| {}),
                                )
                                .semantic_in(cx, NodeSpec::new("actions", Role::Group)),
                        ),
                )
                .into_any_element()
        });
        let row = harness.bounds("composite").expect("fixture is rendered");
        let names = harness
            .bounds("composite.names")
            .expect("fixture is rendered");
        let field = harness
            .bounds("composite.field")
            .expect("fixture is rendered");
        let label = harness
            .bounds("composite.label")
            .expect("fixture is rendered");
        let description = harness
            .bounds("composite.description")
            .expect("fixture is rendered");
        let (body_height, padding) = harness.update(|_, cx| {
            (
                cx.theme().typography.body.line_height,
                cx.theme().space(Space::Md),
            )
        });
        assert!(label.size.height >= px(body_height));
        assert!(description.size.height >= px(body_height));
        assert!(names.top() - row.top() >= px(padding));
        assert_eq!(names.size.width, field.size.width);
        assert!(field.top() > names.bottom());
        for id in [
            "composite.names",
            "composite.field",
            "actions",
            "first",
            "second",
        ] {
            inside(harness.bounds(id).expect("fixture is rendered"), row);
        }
    }
}

#[gpui::test]
fn switch_name_description_and_control_each_report_exactly_one_intent(cx: &mut TestAppContext) {
    let calls: Calls<bool> = Rc::default();
    let sink = calls.clone();
    let on = Rc::new(Cell::new(false));
    let current = on.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
        let sink = sink.clone();
        div()
            .w(px(420.0))
            .child(
                SettingsRow::new("save", "Save automatically")
                    .description("Write changes as they happen")
                    .switch(
                        Switch::new("save.switch")
                            .on(current.get())
                            .on_change(move |next, _, _| sink.borrow_mut().push(next)),
                    ),
            )
            .into_any_element()
    });
    assert_eq!(
        harness
            .node("save.switch")
            .expect("fixture is rendered")
            .text
            .as_deref(),
        Some("Save automatically")
    );
    for id in ["save.label", "save.description", "save.switch"] {
        harness.click(id);
        assert_eq!(
            calls.borrow_mut().drain(..).collect::<Vec<_>>(),
            vec![!on.get()]
        );
        assert_eq!(
            harness
                .node("save.switch")
                .expect("fixture is rendered")
                .checked,
            Some(on.get())
        );
        harness.update(|_, cx| {
            on.set(!on.get());
            cx.refresh_windows();
        });
    }
    harness.keystrokes("space");
    assert_eq!(
        calls.borrow_mut().drain(..).collect::<Vec<_>>(),
        vec![!on.get()]
    );
}

#[gpui::test]
fn switch_label_focuses_the_control_for_space_and_root_tab_navigation(cx: &mut TestAppContext) {
    let calls: Calls<bool> = Rc::default();
    let sink = calls.clone();
    let current = Rc::new(Cell::new(false));
    let root_spaces = Rc::new(Cell::new(0));
    let root_sink = root_spaces.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
        let current = current.clone();
        let sink = sink.clone();
        let root_sink = root_sink.clone();
        div()
            .id("settings-root")
            .w(px(420.0))
            .flex()
            .flex_col()
            .on_key_down(
                move |event, window, cx| match event.keystroke.key.as_str() {
                    "space" => root_sink.set(root_sink.get() + 1),
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    }
                    _ => {}
                },
            )
            .child(Button::new("before").label("Before").on_click(|_, _| {}))
            .child(
                SettingsRow::new("save", "Save automatically")
                    .description("Write changes as they happen")
                    .switch(Switch::new("save.switch").on(current.get()).on_change(
                        move |next, _, cx| {
                            sink.borrow_mut().push(next);
                            current.set(next);
                            cx.refresh_windows();
                        },
                    )),
            )
            .child(Button::new("after").label("After").on_click(|_, _| {}))
            .into_any_element()
    });
    for id in ["save.label", "save.description"] {
        harness.click("before");
        assert!(harness.node("before").expect("prior control").focused);
        harness.click(id);
        assert_eq!(calls.borrow_mut().drain(..).collect::<Vec<_>>(), vec![true]);
        let switch = harness.node("save.switch").expect("switch mounted");
        assert!(switch.focused, "label activation focuses its actual switch");
        assert_eq!(switch.checked, Some(true));
        assert!(!harness.node("save.names").expect("names mounted").focused);
        harness.keystrokes("space");
        assert_eq!(
            calls.borrow_mut().drain(..).collect::<Vec<_>>(),
            vec![false]
        );
        assert_eq!(
            harness.node("save.switch").expect("switch mounted").checked,
            Some(false)
        );
        assert_eq!(
            root_spaces.get(),
            0,
            "the switch, not its root, consumes Space"
        );
        harness.keystrokes("tab");
        assert!(harness.node("after").expect("next control").focused);
        harness.keystrokes("shift-tab");
        assert!(harness.node("save.switch").expect("switch mounted").focused);
        harness.keystrokes("shift-tab");
        assert!(
            harness.node("before").expect("prior control").focused,
            "the label is not a second tab stop"
        );
        assert!(
            calls.borrow().is_empty(),
            "navigation does not activate the switch"
        );
    }
}

#[gpui::test]
fn withheld_disabled_and_handlerless_switches_have_no_label_action(cx: &mut TestAppContext) {
    let calls: Calls<bool> = Rc::default();
    let sink = calls.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
        let row = |id: &'static str| {
            let sink = sink.clone();
            SettingsRow::new(id, "Save automatically")
                .description("Write changes as they happen")
                .switch(
                    Switch::new(format!("{id}.switch"))
                        .disabled(id == "disabled")
                        .on_change(move |next, _, _| sink.borrow_mut().push(next)),
                )
        };
        div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .child(
                Button::new("sentinel")
                    .label("Keep focus")
                    .on_click(|_, _| {}),
            )
            .child(
                SettingsSection::new("general", "General")
                    .row(row("managed").value("Off").managed("your administrator"))
                    .row(row("disabled"))
                    .row(
                        SettingsRow::new("inert", "Display only")
                            .switch(Switch::new("inert.switch")),
                    ),
            )
            .child(
                SettingsSection::new("dimmed", "Sync")
                    .dimmed_by("Offline")
                    .row(row("offline")),
            )
            .into_any_element()
    });
    harness.click("sentinel");
    for id in ["managed", "offline", "disabled", "inert"] {
        harness.click(&format!("{id}.label"));
        assert!(harness.node("sentinel").expect("focused sentinel").focused);
        if id != "inert" {
            harness.click(&format!("{id}.description"));
            assert!(harness.node("sentinel").expect("focused sentinel").focused);
        }
    }
    harness.click("disabled.switch");
    harness.click("inert.switch");
    assert!(harness.node("sentinel").expect("focused sentinel").focused);
    assert!(calls.borrow().is_empty());
    assert!(harness.node("managed.switch").is_none());
    assert!(harness.node("offline.switch").is_none());
    assert!(
        harness
            .node("managed")
            .expect("fixture is rendered")
            .disabled
    );
    assert!(
        harness
            .node("offline")
            .expect("fixture is rendered")
            .disabled
    );
    harness.update(|window, cx| window.focus_next(cx));
    assert!(harness.node("sentinel").expect("only tab stop").focused);
    harness.keystrokes("space enter");
    assert!(calls.borrow().is_empty());
}

fn select_row(cx: &mut TestAppContext, withheld: bool) -> (Harness, Entity<Select>, Calls<String>) {
    let slot: Rc<RefCell<Option<Entity<Select>>>> = Rc::default();
    let build_slot = slot.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |window, cx| {
        let select = build_slot
            .borrow_mut()
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    Select::new("theme.select", window, cx)
                        .options([
                            SelectOption::new("system", "Follow system"),
                            SelectOption::new("dark", "Dark"),
                        ])
                        .selected("system")
                })
            })
            .clone();
        div()
            .w(px(420.0))
            .child(
                SettingsRow::new("theme", "Application theme")
                    .description("Choose the appearance of this workspace")
                    .select(select)
                    .when(withheld, |row| row.managed("your administrator")),
            )
            .into_any_element()
    });
    harness.snapshot();
    let select = slot.borrow().clone().expect("fixture is rendered");
    let calls: Calls<String> = Rc::default();
    let sink = calls.clone();
    harness.update(|_, cx| {
        cx.subscribe(&select, move |_, event: &SelectEvent, _| {
            sink.borrow_mut().push(match event {
                SelectEvent::Opened => "open".into(),
                SelectEvent::Closed => "close".into(),
                SelectEvent::Selected(id) => format!("selected:{id}"),
                SelectEvent::Cleared => "clear".into(),
            });
        })
        .detach();
    });
    (harness, select, calls)
}

#[gpui::test]
fn select_label_opens_the_exact_entity_and_keyboard_continues_from_selection(
    cx: &mut TestAppContext,
) {
    let (mut harness, select, calls) = select_row(cx, false);
    assert_eq!(
        harness
            .node("theme.select")
            .expect("fixture is rendered")
            .text
            .as_deref(),
        Some("Application theme")
    );
    harness.click("theme.label");
    assert_eq!(*calls.borrow(), vec!["open"]);
    assert!(
        harness
            .node("theme.select")
            .expect("fixture is rendered")
            .focused
    );
    assert_eq!(
        harness
            .node("theme.select")
            .expect("fixture is rendered")
            .expanded,
        Some(true)
    );
    harness.update(|window, cx| select.update(cx, |select, cx| select.open(window, cx)));
    assert_eq!(*calls.borrow(), vec!["open"], "opening is idempotent");
    harness.keystrokes("down enter");
    assert_eq!(*calls.borrow(), vec!["open", "selected:dark", "close"]);
    assert_eq!(
        harness
            .node("theme.select")
            .expect("fixture is rendered")
            .value
            .as_deref(),
        Some("Follow system")
    );
    calls.borrow_mut().clear();
    harness.click("theme.description");
    assert_eq!(*calls.borrow(), vec!["open"]);
    harness.keystrokes("escape");
    calls.borrow_mut().clear();
    harness.click("theme.select");
    assert_eq!(
        *calls.borrow(),
        vec!["open"],
        "direct control opens once, not a row toggle"
    );
    harness.keystrokes("escape enter");
    assert_eq!(*calls.borrow(), vec!["open", "close", "open"]);
}

#[gpui::test]
fn disabled_and_managed_select_labels_never_open_a_picker(cx: &mut TestAppContext) {
    let (mut harness, select, calls) = select_row(cx, false);
    harness.update(|_, cx| select.update(cx, |select, cx| select.set_disabled(true, cx)));
    harness.click("theme.label");
    harness.click("theme.description");
    harness.click("theme.select");
    harness.update(|window, cx| select.update(cx, |select, cx| select.open(window, cx)));
    assert_eq!(
        harness
            .node("theme.select")
            .expect("fixture is rendered")
            .expanded,
        Some(false)
    );
    assert!(calls.borrow().is_empty());
    let (mut managed, _, calls) = select_row(cx, true);
    managed.click("theme.label");
    managed.click("theme.description");
    assert!(managed.node("theme.select").is_none());
    assert!(calls.borrow().is_empty());
}

#[gpui::test]
fn switch_track_and_hit_area_scale_with_control_metrics(cx: &mut TestAppContext) {
    let mut previous = px(0.0);
    for size in [
        ControlSize::Xs,
        ControlSize::Sm,
        ControlSize::Md,
        ControlSize::Lg,
        ControlSize::Touch,
    ] {
        let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
            div()
                .child(
                    Switch::new("switch")
                        .control_size(size)
                        .on_change(|_, _, _| {}),
                )
                .into_any_element()
        });
        let target = harness.bounds("switch").expect("fixture is rendered");
        let track = harness
            .context()
            .debug_bounds("switch.track")
            .expect("fixture is rendered");
        let (height, gap) = harness.update(|_, cx| {
            let metrics = cx.theme().control.get(size);
            (metrics.height, metrics.gap)
        });
        assert_eq!(track.size.height, px(height - gap));
        assert!(track.size.height > previous);
        previous = track.size.height;
        assert!(target.size.height >= px(height));
        assert!(target.size.width >= px(height));
        inside(track, target);
    }
}

const LONG_OPTION: &str = "Follow the current workspace appearance";
const PLACEHOLDER: &str = "Choose an appearance";

fn sized_select(
    cx: &mut TestAppContext,
    width: f32,
    control_width: Option<f32>,
    size: ControlSize,
    clearable: bool,
) -> (Harness, Entity<Select>) {
    let slot: Rc<RefCell<Option<Entity<Select>>>> = Rc::default();
    let build_slot = slot.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |window, cx| {
        let select = build_slot
            .borrow_mut()
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    Select::new("appearance.select", window, cx)
                        .options([
                            SelectOption::new("short", "Dark"),
                            SelectOption::new("long", LONG_OPTION),
                            SelectOption::new("medium", "Follow system"),
                        ])
                        .placeholder(PLACEHOLDER)
                        .control_size(size)
                        .clearable(clearable)
                        .selected("short")
                })
            })
            .clone();
        let theme = cx.theme().clone();
        div()
            .w(px(width))
            .flex()
            .flex_col()
            .child(
                SettingsSection::new("settings", "Appearance")
                    .label_width(px(240.0))
                    .row(
                        SettingsRow::new("appearance", "Application appearance")
                            .description(DESCRIPTION)
                            .when_some(control_width, |row, width| row.control_width(px(width)))
                            .select(select),
                    ),
            )
            // This unconstrained line is an independent measurement of what
            // the trigger must display, using the same public typography.
            .child(
                div().flex().child(
                    gpui_kit::foundation::text(&theme, TypeScale::Label, LONG_OPTION)
                        .text_size(px(theme.control.get(size).font_size))
                        .whitespace_nowrap()
                        .flex_none()
                        .debug_selector(|| "long-option-natural-width".into()),
                ),
            )
            .into_any_element()
    });
    harness.snapshot();
    let select = slot.borrow().clone().expect("select mounted");
    (harness, select)
}

fn assert_single_line_trigger(harness: &mut Harness) -> Bounds<Pixels> {
    let trigger = harness
        .bounds("appearance.select")
        .expect("trigger mounted");
    let label = harness
        .bounds("appearance.select.trigger.label")
        .expect("label mounted");
    let adornments = harness
        .context()
        .debug_bounds("appearance.select.trigger.affordances")
        .expect("affordances mounted");
    inside(label, trigger);
    inside(adornments, trigger);
    assert!(
        label.right() < adornments.left(),
        "text cannot paint under the affordances"
    );
    let line_height = harness.update(|_, cx| cx.theme().typography.label.line_height);
    assert_eq!(
        label.size.height,
        px(line_height),
        "the fixed-height trigger holds one line"
    );
    label
}

#[gpui::test]
fn select_infers_longest_option_width_and_keeps_it_for_every_choice(cx: &mut TestAppContext) {
    for size in [
        ControlSize::Xs,
        ControlSize::Sm,
        ControlSize::Md,
        ControlSize::Lg,
        ControlSize::Touch,
    ] {
        for clearable in [false, true] {
            let (mut harness, select) = sized_select(cx, 720.0, None, size, clearable);
            let width = harness
                .bounds("appearance.field")
                .expect("field mounted")
                .size
                .width;
            let minimum = harness.update(|_, cx| cx.theme().measures.menu_min_width);
            assert!(width >= px(minimum));
            let natural = harness
                .context()
                .debug_bounds("long-option-natural-width")
                .expect("reference text mounted")
                .size
                .width;
            for id in [Some("long"), Some("medium"), Some("short"), None] {
                harness.update(|_, cx| {
                    select.update(cx, |select, cx| {
                        select.set_selected(id.map(Into::into), cx);
                    })
                });
                assert_eq!(
                    harness
                        .bounds("appearance.field")
                        .expect("field mounted")
                        .size
                        .width,
                    width
                );
                let label = assert_single_line_trigger(&mut harness);
                if id == Some("long") {
                    assert!(
                        label.size.width >= natural,
                        "inferred width must not ellipsize the longest option: {label:?}, natural={natural:?}"
                    );
                }
            }
            harness.update(|_, cx| select.update(cx, |select, cx| select.set_disabled(true, cx)));
            assert_eq!(
                harness
                    .bounds("appearance.field")
                    .expect("disabled field mounted")
                    .size
                    .width,
                width
            );
        }
    }
}

#[gpui::test]
fn inferred_select_caps_at_available_width_and_top_aligns_beside_description(
    cx: &mut TestAppContext,
) {
    let (mut wide, _) = sized_select(cx, 720.0, None, ControlSize::Lg, false);
    let names = wide.bounds("appearance.names").expect("names mounted");
    let field = wide.bounds("appearance.field").expect("field mounted");
    assert_eq!(
        field.top(),
        names.top(),
        "multiline prose must not vertically center a dropdown"
    );
    assert!(field.left() > names.right());
    let (mut narrow, select) = sized_select(cx, 420.0, None, ControlSize::Lg, true);
    let long = format!("{LONG_OPTION} and the appearance of all connected displays");
    narrow.update(|_, cx| {
        select.update(cx, |select, cx| {
            select.set_options(vec![SelectOption::new("very-long", long)], cx);
            select.set_selected(Some("very-long".into()), cx);
        })
    });
    let row = narrow.bounds("appearance").expect("row mounted");
    let names = narrow.bounds("appearance.names").expect("names mounted");
    let field = narrow.bounds("appearance.field").expect("field mounted");
    inside(field, row);
    assert_eq!(
        field.size.width, names.size.width,
        "oversized inferred select gets the full available line"
    );
    assert!(
        field.top() > names.bottom(),
        "not enough room for two columns"
    );
    assert_single_line_trigger(&mut narrow);
}

#[gpui::test]
fn constrained_select_preserves_full_value_tooltip_popup_and_label_activation(
    cx: &mut TestAppContext,
) {
    let (mut harness, select) = sized_select(cx, 720.0, Some(220.0), ControlSize::Lg, true);
    harness.update(|_, cx| {
        select.update(cx, |select, cx| {
            select.set_selected(Some("long".into()), cx)
        })
    });
    assert_eq!(
        harness
            .bounds("appearance.field")
            .expect("field mounted")
            .size
            .width,
        px(220.0)
    );
    let label = assert_single_line_trigger(&mut harness);
    let natural = harness
        .context()
        .debug_bounds("long-option-natural-width")
        .expect("reference text mounted");
    assert!(
        label.size.width < natural.size.width,
        "fixture must exercise actual clipping"
    );
    assert_eq!(
        harness
            .node("appearance.select")
            .expect("trigger mounted")
            .value
            .as_deref(),
        Some(LONG_OPTION)
    );
    assert_eq!(
        harness
            .node("appearance.select.trigger.label")
            .expect("label mounted")
            .text
            .as_deref(),
        Some(LONG_OPTION)
    );
    let at = harness.point_in("appearance.select.trigger.label");
    harness
        .context()
        .simulate_mouse_move(at, None, Modifiers::none());
    harness.advance(Duration::from_millis(800));
    let tooltip = harness
        .node("appearance.select.tooltip")
        .expect("full label hover help mounted");
    assert_eq!(tooltip.text.as_deref(), Some(LONG_OPTION));
    assert_eq!(tooltip.describes.as_deref(), Some("appearance.select"));
    harness.click("appearance.label");
    let trigger = harness.node("appearance.select").expect("trigger mounted");
    assert_eq!(trigger.expanded, Some(true));
    assert!(trigger.focused);
    assert_eq!(
        harness
            .node("appearance.select.long")
            .expect("full option mounted")
            .text
            .as_deref(),
        Some(LONG_OPTION)
    );
    harness.keystrokes("escape");
    harness.click("appearance.description");
    assert_eq!(
        harness
            .node("appearance.select")
            .expect("trigger mounted")
            .expanded,
        Some(true)
    );
}

#[gpui::test]
fn ordinary_select_still_fills_its_parent_without_inferred_minimum(cx: &mut TestAppContext) {
    let (mut harness, select) = sized_select(cx, 720.0, None, ControlSize::Lg, false);
    harness.update(|_, cx| {
        select.update(cx, |select, cx| {
            select.set_selected(Some("long".into()), cx)
        })
    });
    harness.remount(move |_, _| div().w(px(180.0)).child(select.clone()).into_any_element());
    assert_eq!(
        harness
            .bounds("appearance.select")
            .expect("trigger mounted")
            .size
            .width,
        px(180.0)
    );
    assert_single_line_trigger(&mut harness);
    assert_eq!(
        harness
            .node("appearance.select")
            .expect("trigger mounted")
            .value
            .as_deref(),
        Some(LONG_OPTION)
    );
}
