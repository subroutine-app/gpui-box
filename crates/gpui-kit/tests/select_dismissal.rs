//! Anchored Select dismissal is window-local and never owns a caller's action or value.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    Entity, FocusHandle, Focusable, Modifiers, MouseButton, TestAppContext, div, prelude::*, px,
};
use gpui_kit::prelude::*;
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_testkit::harness::Harness;

type Events = Rc<RefCell<Vec<SelectEvent>>>;

#[derive(Clone)]
struct Controls {
    a: Entity<Select>,
    b: Entity<Select>,
    root: FocusHandle,
}

struct Settings {
    harness: Harness,
    controls: Controls,
    a_events: Events,
    b_events: Events,
    button_calls: Rc<Cell<usize>>,
    plain_calls: Rc<Cell<usize>>,
}

impl Settings {
    // Installation belongs to the test: two fixtures must share the same globals
    // when proving that an active-select registry is scoped to its window.
    fn new(cx: &mut TestAppContext, track_root: bool) -> Self {
        let slot: Rc<RefCell<Option<Controls>>> = Rc::default();
        let build_slot = slot.clone();
        let button_calls = Rc::new(Cell::new(0));
        let plain_calls = Rc::new(Cell::new(0));
        let button_sink = button_calls.clone();
        let plain_sink = plain_calls.clone();
        let mut harness = Harness::new(
            cx,
            |_| {},
            move |window, cx| {
                let controls = build_slot
                    .borrow_mut()
                    .get_or_insert_with(|| Controls {
                        a: cx.new(|cx| {
                            Select::new("a.select", window, cx)
                                .options(options())
                                .selected("system")
                        }),
                        b: cx.new(|cx| {
                            Select::new("b.select", window, cx)
                                .options(options())
                                .selected("system")
                        }),
                        root: cx.focus_handle(),
                    })
                    .clone();
                let button_sink = button_sink.clone();
                let plain_sink = plain_sink.clone();
                div()
                    .id("settings.root")
                    .relative()
                    .w(px(780.0))
                    .h(px(650.0))
                    .when(track_root, |root| root.track_focus(&controls.root))
                    .on_key_down(|event, window, cx| {
                        if event.keystroke.key == "tab" {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev(cx);
                            } else {
                                window.focus_next(cx);
                            }
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .absolute()
                            .left(px(24.0))
                            .top(px(24.0))
                            .w(px(720.0))
                            .child(
                                SettingsRow::new("a", "Application appearance")
                                    .description("Choose this workspace's appearance")
                                    .select(controls.a),
                            ),
                    )
                    // The two-option menu above cannot cover the second row.
                    .child(
                        div()
                            .absolute()
                            .left(px(24.0))
                            .top(px(300.0))
                            .w(px(720.0))
                            .child(
                                SettingsRow::new("b", "Editor appearance")
                                    .description("Choose this editor's appearance")
                                    .select(controls.b),
                            ),
                    )
                    .child(
                        div().absolute().left(px(24.0)).top(px(530.0)).child(
                            Button::new("neighbor")
                                .label("Neighbor action")
                                .on_click(move |_, _| button_sink.set(button_sink.get() + 1)),
                        ),
                    )
                    .child(
                        div()
                            .id("plain-action")
                            .absolute()
                            .left(px(280.0))
                            .top(px(530.0))
                            .w(px(180.0))
                            .h(px(60.0))
                            .on_click(move |_, _, _| plain_sink.set(plain_sink.get() + 1))
                            .child("Non-focusable action")
                            .semantic_in(cx, NodeSpec::new("plain-action", Role::Button)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(520.0))
                            .top(px(530.0))
                            .w(px(200.0))
                            .h(px(60.0))
                            .semantic_in(cx, NodeSpec::new("blank", Role::Group)),
                    )
                    .into_any_element()
            },
        );
        harness.update(|window, _| window.activate_window());
        harness.snapshot();
        let controls = slot.borrow().clone().expect("settings mounted");
        let a_events = events(&mut harness, &controls.a);
        let b_events = events(&mut harness, &controls.b);
        if track_root {
            harness.update(|window, cx| window.focus(&controls.root, cx));
        }
        Self {
            harness,
            controls,
            a_events,
            b_events,
            button_calls,
            plain_calls,
        }
    }

    fn assert_open(&mut self, a: bool, b: bool) {
        self.harness.update(|_, cx| {
            assert_eq!(self.controls.a.read(cx).is_open(), a, "Select A state");
            assert_eq!(self.controls.b.read(cx).is_open(), b, "Select B state");
        });
        let snapshot = self.harness.snapshot();
        for (id, open) in [("a.select", a), ("b.select", b)] {
            assert_eq!(
                snapshot.find(id).expect("trigger mounted").expanded,
                Some(open)
            );
            assert_eq!(snapshot.find(&format!("{id}.menu")).is_some(), open);
            assert_eq!(snapshot.find(&format!("{id}.dark")).is_some(), open);
        }
    }

    fn assert_events(&self, a: &[SelectEvent], b: &[SelectEvent]) {
        assert_eq!(self.a_events.borrow().as_slice(), a, "Select A events");
        assert_eq!(self.b_events.borrow().as_slice(), b, "Select B events");
    }
}

fn options() -> [SelectOption; 2] {
    [
        SelectOption::new("system", "Follow system"),
        SelectOption::new("dark", "Dark"),
    ]
}

fn events(harness: &mut Harness, select: &Entity<Select>) -> Events {
    let events: Events = Rc::default();
    let sink = events.clone();
    harness.update(|_, cx| {
        cx.subscribe(select, move |_, event: &SelectEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
    });
    events
}

fn repeat_activation(cx: &mut TestAppContext, target: &str) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.click(target);
    settings.assert_open(true, false);
    settings.assert_events(&[SelectEvent::Opened], &[]);
    assert!(settings.harness.node("a.select").expect("trigger").focused);

    settings.harness.click(target);
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);

