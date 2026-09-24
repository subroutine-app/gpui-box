//! Recorder behavior through real GPUI keybinding dispatch and focus changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Entity, Focusable, InteractiveElement, IntoElement, KeyBinding, Keystroke,
    ParentElement, Styled, TestAppContext, div, prelude::FluentBuilder, px,
};
use gpui_kit::prelude::*;
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_testkit::harness::Harness;

gpui::actions!(recorder_tests, [AppShortcut]);

struct RecorderCase {
    harness: Harness,
    recorder: Entity<KeybindingRecorder>,
    events: Rc<RefCell<Vec<KeybindingRecorderEvent>>>,
    app_actions: Rc<Cell<usize>>,
    outside_clicks: Rc<Cell<usize>>,
    visible: Rc<Cell<bool>>,
}

fn recorder_case(cx: &mut TestAppContext) -> RecorderCase {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let slot = Rc::new(RefCell::new(None));
    let held = slot.clone();
    let app_actions = Rc::new(Cell::new(0));
    let actions = app_actions.clone();
    let outside_clicks = Rc::new(Cell::new(0));
    let clicks = outside_clicks.clone();
    let visible = Rc::new(Cell::new(true));
    let show = visible.clone();
    let mut harness = Harness::new(
        cx,
        move |cx| {
            gpui_kit::install(cx);
            cx.bind_keys(
                [
                    "cmd-w",
                    "tab",
                    "shift-tab",
                    "enter",
                    "space",
                    "escape",
                    "ctrl-k ctrl-c",
                ]
                .map(|key| KeyBinding::new(key, AppShortcut, None)),
            );
            cx.on_action(move |_: &AppShortcut, _| actions.set(actions.get() + 1));
        },
        move |window, cx| {
            let recorder = held
                .borrow_mut()
                .get_or_insert_with(|| {
                    let recorder = cx.new(|cx| {
                        KeybindingRecorder::new("shortcut", window, cx)
                            .label("Command shortcut")
                            .binding("ctrl-p")
                    });
                    let sink = sink.clone();
                    cx.subscribe(&recorder, move |_, event: &KeybindingRecorderEvent, _| {
                        sink.borrow_mut().push(event.clone());
                    })
                    .detach();
                    recorder
                })
                .clone();
            let clicks = clicks.clone();
            div()
                .flex()
                .flex_col()
                .w(px(420.0))
                // Even an ancestor raw-key listener must not prevent recording
                // a key that the application's keymap would otherwise consume.
                .capture_key_down(|_, _, _| {})
                .when(show.get(), |root| root.child(recorder))
                .child(div().h(px(80.0)).w_full().semantic_in(
                    cx,
                    NodeSpec::new("outside", Role::Status).text("Nonfocusable outside area"),
                ))
                .child(
                    Button::new("outside-button")
                        .label("Another control")
                        .on_click(move |_, _| clicks.set(clicks.get() + 1)),
                )
                .into_any_element()
        },
    );
    harness.update(|window, _| window.activate_window());
    harness.frame();
    let recorder = slot.borrow().clone().expect("recorder mounted");
    RecorderCase {
        harness,
        recorder,
        events,
        app_actions,
        outside_clicks,
        visible,
    }
}

fn assert_idle_with_original_binding(case: &mut RecorderCase) {
    case.harness.update(|_, cx| {
        let recorder = case.recorder.read(cx);
        assert!(!recorder.is_recording());
        assert_eq!(
            recorder.current_binding().map(|binding| binding.as_ref()),
            Some("ctrl-p")
        );
    });
    let field = case.harness.node("shortcut").expect("recorder visible");
    assert!(!field.busy);
    assert_eq!(field.value.as_deref(), Some("ctrl-p"));
    assert!(case.harness.node("shortcut.prompt").is_none());
}

#[gpui::test]
fn bound_app_shortcuts_are_captured_before_actions_or_chord_matching(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    for key in ["cmd-w", "tab", "shift-tab", "enter", "space", "ctrl-k"] {
        case.events.borrow_mut().clear();
        case.harness.click("shortcut");
        case.harness.keystrokes(key);
        assert_eq!(
            *case.events.borrow(),
            vec![
                KeybindingRecorderEvent::Started,
                KeybindingRecorderEvent::Captured(
                    Keystroke::parse(key).expect("valid key").unparse().into()
                ),
            ],
            "{key} is captured once, without a key-up restarting the recorder",
        );
        assert_eq!(
            case.app_actions.get(),
            0,
            "{key} must not run an app action"
        );
        case.harness.update(|window, cx| {
            assert!(
                !window.has_pending_keystrokes(),
                "{key} must not begin a chord"
            );
            assert!(case.recorder.read(cx).focus_handle(cx).is_focused(window));
        });
        assert_idle_with_original_binding(&mut case);
    }
    case.harness.keystrokes("cmd-w");
    assert_eq!(
        case.app_actions.get(),
        1,
        "idle recorders do not swallow app shortcuts"
    );
}

