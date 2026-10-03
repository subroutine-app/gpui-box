use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{Context, Entity, Modifiers, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_kit::prelude::*;
use gpui_kit_testkit::harness::Harness;

fn mount<T: Render>(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut Context<T>) -> T + 'static,
) -> (Harness, Entity<T>) {
    let slot = Rc::new(RefCell::new(None::<Entity<T>>));
    let held = slot.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |window, cx| {
        let control = held
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|cx| build(window, cx)))
            .clone();
        div().w(px(600.0)).child(control).into_any_element()
    });
    harness.update(|window, _| window.activate_window());
    harness.frame();
    let control = slot.borrow().clone().expect("control mounted");
    (harness, control)
}

fn assert_focus(harness: &mut Harness, id: &str, visible: bool) {
    assert!(harness.node(id).expect("focus target").focused, "{id}");
    assert_eq!(
        harness.update(|window, _| window.focus_is_visible()),
        visible,
        "{id} focus visibility"
    );
}

fn tab_to(harness: &mut Harness, id: &str) {
    for _ in 0..32 {
        harness.update(|window, cx| window.focus_next(cx));
        if harness.node(id).expect("tab target").focused {
            assert_focus(harness, id, true);
            return;
        }
    }
    panic!("{id} was not reachable by Tab");
}

#[gpui::test]
fn textarea_pointer_focus_keeps_editing_selection_and_disabled_refusal(cx: &mut TestAppContext) {
    let (mut harness, area) = mount(cx, |window, cx| {
        TextArea::new("area", window, cx).text("before")
    });
    harness.click("area");
    assert_focus(&mut harness, "area", false);
    harness.keystrokes("end x shift-left");
    assert_focus(&mut harness, "area", true);
    harness.update(|_, cx| {
        assert_eq!(area.read(cx).value().as_ref(), "beforex");
        assert_eq!(area.read(cx).selected_range(), 6..7);
    });
    tab_to(&mut harness, "area");
    harness.click("area");
    assert_focus(&mut harness, "area", false);
    harness.update(|_, cx| {
        area.update(cx, |area, cx| {
            area.set_invalid(true, cx);
            area.set_disabled(true, cx);
        });
    });
    harness.click("area");
    harness.keystrokes("z");
    let node = harness.node("area").expect("disabled area");
    assert!(node.disabled && node.invalid && !node.focused);
    assert_eq!(
        harness
            .update(|_, cx| area.read(cx).value().clone())
            .as_ref(),
        "beforex"
    );
}

#[gpui::test]
fn source_editor_pointer_focus_keeps_the_native_caret(cx: &mut TestAppContext) {
    let (mut harness, editor) = mount(cx, |window, cx| {
        Editor::new("source", "Source", "let value", window, cx)
    });
    harness.click("source.input");
    assert_focus(&mut harness, "source.input", false);
    harness.keystrokes("end x");
    assert_focus(&mut harness, "source.input", true);
    harness.update(|_, cx| {
        let area = editor.read(cx).text_area();
        assert_eq!(area.read(cx).value().as_ref(), "let valuex");
    });
    assert_focus(&mut harness, "source.input", true);
}

#[gpui::test]
fn rich_text_toolbar_preserves_the_activation_modality_and_selection(cx: &mut TestAppContext) {
    let (mut harness, editor) = mount(cx, |window, cx| {
        let session = cx.new(|_| {
            RichTextEditSession::new(RichTextDocument::empty("paragraph").expect("fixture"))
        });
        RichTextEditor::new("rich", session, || RichTextBlockId::new("next"), window, cx)
    });
    harness.click("rich");
    assert_focus(&mut harness, "rich", false);
    harness.keystrokes("a b c shift-left");
    let selection = harness.update(|_, cx| editor.read(cx).session().read(cx).selection().clone());
    harness.click("rich.toolbar.bold");
    assert_focus(&mut harness, "rich", false);
    harness.update(|_, cx| {
        assert_eq!(editor.read(cx).session().read(cx).selection(), &selection);
    });
    harness.click("rich.toolbar.link");
    assert_focus(&mut harness, "rich", false);
    tab_to(&mut harness, "rich.toolbar.bold");
    harness.keystrokes("space");
    assert_focus(&mut harness, "rich", true);
    harness.update(|_, cx| {
        assert_eq!(editor.read(cx).session().read(cx).selection(), &selection);
    });
    harness.click("rich");
    assert_focus(&mut harness, "rich", false);
}