    settings.harness.click(target);
    settings.assert_open(true, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Closed,
            SelectEvent::Opened,
        ],
        &[],
    );
}

#[gpui::test]
fn repeated_label_clicks_toggle_without_close_open_bounce(cx: &mut TestAppContext) {
    repeat_activation(cx, "a.label");
}

#[gpui::test]
fn repeated_description_clicks_toggle_without_close_open_bounce(cx: &mut TestAppContext) {
    repeat_activation(cx, "a.description");
}

#[gpui::test]
fn repeated_direct_trigger_clicks_toggle_without_close_open_bounce(cx: &mut TestAppContext) {
    repeat_activation(cx, "a.select");
}

#[gpui::test]
fn names_activate_on_left_down_before_the_settings_root_can_take_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    for target in ["a.label", "a.description"] {
        let mut settings = Settings::new(cx, true);
        for open in [true, false] {
            let at = settings.harness.point_in(target);
            settings.harness.context().simulate_mouse_down(
                at,
                MouseButton::Left,
                Modifiers::none(),
            );
            settings.harness.context().run_until_parked();
            settings.assert_open(open, false);
            settings.harness.update(|window, cx| {
                assert!(settings.controls.a.focus_handle(cx).is_focused(window));
                assert!(!settings.controls.root.is_focused(window));
            });
            let expected = if open {
                vec![SelectEvent::Opened]
            } else {
                vec![SelectEvent::Opened, SelectEvent::Closed]
            };
            settings.assert_events(&expected, &[]);
            settings
                .harness
                .context()
                .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
            settings.harness.context().run_until_parked();
            settings.assert_open(open, false);
            settings.assert_events(&expected, &[]);
        }
    }
}

fn switch_selects(cx: &mut TestAppContext, first: &str, second: &str) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.click(first);
    settings.assert_open(true, false);
    settings.assert_events(&[SelectEvent::Opened], &[]);
    let menu = settings.harness.bounds("a.select.menu").expect("A menu");
    let target = settings.harness.point_in(second);
    assert!(
        !menu.contains(&target),
        "A's menu must not intercept B's activation"
    );

    settings.harness.click(second);
    settings.assert_open(false, true);
    settings.assert_events(
        &[SelectEvent::Opened, SelectEvent::Closed],
        &[SelectEvent::Opened],
    );
}

#[gpui::test]
fn activating_b_by_label_closes_a(cx: &mut TestAppContext) {
    switch_selects(cx, "a.label", "b.label");
}

