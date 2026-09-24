//! Keymap focus, activation, and lifetime behavior through real GPUI dispatch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Entity, FocusHandle, IntoElement, KeyBinding, ParentElement, Styled,
    TestAppContext, div, prelude::FluentBuilder, px,
};
use gpui_kit::prelude::*;
use gpui_kit_testkit::harness::Harness;

gpui::actions!(keymap_editor_tests, [AppShortcut]);

const PRIMARY: &str = "keymap.open.binding.ctrl-p.edit";
const ALTERNATE: &str = "keymap.open.binding.alt-p.edit";
const OTHER: &str = "keymap.other.binding.ctrl-o.edit";
const UNBOUND: &str = "keymap.empty.add";

fn commands() -> Vec<KeymapCommand> {
    vec![
        KeymapCommand::new("open", "Open").bindings([
            KeymapBinding::new("ctrl-p", "ctrl-p"),
            KeymapBinding::new("alt-p", "alt-p"),
        ]),
        KeymapCommand::new("other", "Other").bindings([KeymapBinding::new("ctrl-o", "ctrl-o")]),
        KeymapCommand::new("empty", "Unbound"),
    ]
}

struct Case {
    harness: Harness,
    editor: Entity<KeymapEditor>,
    events: Rc<RefCell<Vec<KeymapEditorEvent>>>,
    app_actions: Rc<Cell<usize>>,
    visible: Rc<Cell<bool>>,
    apply_capture: Rc<Cell<bool>>,
}

fn setup(cx: &mut TestAppContext) -> Case {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let app_actions = Rc::new(Cell::new(0));
    let actions = app_actions.clone();
    let visible = Rc::new(Cell::new(true));
    let show = visible.clone();
    let apply_capture = Rc::new(Cell::new(false));
    let apply = apply_capture.clone();
    let slot = Rc::new(RefCell::new(None));
    let held = slot.clone();
    let mut harness = Harness::new(
        cx,
        move |cx| {
            gpui_kit::install(cx);
            cx.bind_keys(
                ["enter", "space", "ctrl-enter", "cmd-w", "tab", "escape"]
                    .map(|key| KeyBinding::new(key, AppShortcut, None)),
            );
            cx.on_action(move |_: &AppShortcut, _| actions.set(actions.get() + 1));
        },
        move |window, cx| {
            let editor = held
                .borrow_mut()
                .get_or_insert_with(|| {
                    let editor =
                        cx.new(|cx| KeymapEditor::new("keymap", window, cx).commands(commands()));
                    let sink = sink.clone();
                    let apply = apply.clone();
                    cx.subscribe(&editor, move |editor, event: &KeymapEditorEvent, cx| {
                        sink.borrow_mut().push(event.clone());
                        if !apply.get() {
                            return;
                        }
                        let (command_id, old_id, keystroke) = match event {
                            KeymapEditorEvent::ReplaceCaptured {
                                command_id,
                                binding_id,
                                keystroke,
                            } => (command_id, Some(binding_id), keystroke),
                            KeymapEditorEvent::AddCaptured {
                                command_id,
                                keystroke,
                            } => (command_id, None, keystroke),
                            _ => return,
                        };
                        // Model a host whose binding identity is the shortcut itself.
                        editor.update(cx, |editor, cx| {
                            let mut current = editor.current_commands().to_vec();
                            let command = current
                                .iter_mut()
                                .find(|command| command.id() == command_id)
                                .expect("captured command still exists");
                            let mut bindings = command.effective_bindings().to_vec();
                            let replacement =
                                KeymapBinding::new(keystroke.clone(), keystroke.clone());
                            if let Some(old_id) = old_id {
                                let binding = bindings
                                    .iter_mut()
                                    .find(|binding| binding.id() == old_id)
                                    .expect("replaced binding still exists");
                                *binding = replacement;
                            } else {
                                bindings.push(replacement);
                            }
                            *command = command.clone().bindings(bindings);
                            editor.set_commands(current, cx);
                        });
                    })
                    .detach();
                    editor
                })
                .clone();
            div()
                .flex()
                .flex_col()
                .w(px(720.0))
                .when(show.get(), |root| root.child(editor))
                .child(Button::new("outside").label("Outside").on_click(|_, _| {}))
                .into_any_element()
        },
    );
    harness.update(|window, _| window.activate_window());
    harness.frame();
    let editor = slot.borrow().clone().expect("editor mounted");
    Case {
        harness,
        editor,
        events,
        app_actions,
        visible,
        apply_capture,
    }
}

