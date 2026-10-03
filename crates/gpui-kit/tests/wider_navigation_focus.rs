use std::cell::{Cell as Flag, RefCell};
use std::rc::Rc;

use gpui::{Modifiers, MouseButton, TestAppContext, div, prelude::*, px};
use gpui_kit::interaction::range::{RangeEvent, RangeIntent, RangeTarget};
use gpui_kit::navigation::{NavHistory, NavStack};
use gpui_kit::prelude::*;
use gpui_kit_testkit::harness::Harness;

#[gpui::test]
fn sidebar_flyout_entry_preserves_pointer_or_keyboard_origin(cx: &mut TestAppContext) {
    for (active, selectable, target) in [
        ("child", true, "rail.child"),
        ("elsewhere", true, "rail.branch.destination"),
        ("child", false, "rail.branch.flyout"),
    ] {
        let mut harness = Harness::new(cx, gpui_kit::install, move |_, _| {
            Sidebar::new("rail")
                .section(
                    SidebarSection::new("places").item(
                        SidebarItem::new("branch", "Branch")
                            .children([SidebarItem::new("child", "Child")]),
                    ),
                )
                .collapsed(true)
                .active(active)
                .when(selectable, |sidebar| sidebar.on_select(|_, _, _| {}))
                .into_any_element()
        });
        harness.click("rail.branch");
        assert!(harness.node(target).expect("flyout focus target").focused);
        assert!(!harness.update(|window, _| window.focus_is_visible()));
        harness.keystrokes("escape");
        assert!(harness.node("rail.branch").expect("trigger").focused);
        harness.keystrokes("enter");
        assert!(harness.node(target).expect("flyout focus target").focused);
        assert!(harness.update(|window, _| window.focus_is_visible()));
    }
}