#[gpui::test]
fn activating_b_by_direct_trigger_closes_a(cx: &mut TestAppContext) {
    switch_selects(cx, "a.select", "b.select");
}

#[gpui::test]
fn presentation_changes_claim_anchored_ownership_before_render(cx: &mut TestAppContext) {
    use gpui_kit::overlay::popover::PickerPresentation;
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    let Controls { a, b, .. } = &settings.controls;
    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| {
            select.open(window, cx);
            select.set_presentation(PickerPresentation::Bottom, cx);
            select.set_presentation(PickerPresentation::Anchored, cx);
        });
        b.update(cx, |select, cx| select.open(window, cx));
        assert!(!a.read(cx).is_open());
        assert!(b.read(cx).is_open());
    });
    settings.assert_open(false, true);
    settings.assert_events(
        &[SelectEvent::Opened, SelectEvent::Closed],
        &[SelectEvent::Opened],
    );
}

#[gpui::test]
fn same_frame_programmatic_opens_leave_only_b_open(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    let Controls { a, b, .. } = &settings.controls;
    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| select.open(window, cx));
        b.update(cx, |select, cx| select.open(window, cx));
        // No intervening draw, installed outside handler, or focus notification.
        assert!(
            !a.read(cx).is_open(),
            "ownership must change during open, not next render"
        );
        assert!(b.read(cx).is_open());
    });
    settings.assert_open(false, true);
    settings.assert_events(
        &[SelectEvent::Opened, SelectEvent::Closed],
        &[SelectEvent::Opened],
    );
}

#[gpui::test]
fn programmatic_open_remains_idempotent(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    let a = settings.controls.a.clone();
    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| {
            select.open(window, cx);
            select.open(window, cx);
        });
    });
    settings.assert_open(true, false);
    settings
        .harness
        .update(|window, cx| a.update(cx, |select, cx| select.open(window, cx)));
    settings.assert_open(true, false);
    settings.assert_events(&[SelectEvent::Opened], &[]);
}

#[gpui::test]
fn a_disabled_select_cannot_claim_the_active_slot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    let Controls { a, b, .. } = &settings.controls;
    settings.harness.update(|window, cx| {
        b.update(cx, |select, cx| select.set_disabled(true, cx));
        a.update(cx, |select, cx| select.open(window, cx));
        b.update(cx, |select, cx| select.open(window, cx));
        assert!(a.read(cx).is_open());
        assert!(!b.read(cx).is_open());
    });
    settings.assert_open(true, false);
    settings.assert_events(&[SelectEvent::Opened], &[]);
}

#[gpui::test]
fn disabled_label_description_and_trigger_install_no_activation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.update(|_, cx| {
        settings
            .controls
            .a
            .update(cx, |select, cx| select.set_disabled(true, cx));
    });
    for target in ["a.label", "a.description", "a.select"] {
        settings.harness.click(target);
        settings.assert_open(false, false);
        settings.assert_events(&[], &[]);
    }
}

#[gpui::test]
fn disabling_an_open_select_reports_closed_exactly_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    settings.harness.click("a.select");
    settings.harness.update(|_, cx| {
        settings
            .controls
            .a
            .update(cx, |select, cx| select.set_disabled(true, cx));
    });
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
    settings.harness.update(|_, cx| {
        settings.controls.a.update(cx, |select, cx| {
            select.set_disabled(true, cx);
            select.close(cx);
            select.set_disabled(false, cx);
        });
    });
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
}

fn outside_click(cx: &mut TestAppContext, target: &str, button_calls: usize, plain_calls: usize) {
    cx.update(gpui_kit::install);
    // Without a focus-tracked root, a blank/action click cannot rely on blur.
    let mut settings = Settings::new(cx, false);
    settings.harness.click("a.select");
    settings.assert_open(true, false);
    let menu = settings.harness.bounds("a.select.menu").expect("A menu");
    assert!(!menu.contains(&settings.harness.point_in(target)));
    settings.harness.click(target);
    assert_eq!(settings.button_calls.get(), button_calls);
    assert_eq!(settings.plain_calls.get(), plain_calls);
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
}

#[gpui::test]
fn outside_button_click_closes_and_fires_the_neighbor_action(cx: &mut TestAppContext) {
    outside_click(cx, "neighbor", 1, 0);
}