#[gpui::test]
fn combobox_pointer_focus_stays_distinct_from_keyboard_editing(cx: &mut TestAppContext) {
    let (mut harness, combo) = mount(cx, |window, cx| {
        Combobox::new("combo", window, cx).options([SelectOption::new("a", "Alpha")])
    });
    harness.click("combo");
    assert_focus(&mut harness, "combo.query", false);
    assert!(harness.update(|_, cx| combo.read(cx).is_open()));
    harness.keystrokes("a");
    assert_focus(&mut harness, "combo.query", true);
    assert_eq!(
        harness
            .update(|_, cx| combo.read(cx).query_text(cx))
            .as_ref(),
        "a"
    );
    harness.keystrokes("escape");
    harness.update(|window, cx| combo.update(cx, |combo, cx| combo.toggle(window, cx)));
    assert_focus(&mut harness, "combo.query", true);
    harness.keystrokes("escape");
    harness.click("combo");
    assert_focus(&mut harness, "combo.query", false);
}

#[gpui::test]
fn multi_select_pointer_open_keeps_keyboard_navigation_available(cx: &mut TestAppContext) {
    let (mut harness, select) = mount(cx, |window, cx| {
        MultiSelect::new("multi", window, cx).options([
            SelectOption::new("a", "Alpha"),
            SelectOption::new("b", "Beta"),
        ])
    });
    harness.click("multi");
    assert_focus(&mut harness, "multi.query", false);
    assert!(harness.update(|_, cx| select.read(cx).is_open()));
    harness.keystrokes("escape");
    harness.update(|window, cx| select.update(cx, |select, cx| select.open(window, cx)));
    assert_focus(&mut harness, "multi.query", true);
    harness.keystrokes("down enter");
    assert!(harness.update(|_, cx| select.read(cx).selected_ids().is_empty()));
    harness.click("multi");
    assert_focus(&mut harness, "multi.query", false);
    harness.update(|_, cx| select.update(cx, |select, cx| select.set_disabled(true, cx)));
    harness.click("multi");
    assert!(!harness.update(|_, cx| select.read(cx).is_open()));
    assert!(!harness.node("multi.query").expect("disabled query").focused);
}

#[gpui::test]
fn multi_select_trigger_does_not_take_editable_query_focus(cx: &mut TestAppContext) {
    let (mut harness, select) = mount(cx, |window, cx| {
        MultiSelect::new("multi", window, cx).options([SelectOption::new("a", "Alpha")])
    });
    let bounds = harness.bounds("multi").expect("trigger bounds");
    let position = gpui::point(bounds.left() + px(2.0), bounds.center().y);
    harness
        .context()
        .simulate_click(position, Modifiers::none());
    harness.frame();
    assert_focus(&mut harness, "multi", false);
    assert!(!harness.node("multi.query").expect("query").focused);
    assert!(harness.update(|_, cx| select.read(cx).is_open()));

    harness.click("multi.query");
    assert_focus(&mut harness, "multi.query", false);
    harness.keystrokes("a");
    assert_focus(&mut harness, "multi.query", true);
    assert_eq!(
        harness
            .update(|_, cx| select.read(cx).query_input().read(cx).value().clone())
            .as_ref(),
        "a"
    );
}

