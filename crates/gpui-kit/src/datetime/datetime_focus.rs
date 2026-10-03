use super::*;
use crate::datetime::fixture::FixtureDateAdapter;
use gpui::{AppContext as _, Entity, StyleRefinement, TestAppContext};
use gpui_kit_testkit::harness::Harness;

fn day(date: u32) -> Day {
    FixtureDateAdapter::without_today().day(2024, 3, date)
}

fn day_id(day: Day) -> String {
    format!("calendar.day-{}", day.0)
}

fn cell_style(harness: &mut Harness, calendar: &Entity<Calendar>, day: Day) -> StyleRefinement {
    harness.update(|window, cx| {
        calendar.update(cx, |calendar, cx| {
            calendar
                .cell(
                    MonthCell::Day(day),
                    false,
                    false,
                    cx.layout_direction(),
                    window,
                    cx,
                )
                .expect("day cell")
                .style()
                .clone()
        })
    })
}

fn assert_ring(harness: &mut Harness, calendar: &Entity<Calendar>, day: Day, visible: bool) {
    let shadows = cell_style(harness, calendar, day)
        .box_shadow
        .unwrap_or_default();
    let expected = harness.update(|_, cx| cx.theme().focus_ring());
    if visible {
        assert_eq!(shadows.as_slice(), expected.as_slice());
    } else {
        assert!(shadows.is_empty(), "{day:?} must not carry a cursor ring");
    }
}

#[gpui::test]
fn calendar_cursor_ring_requires_own_keyboard_focus_without_changing_selection(
    cx: &mut TestAppContext,
) {
    let slot = Rc::new(RefCell::new(None::<Entity<Calendar>>));
    let build = slot.clone();
    let mut harness = Harness::new(cx, crate::install, move |window, cx| {
        build
            .borrow_mut()
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    Calendar::new(
                        "calendar",
                        Rc::new(FixtureDateAdapter::pinned(2024, 3, 14)),
                        window,
                        cx,
                    )
                    .selected([day(12)])
                    .range(DayRange::new(day(16), day(18)))
                })
            })
            .clone()
            .into_any_element()
    });
    harness.update(|window, _| window.activate_window());
    harness.frame();
    let calendar = slot.borrow().clone().expect("calendar mounted");
    let focus = harness.update(|_, cx| calendar.read(cx).focus_handle(cx));
    let picked = Rc::new(RefCell::new(Vec::new()));
    let sink = picked.clone();
    harness.update(|_, cx| {
        cx.subscribe(&calendar, move |_, event: &CalendarEvent, _| {
            if let CalendarEvent::Picked(day) = event {
                sink.borrow_mut().push(*day);
            }
        })
        .detach();
    });
    let presentation = [12, 13, 14, 16, 17, 18].map(|date| {
        let day = day(date);
        let style = cell_style(&mut harness, &calendar, day);
        let node = harness.node(&day_id(day)).expect("day");
        (day, style.background, node.checked)
    });
    assert_eq!(
        presentation.each_ref().map(|(_, _, checked)| *checked),
        [
            Some(true),
            Some(false),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
        ]
    );
    let today = harness.node("calendar.today").expect("today").text;

    harness.update(|window, cx| window.focus(&focus, cx));
    assert!(harness.node("calendar").expect("calendar").focused);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    harness.click(&day_id(day(13)));
    assert!(harness.node("calendar").expect("calendar").focused);
    assert_ring(&mut harness, &calendar, day(13), false);

    harness.keystrokes("right");
    assert!(harness.node("calendar").expect("calendar").focused);
    assert_eq!(
        harness.update(|_, cx| calendar.read(cx).cursor()),
        Some(day(14))
    );
    assert_ring(&mut harness, &calendar, day(14), true);
    assert_ring(&mut harness, &calendar, day(13), false);
    harness.keystrokes("enter");
    assert_eq!(picked.borrow().as_slice(), &[day(13), day(14)]);

    harness.update(|window, cx| window.focus_next(cx));
    assert!(
        harness
            .node("calendar.previous")
            .expect("previous month")
            .focused
    );
    assert!(harness.update(|window, cx| focus.contains_focused(window, cx)));
    assert!(harness.update(|window, _| window.focus_is_visible()));
    assert_ring(&mut harness, &calendar, day(14), false);

    harness.update(|window, cx| window.focus(&focus, cx));
    assert_ring(&mut harness, &calendar, day(14), true);
    harness.update(|window, _| window.blur());
    assert!(harness.update(|window, _| window.focus_is_visible()));
    assert_ring(&mut harness, &calendar, day(14), false);
    harness.update(|window, cx| window.focus(&focus, cx));
    assert_ring(&mut harness, &calendar, day(14), true);

    harness.click(&day_id(day(14)));
    assert!(harness.node("calendar").expect("calendar").focused);
    assert!(!harness.update(|window, _| window.focus_is_visible()));
    assert_ring(&mut harness, &calendar, day(14), false);
    harness.keystrokes("left");
    assert_eq!(
        harness.update(|_, cx| calendar.read(cx).cursor()),
        Some(day(13))
    );
    assert_ring(&mut harness, &calendar, day(13), true);

    for (day, background, checked) in presentation {
        assert_eq!(
            cell_style(&mut harness, &calendar, day).background,
            background
        );
        assert_eq!(harness.node(&day_id(day)).expect("day").checked, checked);
    }
    assert_eq!(harness.node("calendar.today").expect("today").text, today);
    assert_eq!(
        harness.update(|_, cx| calendar.read(cx).selection().to_vec()),
        [day(12)]
    );

    harness.update(|_, cx| calendar.update(cx, |calendar, cx| calendar.set_disabled(true, cx)));
    assert_ring(&mut harness, &calendar, day(13), false);
}