#[gpui::test]
fn outside_nonfocusable_action_closes_without_consuming_the_click(cx: &mut TestAppContext) {
    outside_click(cx, "plain-action", 0, 1);
}

#[gpui::test]
fn outside_blank_click_closes_without_a_focus_change(cx: &mut TestAppContext) {
    outside_click(cx, "blank", 0, 0);
}

#[gpui::test]
fn inside_choice_reports_selected_then_closed_once_without_accepting_for_the_caller(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.click("a.label");
    settings.harness.click("a.select.dark");
    settings.assert_open(false, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Selected("dark".into()),
            SelectEvent::Closed,
        ],
        &[],
    );
    assert_eq!(settings.button_calls.get(), 0);
    assert_eq!(settings.plain_calls.get(), 0);
    assert_eq!(
        settings
            .harness
            .node("a.select")
            .expect("trigger")
            .value
            .as_deref(),
        Some("Follow system")
    );
    settings.harness.update(|_, cx| {
        assert_eq!(
            settings
                .controls
                .a
                .read(cx)
                .selected_id()
                .map(AsRef::as_ref),
            Some("system")
        );
    });
    settings.harness.click("a.label");
    assert_eq!(
        settings
            .harness
            .node("a.select.system")
            .expect("original choice")
            .checked,
        Some(true)
    );
}

#[gpui::test]
fn label_activation_preserves_the_existing_caller_binding(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    let (signal, _binding) = settings.harness.update(|_, cx| {
        let signal = Signal::new(cx, Some("system".into()));
        let binding = Select::bind(&settings.controls.a, &signal, cx);
        (signal, binding)
    });
    settings.harness.click("a.label");
    settings.harness.click("a.select.dark");
    settings.assert_open(false, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Selected("dark".into()),
            SelectEvent::Closed,
        ],
        &[],
    );
    settings.harness.update(|_, cx| {
        assert_eq!(signal.get(cx).as_deref(), Some("dark"));
        signal.set(cx, Some("system".into()));
    });
    assert_eq!(
        settings
            .harness
            .node("a.select")
            .expect("trigger")
            .value
            .as_deref(),
        Some("Follow system")
    );
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Selected("dark".into()),
            SelectEvent::Closed,
        ],
        &[],
    );
}

#[gpui::test]
fn moving_focus_to_the_settings_root_closes_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.click("a.select");
    settings
        .harness
        .update(|window, cx| window.focus(&settings.controls.root, cx));
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
}

#[gpui::test]
fn tab_away_closes_without_consuming_keyboard_navigation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, true);
    settings.harness.click("a.select");
    settings.harness.keystrokes("tab");
    assert!(
        settings
            .harness
            .node("neighbor")
            .expect("next tab stop")
            .focused
    );
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
}

#[gpui::test]
fn separate_windows_with_the_same_select_identities_do_not_dismiss_each_other(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::install);
    let mut first = Settings::new(cx, false);
    let mut second = Settings::new(cx, false);
    assert_ne!(
        first.harness.window().window_id(),
        second.harness.window().window_id()
    );
    first.harness.click("a.select");
    second.harness.click("a.select");
    first.assert_open(true, false);
    second.assert_open(true, false);
    first.assert_events(&[SelectEvent::Opened], &[]);
    second.assert_events(&[SelectEvent::Opened], &[]);

    first.harness.click("b.select");
    first.assert_open(false, true);
    second.assert_open(true, false);
    first.assert_events(
        &[SelectEvent::Opened, SelectEvent::Closed],
        &[SelectEvent::Opened],
    );
    second.assert_events(&[SelectEvent::Opened], &[]);
}

#[gpui::test]
fn closing_the_stale_owner_does_not_erase_b_from_the_registry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    let Controls { a, b, .. } = &settings.controls;
    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| select.open(window, cx));
        b.update(cx, |select, cx| select.open(window, cx));
        a.update(cx, |select, cx| {
            select.close(cx);
            select.close(cx);
        });
        assert!(b.read(cx).is_open());
        // If A's late close cleared B's registration, opening A cannot evict B.
        a.update(cx, |select, cx| select.open(window, cx));
        assert!(!b.read(cx).is_open());
        assert!(a.read(cx).is_open());
    });
    settings.assert_open(true, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Closed,
            SelectEvent::Opened,
        ],
        &[SelectEvent::Opened, SelectEvent::Closed],
    );
}