fn focus(case: &mut Case, id: &str) -> FocusHandle {
    case.harness.update(|window, _| window.blur());
    for _ in 0..40 {
        case.harness.update(|window, cx| window.focus_next(cx));
        if case.harness.node(id).expect("field exists").focused {
            return case
                .harness
                .update(|window, cx| window.focused(cx).expect("focused handle"));
        }
    }
    panic!("{id} was not reachable by tab navigation");
}

#[gpui::test]
fn large_shortcut_fields_keep_their_size_and_fit_narrow_settings_rows(cx: &mut TestAppContext) {
    for width in [380.0, 580.0] {
        let mut harness = Harness::new(cx, gpui_kit::install, move |window, cx| {
            let editor = window.use_keyed_state("large-keymap", cx, |window, cx| {
                KeymapEditor::new("large-keymap", window, cx)
                    .large()
                    .commands([KeymapCommand::new("open", "Open workspace settings")
                        .context("Application")
                        .bindings([KeymapBinding::new("primary", "cmd-shift-p")])])
            });
            div().w(px(width)).child(editor).into_any_element()
        });
        let field_id = "large-keymap.open.binding.primary.edit";
        let field = harness.bounds(field_id).expect("shortcut field");
        let row = harness.bounds("large-keymap.open").expect("setting row");
        let height = harness.update(|_, cx| px(cx.theme().control.lg.height));
        assert_eq!(field.size.height, height);
        assert!(field.left() >= row.left() && field.right() <= row.right());
        harness.click(field_id);
        let recording = harness
            .bounds("large-keymap.recorder")
            .expect("inline capture");
        assert_eq!(
            recording, field,
            "capture must not move or resize the field"
        );
        assert_eq!(harness.bounds("large-keymap.open"), Some(row));
        harness.keystrokes("escape");
        assert_eq!(harness.bounds(field_id), Some(field));
    }
}

#[gpui::test]
fn bound_enter_and_space_activate_only_the_focused_actionable_field(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    for field in [PRIMARY, ALTERNATE, OTHER, UNBOUND] {
        for key in ["enter", "space"] {
            focus(&mut case, field);
            case.harness.keystrokes(key);
            assert!(
                case.harness
                    .node("keymap.recorder")
                    .expect("capture started")
                    .busy
            );
            assert!(case.harness.node(field).is_none());
            assert_eq!(case.app_actions.get(), 0);
            case.harness.keystrokes("tab");
            assert!(case.harness.node("keymap.recorder").is_none());
            assert!(case.harness.node(field).expect("restored field").focused);
            assert_eq!(case.app_actions.get(), 0);
        }
    }
    assert_eq!(
        case.events.borrow().len(),
        8,
        "one capture per activation, not its activation key"
    );
    assert!(case.events.borrow().iter().all(|event| matches!(event,
        KeymapEditorEvent::ReplaceCaptured { keystroke, .. } | KeymapEditorEvent::AddCaptured { keystroke, .. }
        if keystroke == "tab")));
    case.harness
        .update(|_, cx| assert_eq!(case.editor.read(cx).current_commands(), commands()));
    case.harness.keystrokes("ctrl-enter cmd-w");
    assert_eq!(
        case.app_actions.get(),
        2,
        "other shortcuts remain application-owned"
    );
}

#[gpui::test]
fn escape_and_capture_restore_the_selected_field_without_restarting(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    for field in [PRIMARY, ALTERNATE, UNBOUND] {
        for key in ["escape", "cmd-w"] {
            case.harness.click(field);
            case.harness.keystrokes(key);
            assert!(case.harness.node(field).expect("restored field").focused);
            assert!(case.harness.node("keymap.recorder").is_none());
        }
    }
    assert_eq!(case.app_actions.get(), 0);
    assert_eq!(case.events.borrow().len(), 6);
}

#[gpui::test]
fn capture_restores_a_current_field_after_the_host_replaces_binding_identity(
    cx: &mut TestAppContext,
) {
    let mut case = setup(cx);
    case.apply_capture.set(true);
    case.harness.click(PRIMARY);
    case.harness.keystrokes("cmd-w");
    assert!(case.harness.node(PRIMARY).is_none());
    let current = "keymap.open.binding.cmd-w.edit";
    assert!(
        case.harness
            .node(current)
            .expect("host replacement")
            .focused
    );
    assert_eq!(case.app_actions.get(), 0);
    assert_eq!(
        case.events.borrow().as_slice(),
        [KeymapEditorEvent::ReplaceCaptured {
            command_id: "open".into(),
            binding_id: "ctrl-p".into(),
            keystroke: "cmd-w".into(),
        }]
    );
    case.harness.keystrokes("enter");
    assert!(
        case.harness
            .node("keymap.recorder")
            .expect("replacement can be edited")
            .busy
    );
    case.harness.keystrokes("escape");
    assert!(case.harness.node(current).expect("current field").focused);

    case.harness.click(UNBOUND);
    case.harness.keystrokes("ctrl-n");
    assert!(
        case.harness.node(UNBOUND).is_some(),
        "add is now the alternate button"
    );
    assert!(
        case.harness
            .node("keymap.empty.binding.ctrl-n.edit")
            .expect("new binding")
            .focused
    );
}