#[gpui::test]
fn cascader_pointer_open_and_keyboard_navigation_keep_distinct_focus(cx: &mut TestAppContext) {
    let (mut harness, cascader) = mount(cx, |window, cx| {
        Cascader::new("cascade", window, cx).options([
            CascaderOption::new("a", "Alpha"),
            CascaderOption::new("b", "Beta"),
        ])
    });
    let selected = Rc::new(RefCell::new(Vec::new()));
    let sink = selected.clone();
    harness.update(|_, cx| {
        cx.subscribe(&cascader, move |_, event: &CascaderEvent, _| {
            if let CascaderEvent::Selected(id) = event {
                sink.borrow_mut().push(id.clone());
            }
        })
        .detach();
    });
    harness.click("cascade");
    assert_focus(&mut harness, "cascade", false);
    harness.keystrokes("down enter");
    assert_eq!(selected.borrow().as_slice(), &["b"]);
    harness.update(|window, cx| cascader.update(cx, |cascader, cx| cascader.open(window, cx)));
    assert_focus(&mut harness, "cascade", true);
    harness.click("cascade");
    assert_focus(&mut harness, "cascade", false);
}

#[gpui::test]
fn inline_edit_distinguishes_pointer_keyboard_and_host_opening(cx: &mut TestAppContext) {
    let editing = Rc::new(Cell::new(false));
    let state = editing.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
        let requested = state.clone();
        InlineEdit::new("inline", "before")
            .editing(state.get())
            .on_edit(move |_, cx| {
                requested.set(true);
                cx.refresh_windows();
            })
            .into_any_element()
    });
    harness.click("inline");
    assert_focus(&mut harness, "inline.field", false);
    harness.keystrokes("end x");
    assert_eq!(
        harness
            .node("inline.field")
            .expect("editor")
            .value
            .as_deref(),
        Some("beforex")
    );
    harness.update(|window, cx| {
        editing.set(false);
        window.blur();
        cx.refresh_windows();
    });
    harness.update(|window, cx| window.focus_next(cx));
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.keystrokes("enter");
    assert_focus(&mut harness, "inline.field", true);
    harness.update(|window, cx| {
        editing.set(false);
        window.blur();
        cx.refresh_windows();
    });
    let visible = harness.update(|window, _| window.focus_is_visible());
    harness.update(|_, cx| {
        editing.set(true);
        cx.refresh_windows();
    });
    assert_focus(&mut harness, "inline.field", visible);
}

#[gpui::test]
fn recorder_pointer_start_preserves_focus_without_suppressing_recording(cx: &mut TestAppContext) {
    let (mut harness, recorder) = mount(cx, |window, cx| {
        KeybindingRecorder::new("recorder", window, cx)
    });
    harness.click("recorder");
    assert_focus(&mut harness, "recorder", false);
    assert!(harness.node("recorder").expect("recording state").busy);
    harness.update(|_, cx| recorder.update(cx, |recorder, cx| recorder.cancel(cx)));
    assert_focus(&mut harness, "recorder", false);
    harness.update(|window, cx| recorder.update(cx, |recorder, cx| recorder.start(window, cx)));
    assert_focus(&mut harness, "recorder", false);
    harness.update(|_, cx| recorder.update(cx, |recorder, cx| recorder.cancel(cx)));
    tab_to(&mut harness, "recorder");
    harness.keystrokes("enter");
    assert_focus(&mut harness, "recorder", true);
    harness.keystrokes("escape");
    assert!(!harness.node("recorder").expect("cancelled").busy);
}

#[gpui::test]
fn keymap_pointer_activation_and_keyboard_restoration_remain_distinct(cx: &mut TestAppContext) {
    let (mut harness, _) = mount(cx, |window, cx| {
        KeymapEditor::new("keymap", window, cx)
            .commands([KeymapCommand::new("open", "Open")
                .bindings([KeymapBinding::new("ctrl-p", "ctrl-p")])])
    });
    let field = "keymap.open.binding.ctrl-p.edit";
    harness.click(field);
    assert_focus(&mut harness, "keymap.recorder", false);
    harness.keystrokes("escape");
    assert_focus(&mut harness, field, true);
    harness.keystrokes("enter");
    assert_focus(&mut harness, "keymap.recorder", true);
}