#[gpui::test]
fn grid_editing_preserves_pointer_focus_and_keyboard_edit_navigation(cx: &mut TestAppContext) {
    let editing = Rc::new(RefCell::new(None::<EditingCell>));
    let edits = Rc::new(RefCell::new(Vec::new()));
    let mut harness = Harness::new(cx, gpui_kit::install, {
        let editing = editing.clone();
        let edits = edits.clone();
        move |_, _| {
            let request = editing.clone();
            let accepted = editing.clone();
            let edits = edits.clone();
            div()
                .w(px(600.))
                .child(
                    DataGrid::new("grid", 1, |_, _, _| {
                        GridRow::new("row")
                            .cell("name", Cell::new("Name").text("Name"))
                            .cell("value", Cell::new("Value").text("Value"))
                    })
                    .columns([
                        GridColumn::new("name", "Name").editable(true),
                        GridColumn::new("value", "Value").editable(true),
                    ])
                    .selection_mode(SelectionMode::Single)
                    .selected(["row"])
                    .editing(editing.borrow().clone())
                    .on_edit_request(move |row, column, window, _| {
                        *request.borrow_mut() = Some(EditingCell::new(row, column, ""));
                        window.refresh();
                    })
                    .on_edit(move |intent, window, _| {
                        edits.borrow_mut().push(intent.clone());
                        *accepted.borrow_mut() = intent
                            .next
                            .as_ref()
                            .map(|(row, column)| EditingCell::new(row.clone(), column.clone(), ""));
                        window.refresh();
                    }),
                )
                .into_any_element()
        }
    });

    let position = harness.point_in("grid.row.name");
    harness.context().simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    harness.context().simulate_event(gpui::MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    harness.context().run_until_parked();
    assert!(harness.node("grid.edit").expect("pointer editor").focused);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    assert!(harness.node("grid.row").expect("selected row").selected);

    harness.keystrokes("x");
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.keystrokes("tab");
    assert_eq!(edits.borrow()[0].value.as_ref(), "x");
    assert_eq!(edits.borrow()[0].next, Some(("row".into(), "value".into())));
    assert!(harness.node("grid.edit").expect("next editor").focused);
    assert!(harness.update(|window, _| window.focus_is_visible()));

    harness.keystrokes("escape");
    assert!(harness.node("grid.edit").is_none());
    harness.click("grid.row.name");
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.keystrokes("enter");
    assert!(harness.node("grid.edit").expect("keyboard editor").focused);
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.click("grid.edit");
    assert!(harness.node("grid.edit").expect("clicked editor").focused);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.update(|window, _| window.blur());
    assert!(!harness.node("grid.edit").expect("unfocused editor").focused);
    assert!(harness.node("grid.row").expect("selected row").selected);
}

#[gpui::test]
fn sidebar_mode_changes_preserve_focus_origin_and_active_selection(cx: &mut TestAppContext) {
    let collapsed = Rc::new(Flag::new(false));
    let mut harness = Harness::new(cx, gpui_kit::install, {
        let collapsed = collapsed.clone();
        move |_, _| {
            Sidebar::new("rail")
                .section(
                    SidebarSection::new("places").items([
                        SidebarItem::new("branch", "Branch")
                            .children([SidebarItem::new("child", "Child")]),
                        SidebarItem::new("last", "Last"),
                    ]),
                )
                .active("child")
                .collapsed(collapsed.get())
                .on_select(|_, _, _| {})
                .into_any_element()
        }
    });

    harness.click("rail.child");
    assert!(harness.node("rail.child").expect("active child").selected);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    collapsed.set(true);
    harness.frame();
    assert!(
        harness
            .node("rail.branch")
            .expect("collapsed branch")
            .focused
    );
    assert!(!harness.update(|window, _| window.focus_is_visible()));

    collapsed.set(false);
    harness.frame();
    harness.click("rail.last");
    harness.keystrokes("up");
    assert!(harness.node("rail.child").expect("keyboard child").focused);
    assert!(harness.update(|window, _| window.focus_is_visible()));
    collapsed.set(true);
    harness.frame();
    assert!(
        harness
            .node("rail.branch")
            .expect("collapsed branch")
            .focused
    );
    assert!(harness.update(|window, _| window.focus_is_visible()));
    collapsed.set(false);
    harness.frame();
    assert!(harness.node("rail.child").expect("active child").selected);
}

#[gpui::test]
fn nav_stack_restoration_preserves_focus_origin_even_when_saved_child_disappears(
    cx: &mut TestAppContext,
) {
    let history = Rc::new(RefCell::new(NavHistory::new("first")));
    let show_child = Rc::new(Flag::new(true));
    let first = cx.update(|cx| cx.focus_handle());
    let child = cx.update(|cx| cx.focus_handle());
    let second = cx.update(|cx| cx.focus_handle());
    let mut harness = Harness::new(cx, gpui_kit::install, {
        let history = history.clone();
        let show_child = show_child.clone();
        let first = first.clone();
        let child = child.clone();
        let second = second.clone();
        move |_, _| {
            let history = history.borrow();
            let is_first = history.current().as_ref() == "first";
            NavStack::new(
                "nav",
                &history,
                "Active page",
                if is_first {
                    first.clone()
                } else {
                    second.clone()
                },
                div().when(is_first && show_child.get(), |content| {
                    content.child(div().id("child").track_focus(&child).child("First control"))
                }),
            )
            .into_any_element()
        }
    });
    harness.update(|window, cx| {
        cx.set_reduce_motion(true);
        window.focus_from_pointer(&child, cx);
    });
    assert!(history.borrow_mut().push("second"));
    harness.frame();
    assert!(harness.update(|window, _| second.is_focused(window)));
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    assert!(history.borrow_mut().pop());
    harness.frame();
    assert!(harness.update(|window, _| child.is_focused(window)));
    assert!(!harness.update(|window, _| window.focus_is_visible()));

    assert!(history.borrow_mut().forward());
    harness.frame();
    show_child.set(false);
    assert!(history.borrow_mut().pop());
    harness.frame();
    harness.frame();
    assert!(harness.update(|window, _| first.is_focused(window)));
    assert!(!harness.update(|window, _| window.focus_is_visible()));

    harness.update(|window, cx| first.focus(window, cx));
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.keystrokes("tab");
    assert!(harness.update(|window, _| window.focus_is_visible()));
    assert!(history.borrow_mut().forward());
    harness.frame();
    assert!(harness.update(|window, _| second.is_focused(window)));
    assert!(harness.update(|window, _| window.focus_is_visible()));
    assert!(history.borrow_mut().pop());
    harness.frame();
    assert!(harness.update(|window, _| first.is_focused(window)));
    assert!(harness.update(|window, _| window.focus_is_visible()));
}

#[gpui::test]
fn trace_range_pointer_capture_keeps_focus_and_keyboard_adjustment(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut harness = Harness::new(cx, gpui_kit::install, {
        let events = events.clone();
        move |_, _| {
            let events = events.clone();
            div()
                .w(px(600.))
                .child(
                    TraceView::new("trace", "Range fixture")
                        .spans([TraceSpan::new("span", "Span", 0., 1.)])
                        .time_viewport([0., 1000.])
                        .expect("domain")
                        .selected_time(Some([200., 450.]))
                        .expect("selection")
                        .on_time_selection(move |event, _, _| events.borrow_mut().push(event)),
                )
                .into_any_element()
        }
    });
    harness.drag_start("trace.time-selection.start");
    assert!(harness.update(|window, _| window.captured_hitbox().is_some()));
    assert!(
        harness
            .node("trace.time-selection.start")
            .expect("start")
            .focused
    );
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.drop_here();
    assert!(harness.update(|window, _| window.captured_hitbox().is_none()));
    assert!(matches!(
        events.borrow().last(),
        Some(RangeEvent::Commit { .. })
    ));

    harness.update(|window, cx| window.focus_prev(cx));
    assert!(
        harness
            .node("trace.time-selection.track")
            .expect("track")
            .focused
    );
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.update(|window, cx| window.focus_next(cx));
    assert!(
        harness
            .node("trace.time-selection.start")
            .expect("start")
            .focused
    );
    harness.update(|window, cx| window.focus_next(cx));
    assert!(
        harness
            .node("trace.time-selection.end")
            .expect("end")
            .focused
    );
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.click("trace.time-selection.end");
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    events.borrow_mut().clear();
    harness.keystrokes("right");
    assert!(harness.update(|window, _| window.focus_is_visible()));
    assert_eq!(
        events.borrow().last(),
        Some(&RangeEvent::Commit {
            intent: RangeIntent::Resize(RangeTarget::End),
            value: [200., 460.],
        })
    );
    harness.click("trace.time-selection.end");
    assert!(
        harness
            .node("trace.time-selection.end")
            .expect("end")
            .focused
    );
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    assert_eq!(
        harness
            .node("trace.time-selection.end")
            .expect("end")
            .value
            .as_deref(),
        Some("450")
    );
}