#[gpui::test]
fn host_invalidation_restores_only_visible_actionable_fields(cx: &mut TestAppContext) {
    for reason in [
        "filter",
        "removed command",
        "removed binding",
        "refused",
        "disabled",
        "no results",
    ] {
        let mut case = setup(cx);
        case.harness.click(PRIMARY);
        let mut current = commands();
        match reason {
            "removed command" => {
                current.remove(0);
            }
            "removed binding" => {
                current[0] = current[0]
                    .clone()
                    .bindings([current[0].effective_bindings()[1].clone()]);
            }
            "refused" => {
                current[0] = current[0].clone().refused("Managed");
            }
            _ => {}
        }
        case.harness.update(|_, cx| {
            case.editor.update(cx, |editor, cx| match reason {
                "filter" => editor.set_query("Other", cx),
                "no results" => editor.set_query("Nothing matches", cx),
                "disabled" => editor.set_disabled(true, cx),
                _ => editor.set_commands(current, cx),
            })
        });
        let snapshot = case.harness.snapshot();
        assert!(snapshot.find("keymap.recorder").is_none(), "{reason}");
        assert_eq!(
            case.events.borrow().as_slice(),
            [KeymapEditorEvent::RecordingCancelled {
                command_id: "open".into()
            }],
            "{reason}"
        );
        match reason {
            "disabled" | "no results" => case
                .harness
                .update(|window, cx| assert!(window.focused(cx).is_none(), "{reason}")),
            "removed binding" => assert!(
                snapshot
                    .find(ALTERNATE)
                    .expect("remaining alternate")
                    .focused
            ),
            _ => assert!(
                snapshot.find(OTHER).expect("fallback row").focused,
                "{reason}"
            ),
        }
    }
}

#[gpui::test]
fn outside_focus_and_host_focus_changes_take_precedence_over_restoration(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    let outside = focus(&mut case, "outside");
    case.harness.click(PRIMARY);
    case.harness.click("outside");
    assert!(
        case.harness
            .node("outside")
            .expect("outside button")
            .focused
    );
    assert!(case.harness.node("keymap.recorder").is_none());

    case.harness.click(ALTERNATE);
    case.harness.update(|window, cx| window.focus(&outside, cx));
    assert!(
        case.harness
            .node("outside")
            .expect("outside button")
            .focused
    );
    assert!(case.harness.node("keymap.recorder").is_none());
    assert_eq!(case.events.borrow().len(), 2);
}

#[gpui::test]
fn retained_unmounted_disabled_and_refused_fields_do_not_intercept(cx: &mut TestAppContext) {
    for reason in ["unmounted", "disabled", "refused", "filtered"] {
        let mut case = setup(cx);
        let old_focus = focus(&mut case, PRIMARY);
        if reason == "unmounted" {
            case.visible.set(false);
            case.harness.frame();
        } else {
            case.harness.update(|_, cx| {
                case.editor.update(cx, |editor, cx| match reason {
                    "disabled" => editor.set_disabled(true, cx),
                    "filtered" => editor.set_query("Other", cx),
                    _ => {
                        let mut current = commands();
                        current[0] = current[0].clone().refused("Managed");
                        editor.set_commands(current, cx);
                    }
                })
            });
        }
        // A retained FocusHandle can still be focused even when its element is absent.
        case.harness
            .update(|window, cx| window.focus(&old_focus, cx));
        case.harness.keystrokes("enter space");
        assert_eq!(case.app_actions.get(), 2, "{reason}");
        assert!(case.events.borrow().is_empty(), "{reason}");
        assert!(case.harness.node("keymap.recorder").is_none(), "{reason}");
    }
}

