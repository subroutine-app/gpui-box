use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, TestAppContext, prelude::*};
use gpui_kit::prelude::*;
use gpui_kit_testkit::harness::Harness;

#[gpui::test]
fn clearing_search_preserves_pointer_or_keyboard_focus_visibility(cx: &mut TestAppContext) {
    let slot = Rc::new(RefCell::new(None::<Entity<SearchInput>>));
    let build = slot.clone();
    let mut harness = Harness::new(cx, gpui_kit::install, move |window, cx| {
        build
            .borrow_mut()
            .get_or_insert_with(|| {
                cx.new(|cx| SearchInput::new("search", window, cx).name("Search settings"))
            })
            .clone()
            .into_any_element()
    });
    let search = slot.borrow().clone().expect("search mounted");

    harness.click("search.query");
    harness.keystrokes("a");
    assert!(harness.node("search.query").expect("input").focused);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.click("search.clear");
    assert!(harness.node("search.query").expect("cleared input").focused);
    assert!(harness.update(|_, cx| search.read(cx).value(cx)).is_empty());
    assert!(harness.node("search.clear").is_none());
    assert!(!harness.update(|window, _| window.focus_is_visible()));

    harness.keystrokes("b");
    assert_eq!(
        harness.update(|_, cx| search.read(cx).value(cx)).as_ref(),
        "b",
        "pointer clearing restores editing focus"
    );
    harness.update(|window, cx| window.focus_next(cx));
    assert!(
        harness
            .node("search.clear")
            .expect("clear tab stop")
            .focused
    );
    assert!(harness.update(|window, _| window.focus_is_visible()));
    harness.keystrokes("space");
    assert!(
        harness
            .node("search.query")
            .expect("keyboard-cleared input")
            .focused
    );
    assert!(harness.update(|_, cx| search.read(cx).value(cx)).is_empty());
    assert!(harness.node("search.clear").is_none());
    assert!(harness.update(|window, _| window.focus_is_visible()));

    harness.keystrokes("c");
    harness.click("search.clear");
    assert!(harness.node("search.query").expect("cleared input").focused);
    assert!(harness.update(|_, cx| search.read(cx).value(cx)).is_empty());
    assert!(!harness.update(|window, _| window.focus_is_visible()));
}
