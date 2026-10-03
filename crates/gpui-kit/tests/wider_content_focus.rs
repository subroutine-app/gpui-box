use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{Context, Entity, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_kit::prelude::*;
use gpui_kit_testkit::harness::Harness;

#[derive(Clone, Copy, Debug)]
enum Entry {
    Initial,
    Pointer,
    Keyboard,
}

fn mount<T: Render>(
    cx: &mut TestAppContext,
    entry: Entry,
    build: impl Fn(&mut Window, &mut Context<T>) -> T + 'static,
) -> (Harness, Entity<T>) {
    let shown = Rc::new(Cell::new(matches!(entry, Entry::Initial)));
    let slot = Rc::new(RefCell::new(None::<Entity<T>>));
    let trigger = cx.update(|cx| cx.focus_handle());
    let mut harness = Harness::new(cx, gpui_kit::install, {
        let slot = slot.clone();
        let trigger = trigger.clone();
        move |window, cx| {
            let content = if shown.get() {
                slot.borrow_mut()
                    .get_or_insert_with(|| cx.new(|cx| build(window, cx)))
                    .clone()
                    .into_any_element()
            } else {
                let shown = shown.clone();
                Button::new("open")
                    .label("Open question")
                    .track_focus(&trigger)
                    .on_click(move |window, _| {
                        shown.set(true);
                        window.refresh();
                    })
                    .into_any_element()
            };
            div().w(px(600.0)).child(content).into_any_element()
        }
    });
    harness.update(|window, _| window.activate_window());
    match entry {
        Entry::Initial => {}
        Entry::Pointer => harness.click("open"),
        Entry::Keyboard => {
            harness.update(|window, cx| trigger.focus(window, cx));
            assert_focus(&mut harness, "open", false);
            harness.keystrokes("enter");
        }
    }
    harness.frame();
    let control = slot.borrow().clone().expect("question mounted");
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

#[gpui::test]
fn approval_entry_preserves_modality_and_decline_first_safety(cx: &mut TestAppContext) {
    for entry in [Entry::Initial, Entry::Pointer, Entry::Keyboard] {
        let (mut harness, prompt) = mount(cx, entry, |window, cx| {
            ApprovalPrompt::new("approval", "Read the requested file?", window, cx)
        });
        assert_focus(
            &mut harness,
            "approval.decline",
            matches!(entry, Entry::Keyboard),
        );
        let reports = Rc::new(RefCell::new(Vec::new()));
        let sink = reports.clone();
        harness.update(|_, cx| {
            cx.subscribe(&prompt, move |_, event: &ApprovalEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
        });
        harness.keystrokes("enter");
        assert_focus(&mut harness, "approval.decline", true);
        assert_eq!(reports.borrow().as_slice(), &[ApprovalEvent::Declined]);
        harness.keystrokes("tab");
        assert_focus(&mut harness, "approval.approve", true);
        harness.keystrokes("enter");
        assert_eq!(
            reports.borrow().as_slice(),
            &[
                ApprovalEvent::Declined,
                ApprovalEvent::Approved(ApprovalDecision::Once),
            ]
        );
        harness.click("approval.decline");
        assert_focus(&mut harness, "approval.decline", false);
        harness.keystrokes("a");
        assert_focus(&mut harness, "approval.decline", true);
    }
}

#[gpui::test]
fn clarification_entry_preserves_modality_without_changing_selection(cx: &mut TestAppContext) {
    for entry in [Entry::Initial, Entry::Pointer, Entry::Keyboard] {
        let (mut harness, panel) = mount(cx, entry, |window, cx| {
            ClarificationPanel::new("question", "Which files?", window, cx)
                .multiple()
                .options([
                    ClarificationOption::new("gone", "Deleted").unavailable("File was deleted"),
                    ClarificationOption::new("first", "First"),
                    ClarificationOption::new("second", "Second"),
                ])
        });
        assert_focus(
            &mut harness,
            "question.options.first",
            matches!(entry, Entry::Keyboard),
        );
        assert!(harness.node("question.answer").expect("answer").disabled);
        harness.click("question.options.gone");
        assert!(harness.update(|_, cx| panel.read(cx).chosen().is_empty()));
        harness.click("question.options.first");
        assert_focus(&mut harness, "question.options.first", false);
        assert!(
            harness
                .node("question.options.first")
                .expect("first")
                .selected
        );
        harness.keystrokes("up");
        assert_focus(&mut harness, "question.options.second", true);
        assert!(
            harness
                .node("question.options.first")
                .expect("first")
                .selected
        );
        harness.keystrokes("space");
        assert_focus(&mut harness, "question.options.second", true);
        assert!(
            harness
                .node("question.options.second")
                .expect("second")
                .selected
        );
        harness.update(|_, cx| {
            assert_eq!(panel.read(cx).chosen(), &["first", "second"]);
        });
        harness.click("question.options.second");
        assert_focus(&mut harness, "question.options.second", false);
        harness.update(|_, cx| assert_eq!(panel.read(cx).chosen(), &["first"]));
    }
}