#[gpui::test]
fn unmounting_during_capture_does_not_restore_or_intercept(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    case.harness.click(PRIMARY);
    case.visible.set(false);
    case.harness.frame();
    case.harness.keystrokes("enter cmd-w");
    assert_eq!(case.app_actions.get(), 2);
    assert_eq!(
        case.events.borrow().as_slice(),
        [KeymapEditorEvent::RecordingCancelled {
            command_id: "open".into()
        }]
    );
    case.visible.set(true);
    case.harness.frame();
    assert!(case.harness.node("keymap.recorder").is_none());
    assert!(!case.harness.node(PRIMARY).expect("remounted field").focused);
}

#[gpui::test]
fn removed_field_handles_are_released_but_filtering_and_reordering_preserve_identity(
    cx: &mut TestAppContext,
) {
    let mut case = setup(cx);
    let old_focus = focus(&mut case, PRIMARY).downgrade();
    case.harness.update(|window, cx| {
        window.blur();
        case.editor
            .update(cx, |editor, cx| editor.set_query("Other", cx));
    });
    case.harness.frame();
    assert!(
        old_focus.upgrade().is_some(),
        "filtering preserves the handle"
    );
    let mut current = commands();
    current.reverse();
    case.harness.update(|_, cx| {
        case.editor.update(cx, |editor, cx| {
            editor.set_commands(current, cx);
            editor.set_query("", cx);
        })
    });
    assert_eq!(
        focus(&mut case, PRIMARY),
        old_focus.upgrade().expect("retained field handle")
    );
    case.harness.update(|window, cx| {
        window.blur();
        case.editor.update(cx, |editor, cx| {
            editor.set_commands(vec![commands()[1].clone()], cx)
        });
    });
    case.harness.frame();
    case.harness.frame();
    assert!(
        old_focus.upgrade().is_none(),
        "removed identities must not remain retained by the editor"
    );
}

#[gpui::test]
fn removed_binding_and_unbound_field_handles_are_pruned(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    let primary = focus(&mut case, PRIMARY).downgrade();
    let alternate = focus(&mut case, ALTERNATE).downgrade();
    let unbound = focus(&mut case, UNBOUND).downgrade();
    let mut current = commands();
    current[0] = current[0]
        .clone()
        .bindings([current[0].effective_bindings()[1].clone()]);
    current[2] = current[2]
        .clone()
        .bindings([KeymapBinding::new("ctrl-n", "ctrl-n")]);
    case.harness.update(|window, cx| {
        window.blur();
        case.editor
            .update(cx, |editor, cx| editor.set_commands(current, cx));
    });
    case.harness.frame();
    case.harness.frame();
    assert!(primary.upgrade().is_none());
    assert!(unbound.upgrade().is_none());
    assert_eq!(
        focus(&mut case, ALTERNATE),
        alternate.upgrade().expect("retained alternate handle")
    );
}

#[gpui::test]
fn host_focus_on_capture_is_not_overridden_by_restoration(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    let outside = focus(&mut case, "outside");
    let _subscription = case.harness.update(|window, cx| {
        let window = window.window_handle();
        cx.subscribe(&case.editor, move |_, event, cx| {
            if matches!(event, KeymapEditorEvent::ReplaceCaptured { .. }) {
                window
                    .update(cx, |_, window, cx| window.focus(&outside, cx))
                    .expect("test window is open");
            }
        })
    });
    case.harness.click(PRIMARY);
    case.harness.keystrokes("cmd-w");
    assert!(
        case.harness
            .node("outside")
            .expect("outside button")
            .focused
    );
    assert!(case.harness.node("keymap.recorder").is_none());
    case.harness.frame();
    assert!(
        case.harness
            .node("outside")
            .expect("outside button")
            .focused
    );
}

#[gpui::test]
fn activation_interception_is_scoped_to_its_window(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    focus(&mut case, PRIMARY);
    let mut other = Harness::new(cx, |_| {}, |_, _| div().into_any_element());
    other.update(|window, _| window.activate_window());
    other.frame();
    other.keystrokes("enter space");
    assert_eq!(case.app_actions.get(), 2);
    assert!(case.events.borrow().is_empty());
    assert!(case.harness.node("keymap.recorder").is_none());
}

#[gpui::test]
fn editor_and_subscriptions_are_released_after_unmount(cx: &mut TestAppContext) {
    let mut case = setup(cx);
    case.harness.click(PRIMARY);
    let weak = case.editor.downgrade();
    let Case {
        mut harness,
        editor,
        app_actions,
        ..
    } = case;
    harness.remount(|_, _| div().into_any_element());
    drop(editor);
    harness.frame();
    harness.frame();
    assert!(
        weak.upgrade().is_none(),
        "subscriptions must not retain the editor"
    );
    harness.keystrokes("enter space cmd-w");
    assert_eq!(app_actions.get(), 3);
}