#[gpui::test]
fn escape_cancels_instead_of_dispatching_the_bound_app_action(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.keystrokes("escape");
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    assert_eq!(case.app_actions.get(), 0);
    assert_idle_with_original_binding(&mut case);
    case.harness.keystrokes("escape");
    assert_eq!(
        case.app_actions.get(),
        1,
        "cancellation restores app dispatch"
    );
}

#[gpui::test]
fn escape_can_still_be_explicitly_captured(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.update(|_, cx| {
        case.recorder
            .update(cx, |recorder, cx| recorder.set_allow_escape(true, cx));
    });
    case.harness.click("shortcut");
    case.harness.keystrokes("escape");
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Captured("escape".into())
        ],
    );
    assert_eq!(case.app_actions.get(), 0);
    assert_idle_with_original_binding(&mut case);
}

#[gpui::test]
fn clicking_outside_cancels_even_without_moving_focus(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.click("outside");
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    case.harness.update(|window, cx| {
        assert!(case.recorder.read(cx).focus_handle(cx).is_focused(window));
    });
    assert_idle_with_original_binding(&mut case);
    case.harness.keystrokes("cmd-w");
    assert_eq!(case.app_actions.get(), 1);
}

#[gpui::test]
fn outside_click_and_blur_cancel_once_without_eating_the_click(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.click("outside-button");
    assert_eq!(case.outside_clicks.get(), 1);
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    assert_idle_with_original_binding(&mut case);
}

#[gpui::test]
fn programmatic_focus_loss_cancels_and_refocusing_does_not_resume(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.update(|window, _| window.blur());
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    assert_idle_with_original_binding(&mut case);
    case.harness.update(|window, cx| {
        window.focus(&case.recorder.read(cx).focus_handle(cx), cx);
    });
    case.harness.keystrokes("cmd-w");
    assert_eq!(case.app_actions.get(), 1);
    assert_idle_with_original_binding(&mut case);
}

#[gpui::test]
fn unmounting_a_focused_recorder_cancels_its_session(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.visible.set(false);
    case.harness.frame();
    assert!(case.harness.node("shortcut").is_none());
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    case.harness.keystrokes("cmd-w");
    assert_eq!(
        case.app_actions.get(),
        1,
        "a retained but hidden entity cannot eat keys"
    );
    case.visible.set(true);
    case.harness.frame();
    assert_idle_with_original_binding(&mut case);
}

#[gpui::test]
fn deactivating_the_window_cancels_recording(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.context().deactivate_window();
    case.harness.frame();
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    assert_idle_with_original_binding(&mut case);
}

#[gpui::test]
fn standalone_recorder_is_tabbable_and_starts_with_enter_or_space(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.update(|window, cx| window.focus_next(cx));
    case.harness.update(|window, cx| {
        assert!(case.recorder.read(cx).focus_handle(cx).is_focused(window));
    });
    for key in ["enter", "space"] {
        case.events.borrow_mut().clear();
        case.harness.keystrokes(key);
        assert_eq!(
            *case.events.borrow(),
            vec![KeybindingRecorderEvent::Started]
        );
        assert!(case.harness.node("shortcut").expect("visible").busy);
        let prompt = case
            .harness
            .node("shortcut.prompt")
            .expect("visible capture prompt");
        assert_eq!(prompt.text.as_deref(), Some("Press shortcut…"));
        assert!(prompt.bounds.area() > 0.0);
        case.harness.keystrokes("cmd-w");
        assert_idle_with_original_binding(&mut case);
    }
    assert_eq!(
        case.app_actions.get(),
        0,
        "activation belongs to the focused recorder"
    );
}

#[gpui::test]
fn disabling_a_recorder_cancels_and_releases_app_shortcuts(cx: &mut TestAppContext) {
    let mut case = recorder_case(cx);
    case.harness.click("shortcut");
    case.harness.update(|_, cx| {
        case.recorder
            .update(cx, |recorder, cx| recorder.set_disabled(true, cx));
    });
    case.harness.keystrokes("cmd-w enter");
    case.harness.click("shortcut");
    assert_eq!(case.app_actions.get(), 2);
    assert_eq!(
        *case.events.borrow(),
        vec![
            KeybindingRecorderEvent::Started,
            KeybindingRecorderEvent::Cancelled
        ],
    );
    assert_idle_with_original_binding(&mut case);
    case.harness.update(|_, cx| {
        case.recorder
            .update(cx, |recorder, cx| recorder.set_disabled(false, cx));
    });
    assert_idle_with_original_binding(&mut case);
    case.harness.click("shortcut");
    case.harness.keystrokes("tab");
    assert!(
        matches!(case.events.borrow().last(), Some(KeybindingRecorderEvent::Captured(key)) if key == "tab")
    );
}