fn retired_names_are_not_an_exemption(cx: &mut TestAppContext, hidden: bool) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    settings.harness.click("a.label");
    let names = settings.harness.bounds("a.names").expect("bound names");
    let old_label = settings.harness.point_in("a.label");
    let trigger = settings.harness.bounds("a.select").expect("trigger");
    settings.harness.update(|_, cx| {
        settings
            .controls
            .a
            .update(cx, |select, cx| select.close(cx))
    });
    settings.a_events.borrow_mut().clear();

    let a = settings.controls.a.clone();
    let calls = settings.plain_calls.clone();
    settings.harness.remount(move |_, cx| {
        let calls = calls.clone();
        div()
            .relative()
            .size_full()
            .when(hidden, |root| {
                root.child(
                    div().hidden().child(
                        SettingsRow::new("a", "Application appearance")
                            .description("Choose this workspace's appearance")
                            .select(a.clone()),
                    ),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(trigger.left())
                    .top(trigger.top())
                    .w(trigger.size.width)
                    .child(a.clone()),
            )
            .child(
                div()
                    .id("replacement")
                    .absolute()
                    .left(names.left())
                    .top(names.top())
                    .w(names.size.width)
                    .h(names.size.height)
                    .on_click(move |_, _, _| calls.set(calls.get() + 1))
                    .semantic_in(cx, NodeSpec::new("replacement", Role::Button)),
            )
            .into_any_element()
    });
    assert!(settings.harness.node("a.names").is_none());
    assert!(settings.harness.node("a.label").is_none());
    settings.harness.update(|window, cx| {
        settings
            .controls
            .a
            .update(cx, |select, cx| select.open(window, cx))
    });
    assert!(settings.harness.node("a.select.menu").is_some());
    assert!(
        settings
            .harness
            .bounds("replacement")
            .expect("replacement")
            .contains(&old_label)
    );
    settings
        .harness
        .context()
        .simulate_click(old_label, Modifiers::none());
    settings.harness.context().run_until_parked();
    assert_eq!(settings.plain_calls.get(), 1);
    assert_eq!(
        settings
            .harness
            .node("a.select")
            .expect("retained trigger")
            .expanded,
        Some(false)
    );
    assert!(settings.harness.node("a.select.menu").is_none());
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);
}

#[gpui::test]
fn removed_settings_names_are_not_a_stale_outside_click_exemption(cx: &mut TestAppContext) {
    retired_names_are_not_an_exemption(cx, false);
}

#[gpui::test]
fn hidden_settings_names_are_not_a_stale_outside_click_exemption(cx: &mut TestAppContext) {
    retired_names_are_not_an_exemption(cx, true);
}

#[gpui::test]
fn public_toggle_opens_closes_and_refuses_disabled_controls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::install);
    let mut settings = Settings::new(cx, false);
    let a = settings.controls.a.clone();
    let b = settings.controls.b.clone();
    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| select.toggle(window, cx));
    });
    settings.assert_open(true, false);
    settings.assert_events(&[SelectEvent::Opened], &[]);

    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| {
            select.open(window, cx);
            select.toggle(window, cx);
        });
    });
    settings.assert_open(false, false);
    settings.assert_events(&[SelectEvent::Opened, SelectEvent::Closed], &[]);

    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| select.toggle(window, cx));
        b.update(cx, |select, cx| {
            select.set_disabled(true, cx);
            select.toggle(window, cx);
        });
    });
    settings.assert_open(true, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Closed,
            SelectEvent::Opened,
        ],
        &[],
    );

    settings.harness.update(|window, cx| {
        a.update(cx, |select, cx| {
            select.set_disabled(true, cx);
            select.toggle(window, cx);
            select.toggle(window, cx);
        });
    });
    settings.assert_open(false, false);
    settings.assert_events(
        &[
            SelectEvent::Opened,
            SelectEvent::Closed,
            SelectEvent::Opened,
            SelectEvent::Closed,
        ],
        &[],
    );
}
