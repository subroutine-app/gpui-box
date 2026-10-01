//! A filterable list of commands: the keyboard surface of an application.
//!
//! The palette is the one place a typist expects to find everything the
//! application can do, so it never hides a command it was given. A command the
//! host has marked unavailable is shown as unavailable, with the host's own
//! reason, and a query that matches nothing says so about that query instead
//! of drawing an empty list that looks like an application with no commands.

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_theme::{ActiveTheme, Space, TypeScale};

use crate::controls::input::{TextInput, TextInputEvent};
use crate::display::empty::{EmptyKind, EmptyState};
use crate::foundation::slot::{self, Slots, Slotted};
use crate::foundation::{Ident, Pressable, StyledExt};

use crate::layout::scroll::scroll_handle;
use crate::motion;
use crate::overlay::kbd::Kbd;
use crate::overlay::layer::{OverlaySurface, surface};
use crate::overlay::popover::{self, MenuKey};
use crate::strings::{ActiveSearch, ActiveStrings, SearchMatcher, StringKey};

/// How wide the palette is. The value occurs once, so it stays here rather
/// than in the token document.
const PALETTE_WIDTH: f32 = 480.0;

/// One thing the application can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    id: SharedString,
    label: SharedString,
    section: Option<SharedString>,
    shortcut: Option<SharedString>,
    /// Why the host will not run this now. `None` means it will.
    unavailable: Option<SharedString>,
}

impl Command {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            section: None,
            shortcut: None,
            unavailable: None,
        }
    }

    /// The group this command belongs to. Sections stay contiguous, ordered by
    /// the best match inside them.
    pub fn section(mut self, section: impl Into<SharedString>) -> Self {
        self.section = Some(section.into());
        self
    }

    /// The keystroke that runs it without the palette.
    pub fn shortcut(mut self, keystroke: impl Into<SharedString>) -> Self {
        self.shortcut = Some(keystroke.into());
        self
    }

    /// Marks the command as one the host will not run now, without adding
    /// explanatory copy to the row.
    pub fn disabled(mut self) -> Self {
        self.unavailable = Some(SharedString::default());
        self
    }

    /// Marks the command as one the host will not run now, in the host's own
    /// words. It is still listed: hiding a command a typist knows exists is a
    /// lie about the application.
    pub fn unavailable(mut self, reason: impl Into<SharedString>) -> Self {
        self.unavailable = Some(reason.into());
        self
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    pub fn reason(&self) -> Option<&SharedString> {
        self.unavailable
            .as_ref()
            .filter(|reason| !reason.is_empty())
    }

    pub fn is_available(&self) -> bool {
        self.unavailable.is_none()
    }
}

/// What the palette reports. The owner decides what any of it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteEvent {
    QueryChanged(SharedString),
    /// The highlighted command was taken.
    Invoked(SharedString),
    /// The palette was waved away with escape. The host owns whether it stays
    /// on screen, so the palette only reports the intent.
    Dismissed,
}

impl EventEmitter<CommandPaletteEvent> for CommandPalette {}

/// Orders the commands that answer `query`, keeping each section contiguous.
///
/// A section sits where its best match does, so the closest answer is still
/// the first row while the grouping stays readable.
#[allow(dead_code)]
fn order_matches(commands: &[Command], query: &str) -> Vec<usize> {
    order_matches_with(commands, query, &crate::strings::EnglishSearch)
}

fn order_matches_with(
    commands: &[Command],
    query: &str,
    matcher: &dyn SearchMatcher,
) -> Vec<usize> {
    let ranked: Vec<(usize, usize)> = commands
        .iter()
        .enumerate()
        .filter_map(|(index, command)| {
            matcher
                .rank(query, command.label.as_ref())
                .map(|rank| (rank, index))
        })
        .collect();

    let section_of = |index: usize| commands[index].section.clone().unwrap_or_default();
    let mut sections: Vec<(SharedString, usize, usize)> = Vec::new();
    for &(rank, index) in &ranked {
        let name = section_of(index);
        match sections.iter_mut().find(|(known, _, _)| *known == name) {
            Some(section) => section.1 = section.1.min(rank),
            None => sections.push((name, rank, index)),
        }
    }
    sections.sort_by_key(|(_, rank, first)| (*rank, *first));

    let mut ordered = Vec::with_capacity(ranked.len());
    for (name, _, _) in &sections {
        let mut group: Vec<(usize, usize)> = ranked
            .iter()
            .copied()
            .filter(|(_, index)| section_of(*index) == *name)
            .collect();
        group.sort_by_key(|&(rank, index)| (rank, index));
        ordered.extend(group.into_iter().map(|(_, index)| index));
    }
    ordered
}

/// A query field over a list of commands.
///
/// The palette owns the query and highlighted command; the command list,
/// focus policy and whether the palette is on screen at all belong to the host.
///
/// # Slots
///
/// - [`slot::EMPTY`] replaces the no-results content.
/// - [`slot::HEADER_EXTRA`] places compact leading controls beside the query in
///   the same search row. The controls keep their intrinsic width; the query
///   takes the remaining width, separated by [`Space::Xs`]. Without this slot,
///   the original query wrapper and its geometry are unchanged.
/// - [`slot::FOOTER`] places content below the results (including the empty
///   state), outside their scroll area and inside the palette's sole surface.
///   The palette owns the footer's [`Space::Xs`] padding and subdued top border:
///   the theme's hairline width and divider colour at muted opacity. The caller
///   supplies compact contents, not another surface, border or outer padding.
///
/// Slot contents must fit the palette's content width. Callers own their stable
/// semantic ids, focus handles and handlers, including any back action and
/// restoring query focus via [`Self::focus_query`]. No navigation state or
/// additional palette events are introduced by either slot. The optional slot
/// frames publish `header-extra` and `footer` child ids without copying content.
pub struct CommandPalette {
    ident: Ident,
    focus_handle: FocusHandle,
    query: Entity<TextInput>,
    commands: Vec<Command>,
    /// The highlighted command, by identity, so filtering does not move the
    /// highlight onto whatever happens to sit at the same position.
    active: Option<SharedString>,
    reveal_active: bool,
    slots: Slots,
    /// Held so the query subscription lives as long as the palette does.
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for CommandPalette {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandPalette")
            .field("ident", &self.ident)
            .field("commands", &self.commands.len())
            .field("active", &self.active)
            .finish()
    }
}

impl CommandPalette {
    pub fn new(ident: impl Into<Ident>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ident = ident.into();
        let query = cx.new(|cx| {
            TextInput::new(ident.child("query"), window, cx)
                .placeholder(cx.strings().text(StringKey::PalettePlaceholder))
        });
        let subscription = cx.subscribe(&query, |palette, _query, event, cx| match event {
            TextInputEvent::Change(text) => {
                // A new query is a new list, so the highlight goes back to the
                // best answer rather than staying on a row that may be gone.
                palette.active = None;
                palette.reveal_active = true;
                cx.emit(CommandPaletteEvent::QueryChanged(text.clone()));
                cx.notify();
            }
            TextInputEvent::Submit => palette.invoke(cx),
            TextInputEvent::Cancel => cx.emit(CommandPaletteEvent::Dismissed),
            _ => {}
        });

        Self {
            ident,
            focus_handle: cx.focus_handle(),
            query,
            commands: Vec::new(),
            active: None,
            reveal_active: false,
            slots: Slots::default(),
            _subscriptions: vec![subscription],
        }
    }

    pub fn commands(mut self, commands: impl IntoIterator<Item = Command>) -> Self {
        self.commands = commands.into_iter().collect();
        self
    }

    pub fn set_commands(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        self.commands = commands;
        self.active = None;
        self.reveal_active = true;
        cx.notify();
    }

    pub fn query(&self, cx: &App) -> SharedString {
        self.query.read(cx).value().clone()
    }

    /// Replaces the query from the host side, for example when the palette is
    /// opened with something already typed.
    pub fn set_query(&mut self, query: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.query
            .update(cx, |input, cx| input.set_value(query, cx));
    }

    pub fn query_input(&self) -> &Entity<TextInput> {
        &self.query
    }

    /// Moves the keyboard into the query field.
    pub fn focus_query(&self, window: &mut Window, cx: &mut App) {
        self.query.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// The command the keyboard is on, or `None` when nothing can be taken.
    pub fn active_id(&self, cx: &App) -> Option<SharedString> {
        let ordered = self.ordered(cx);
        self.resolved(&ordered)
            .map(|index| self.commands[index].id.clone())
    }

    fn ordered(&self, cx: &App) -> Vec<usize> {
        order_matches_with(
            &self.commands,
            self.query.read(cx).value().as_ref(),
            cx.search().as_ref(),
        )
    }

    /// The command the highlight sits on: the one the typist put it on while
    /// it still answers the query, or the best answer that can be taken.
    fn resolved(&self, ordered: &[usize]) -> Option<usize> {
        if let Some(active) = &self.active
            && let Some(index) = ordered
                .iter()
                .copied()
                .find(|index| &self.commands[*index].id == active)
            && self.commands[index].is_available()
        {
            return Some(index);
        }
        ordered
            .iter()
            .copied()
            .find(|index| self.commands[*index].is_available())
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let ordered = self.ordered(cx);
        let choosable: Vec<usize> = ordered
            .iter()
            .copied()
            .filter(|index| self.commands[*index].is_available())
            .collect();
        if choosable.is_empty() {
            return;
        }
        let current = self
            .resolved(&ordered)
            .and_then(|index| choosable.iter().position(|choice| *choice == index));
        let Some(next) = popover::step(current, choosable.len(), delta) else {
            return;
        };
        self.active = Some(self.commands[choosable[next]].id.clone());
        self.reveal_active = true;
        cx.notify();
    }

    /// Reports the highlighted command. An unavailable command is never
    /// invoked, and no reachable row installs a handler for one.
    fn invoke(&mut self, cx: &mut Context<Self>) {
        let ordered = self.ordered(cx);
        let Some(index) = self.resolved(&ordered) else {
            return;
        };
        cx.emit(CommandPaletteEvent::Invoked(
            self.commands[index].id.clone(),
        ));
    }

    fn choose(&mut self, id: SharedString, cx: &mut Context<Self>) {
        self.active = Some(id.clone());
        cx.emit(CommandPaletteEvent::Invoked(id));
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = popover::classify_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.control,
        );
        match key {
            MenuKey::Down => {
                self.step(1, cx);
                cx.stop_propagation();
            }
            MenuKey::Up => {
                self.step(-1, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn results(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = cx.theme().clone();
        let ordered = self.ordered(cx);
        let highlighted = self.resolved(&ordered);
        let results_id = self.ident.child("results").semantic_id();
        let mut section: Option<SharedString> = None;
        let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(ordered.len());
        let mut highlighted_row = None;
        let count = ordered.len();

        for (position, index) in ordered.into_iter().enumerate() {
            let command = &self.commands[index];
            if command.section != section {
                section = command.section.clone();
                if let Some(name) = section.clone() {
                    rows.push(
                        popover::heading(&theme, name.as_ref())
                            .semantic_in(
                                cx,
                                NodeSpec::new(
                                    self.ident
                                        .child("section")
                                        .child(name.as_ref())
                                        .semantic_id(),
                                    Role::Heading,
                                )
                                .parent(results_id.clone())
                                .level(2)
                                .text(name),
                            )
                            .into_any_element(),
                    );
                }
            }

            let row_ident = self.ident.child(command.id.as_ref());
            let active = highlighted == Some(index);
            if active {
                highlighted_row = Some(rows.len());
            }
            let available = command.is_available();
            let id = command.id.clone();
            let mut spec = NodeSpec::new(row_ident.semantic_id(), Role::MenuItem)
                .parent(results_id.clone())
                .text(command.label.clone())
                .disabled(!available)
                .hovered(active);
            if let Some(reason) = command.reason() {
                spec = spec.value(reason.clone());
            }

            let row = popover::menu_row(&theme, false, active)
                .id(row_ident.element_id())
                .when(available, |element| element.cursor_pointer().pressable(cx))
                .when(!available, |element| {
                    element.opacity(theme.opacity.disabled)
                })
                .child(div().flex_1().child(command.label.clone()))
                .children(command.reason().map(|reason| {
                    div()
                        .type_scale(&theme, TypeScale::Caption)
                        .text_color(theme.colors.warning)
                        .child(reason.clone())
                }))
                .children(
                    command
                        .shortcut
                        .clone()
                        .map(|keystroke| Kbd::new(keystroke).into_any_element()),
                )
                .when(available, |element| {
                    element.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |palette, _, _, cx| {
                            palette.choose(id.clone(), cx);
                        }),
                    )
                })
                .semantic_in(cx, spec);

            rows.push(
                motion::row_in(
                    row_ident.child("in").element_id(),
                    &theme,
                    position,
                    count,
                    row,
                )
                .into_any_element(),
            );
        }

        if self.reveal_active {
            if let Some(row) = highlighted_row {
                scroll_handle(&self.ident.child("results"), window, cx).scroll_to_item(row);
            }
            self.reveal_active = false;
        }

        rows
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Slotted for CommandPalette {
    const SLOTS: &'static [&'static str] = &[slot::EMPTY, slot::HEADER_EXTRA, slot::FOOTER];

    fn slots_mut(&mut self) -> &mut Slots {
        &mut self.slots
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let query = self.query.read(cx).value().clone();
        let rows = self.results(window, cx);
        let empty = rows.is_empty();
        let results_id = self.ident.child("results").semantic_id();

        let body = if empty {
            // A query that answered nothing is a fact about the query, not an
            // application without commands, so it says which query it was.
            self.slots.or_else(slot::EMPTY, window, cx, |_, cx| {
                EmptyState::new(
                    self.ident.child("empty"),
                    cx.strings().format(StringKey::PaletteNoMatch, &[&query]),
                )
                .kind(EmptyKind::Empty)
                .into_any_element()
            })
        } else {
            let results_ident = self.ident.child("results");
            let scroll = scroll_handle(&results_ident, window, cx);
            popover::menu_body(
                &results_ident.child("fade"),
                &scroll,
                div()
                    .id(results_ident.element_id())
                    .flex()
                    .flex_col()
                    .max_h(px(theme.measures.menu_max_height))
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .children(rows)
                    .semantic_in(cx, NodeSpec::new(results_id, Role::Menu)),
            )
            .into_any_element()
        };

        let search = match self.slots.render(slot::HEADER_EXTRA, window, cx) {
            Some(extra) => div()
                .row()
                .p_token(&theme, Space::Xs)
                .gap_token(&theme, Space::Xs)
                .child(
                    div().flex_none().child(extra).semantic_in(
                        cx,
                        NodeSpec::new(self.ident.child("header-extra").semantic_id(), Role::Group)
                            .parent(self.ident.semantic_id()),
                    ),
                )
                .child(div().flex_1().min_w(px(0.0)).child(self.query.clone())),
            None => div().p_token(&theme, Space::Xs).child(self.query.clone()),
        };
        let footer = self.slots.render(slot::FOOTER, window, cx).map(|footer| {
            div()
                .flex_none()
                .min_w(px(0.0))
                .border_t(px(theme.borders.hairline))
                .border_color(theme.colors.divider.opacity(theme.opacity.muted))
                .p_token(&theme, Space::Xs)
                .child(footer)
                .semantic_in(
                    cx,
                    NodeSpec::new(self.ident.child("footer").semantic_id(), Role::Group)
                        .parent(self.ident.semantic_id()),
                )
        });

        let mut spec = NodeSpec::new(self.ident.semantic_id(), Role::Group).value(query);
        if empty {
            spec = spec.description(cx.strings().text(StringKey::PaletteEmptyDetail));
        }

        surface(self.ident.clone(), &theme, OverlaySurface::MODAL)
            .w(px(PALETTE_WIDTH))
            .p_token(&theme, Space::Xs)
            .gap_token(&theme, Space::Xs)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(search)
            .child(body)
            .children(footer)
            .semantic_in(cx, spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::button::IconButton;
    use crate::foundation::Sizable;
    use gpui::TestAppContext;
    use gpui_kit_assets::Icon;
    use gpui_kit_semantics::Snapshot;
    use gpui_kit_testkit::harness::Harness;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn commands() -> Vec<Command> {
        vec![
            Command::new("editor.split", "Split editor").section("Editor"),
            Command::new("workspace.save", "Save workspace").section("Workspace"),
            Command::new("editor.save", "Save file")
                .section("Editor")
                .shortcut("cmd-s"),
            Command::new("workspace.publish", "Publish workspace")
                .section("Workspace")
                .unavailable("Approval is required"),
        ]
    }

    #[test]
    fn an_empty_query_lists_everything_grouped_by_section() {
        assert_eq!(order_matches(&commands(), ""), vec![0, 2, 1, 3]);
    }

    #[test]
    fn a_section_sits_where_its_best_match_does() {
        // "Save workspace" is a prefix match, so its section leads even though
        // the editor section was declared first.
        assert_eq!(order_matches(&commands(), "save"), vec![1, 2]);
    }

    #[test]
    fn a_command_the_host_refused_is_still_listed() {
        let ordered = order_matches(&commands(), "publish");
        assert_eq!(ordered, vec![3]);
        assert!(!commands()[3].is_available());
    }

    #[test]
    fn a_disabled_command_has_no_warning_copy() {
        let command = Command::new("workspace.publish", "Publish workspace").disabled();

        assert!(!command.is_available());
        assert_eq!(command.reason(), None);
    }

    #[gpui::test]
    fn arrow_navigation_reveals_the_active_command(cx: &mut TestAppContext) {
        let slot = Rc::new(RefCell::new(None));
        let build = slot.clone();
        let mut harness = Harness::new(
            cx,
            |cx| {
                crate::install(cx);
                cx.set_reduce_motion(true);
            },
            move |window, cx| {
                let palette =
                    build
                        .borrow_mut()
                        .get_or_insert_with(|| {
                            cx.new(|cx| {
                                CommandPalette::new("test.palette", window, cx).commands(
                                    (0..20).map(|index| {
                                        Command::new(
                                            format!("command-{index}"),
                                            format!("Command {index}"),
                                        )
                                        .section(if index < 10 { "First" } else { "Second" })
                                    }),
                                )
                            })
                        })
                        .clone();
                palette.update(cx, |palette, cx| palette.focus_query(window, cx));
                palette.into_any_element()
            },
        );
        let palette = slot.borrow().clone().expect("palette mounted");

        harness.keystrokes(
            "down down down down down down down down down down down down down down down",
        );
        harness.frame();

        harness.update(|window, cx| {
            let palette = palette.read(cx);
            assert_eq!(palette.active_id(cx).as_deref(), Some("command-15"));
            assert!(
                scroll_handle(&palette.ident.child("results"), window, cx)
                    .offset()
                    .y
                    < px(0.0)
            );
        });
    }

    struct PickerHarness {
        harness: Harness,
        palette: Entity<CommandPalette>,
        back_presses: Rc<Cell<usize>>,
        events: Rc<RefCell<Vec<CommandPaletteEvent>>>,
    }

    impl PickerHarness {
        fn new(cx: &mut TestAppContext, header_extra: bool, footer: bool) -> Self {
            let mounted = Rc::new(RefCell::new(None));
            let build = mounted.clone();
            let back_presses = Rc::new(Cell::new(0));
            let presses = back_presses.clone();
            let mut harness = Harness::new(
                cx,
                |cx| {
                    crate::install(cx);
                    cx.set_reduce_motion(true);
                },
                move |window, cx| {
                    build
                        .borrow_mut()
                        .get_or_insert_with(|| {
                            cx.new(|cx| {
                                let mut palette = CommandPalette::new("test.picker", window, cx)
                                    .commands(commands());
                                if header_extra {
                                    let query = palette.query_input().clone();
                                    let presses = presses.clone();
                                    palette = palette.slot(slot::HEADER_EXTRA, move |_, _| {
                                        let query = query.clone();
                                        let presses = presses.clone();
                                        IconButton::new("test.picker.back", Icon::ArrowLeft, "Back")
                                            .small()
                                            .on_click(move |window, cx| {
                                                presses.set(presses.get() + 1);
                                                query.read(cx).focus_handle(cx).focus(window, cx);
                                            })
                                            .into_any_element()
                                    });
                                }
                                if footer {
                                    palette = palette.slot(slot::FOOTER, |_, cx| {
                                        let theme = cx.theme().clone();
                                        div()
                                            .type_scale(&theme, TypeScale::Caption)
                                            .child("Fixture choices")
                                            .semantic_in(
                                                cx,
                                                NodeSpec::new(
                                                    "test.picker.footer-content",
                                                    Role::Group,
                                                )
                                                .parent("test.picker.footer"),
                                            )
                                            .into_any_element()
                                    });
                                }
                                palette
                            })
                        })
                        .clone()
                        .into_any_element()
                },
            );
            let palette = mounted.borrow().clone().expect("palette mounted");
            let events = Rc::new(RefCell::new(Vec::new()));
            let reports = events.clone();
            harness.update(|_, cx| {
                cx.subscribe(&palette, move |_, event: &CommandPaletteEvent, _| {
                    reports.borrow_mut().push(event.clone());
                })
                .detach();
            });
            Self {
                harness,
                palette,
                back_presses,
                events,
            }
        }
    }

    fn assert_inside(snapshot: &Snapshot, outer: &str, inner: &str) {
        let outer = snapshot.find(outer).expect("outer node").bounds;
        let node = snapshot.find(inner).expect("inner node");
        let bounds = node.bounds;
        assert!(
            node.visible && bounds.area() > 0.0,
            "{inner} must be visible"
        );
        assert!(
            bounds.x >= outer.x
                && bounds.y >= outer.y
                && bounds.x + bounds.width <= outer.x + outer.width
                && bounds.y + bounds.height <= outer.y + outer.height,
            "{inner} {bounds:?} must be inside {outer:?}",
        );
    }

    #[gpui::test]
    fn slots_share_the_palette_surface_with_results_and_empty_states(cx: &mut TestAppContext) {
        let PickerHarness {
            mut harness,
            palette,
            ..
        } = PickerHarness::new(cx, true, true);

        for query in ["", "zzz"] {
            harness.update(|_, cx| {
                palette.update(cx, |palette, cx| palette.set_query(query, cx));
            });
            let snapshot = harness.snapshot();
            let body = if query.is_empty() {
                "test.picker.results"
            } else {
                "test.picker.empty"
            };
            for id in [
                "test.picker.query",
                "test.picker.header-extra",
                "test.picker.back",
                "test.picker.footer",
                "test.picker.footer-content",
                body,
            ] {
                assert_inside(&snapshot, "test.picker", id);
            }
            assert_inside(&snapshot, "test.picker.header-extra", "test.picker.back");
            assert_inside(
                &snapshot,
                "test.picker.footer",
                "test.picker.footer-content",
            );
            let back = snapshot
                .find("test.picker.back")
                .expect("header back control remains mounted")
                .bounds;
            let query = snapshot
                .find("test.picker.query")
                .expect("query remains mounted beside the header control")
                .bounds;
            let body = snapshot
                .find(body)
                .expect("results or empty state is mounted below the query")
                .bounds;
            let footer = snapshot
                .find("test.picker.footer")
                .expect("footer remains mounted with results or an empty state");
            assert!(back.x + back.width < query.x, "control leads the query");
            assert_eq!(back.center().1, query.center().1, "one search row");
            assert!(query.y + query.height <= body.y, "results follow the query");
            assert!(
                body.y + body.height <= footer.bounds.y,
                "footer follows results"
            );
            assert_eq!(footer.parent.as_deref(), Some("test.picker"));
        }

        // EMPTY remains independently replaceable; it must not replace the
        // surrounding search row or footer when a host authors the vacancy.
        harness.update(|_, cx| {
            palette.update(cx, |palette, cx| {
                palette.slots_mut().set(slot::EMPTY, |_, cx| {
                    div()
                        .child("No fixture choices")
                        .semantic_in(cx, NodeSpec::new("test.picker.vacancy", Role::Group))
                        .into_any_element()
                });
                cx.notify();
            });
        });
        let snapshot = harness.snapshot();
        assert!(!snapshot.contains("test.picker.empty"));
        for id in [
            "test.picker.back",
            "test.picker.vacancy",
            "test.picker.footer",
        ] {
            assert_inside(&snapshot, "test.picker", id);
        }
        let vacancy = snapshot
            .find("test.picker.vacancy")
            .expect("caller-supplied empty state replaces the default vacancy")
            .bounds;
        assert!(
            vacancy.y + vacancy.height
                <= snapshot
                    .find("test.picker.footer")
                    .expect("footer remains mounted below the caller-supplied empty state")
                    .bounds
                    .y
        );
    }

    #[gpui::test]
    fn header_control_and_query_keyboard_keep_caller_owned_behavior(cx: &mut TestAppContext) {
        let PickerHarness {
            mut harness,
            palette,
            back_presses,
            events,
        } = PickerHarness::new(cx, true, true);
        harness.update(|window, cx| {
            palette.read(cx).focus_handle(cx).focus(window, cx);
        });
        assert!(
            !harness
                .node("test.picker.query")
                .expect("query is mounted before the header control is clicked")
                .focused
        );
        harness.click("test.picker.back");
        assert_eq!(back_presses.get(), 1);
        assert!(
            harness
                .node("test.picker.query")
                .expect("query remains mounted after the header control is clicked")
                .focused
        );
        assert!(
            events.borrow().is_empty(),
            "slot actions are not palette events"
        );

        harness.keystrokes("s a v e");
        let snapshot = harness.snapshot();
        assert!(snapshot.contains("test.picker.workspace.save"));
        assert!(snapshot.contains("test.picker.editor.save"));
        assert!(!snapshot.contains("test.picker.editor.split"));
        assert!(!snapshot.contains("test.picker.workspace.publish"));
        assert!(
            snapshot
                .find("test.picker.workspace.save")
                .expect("workspace save command matches the query")
                .hovered
        );
        assert!(
            events
                .borrow()
                .contains(&CommandPaletteEvent::QueryChanged("save".into()))
        );

        harness.keystrokes("down");
        assert!(
            harness
                .node("test.picker.editor.save")
                .expect("editor save command remains mounted after Down")
                .hovered
        );
        harness.keystrokes("up");
        assert!(
            harness
                .node("test.picker.workspace.save")
                .expect("workspace save command remains mounted after Up")
                .hovered
        );
        harness.keystrokes("down enter");
        let invoked: Vec<_> = events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                CommandPaletteEvent::Invoked(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(invoked, vec![SharedString::from("editor.save")]);

        harness.click("test.picker.back");
        assert_eq!(back_presses.get(), 2);
        harness.update(|_, cx| {
            assert_eq!(palette.read(cx).query(cx).as_ref(), "save");
        });
        harness.keystrokes("escape");
        assert_eq!(
            events.borrow().last(),
            Some(&CommandPaletteEvent::Dismissed)
        );
    }

    #[gpui::test]
    fn unslotted_query_geometry_is_unchanged_and_footer_is_independent(cx: &mut TestAppContext) {
        for (header_extra, footer) in [(false, false), (false, true), (true, false)] {
            let PickerHarness { mut harness, .. } = PickerHarness::new(cx, header_extra, footer);
            let snapshot = harness.snapshot();
            let surface = snapshot
                .find("test.picker")
                .expect("palette surface is mounted with each slot configuration")
                .bounds;
            let query = snapshot
                .find("test.picker.query")
                .expect("query is mounted with each slot configuration")
                .bounds;
            let (padding, query_height) = harness.update(|_, cx| {
                let theme = cx.theme();
                (
                    theme.space(Space::Xs),
                    theme.control.get(Default::default()).height,
                )
            });
            assert_eq!(surface.width, PALETTE_WIDTH);
            assert_eq!(query.y, surface.y + padding * 2.0);
            assert_eq!(query.height, query_height);
            if !header_extra {
                assert_eq!(query.x, surface.x + padding * 2.0);
                assert_eq!(query.width, surface.width - padding * 4.0);
            }
            assert_eq!(snapshot.contains("test.picker.header-extra"), header_extra);
            assert_eq!(snapshot.contains("test.picker.footer"), footer);
            assert_inside(&snapshot, "test.picker", "test.picker.query");
            assert_inside(&snapshot, "test.picker", "test.picker.results");
            assert!(snapshot.contains("test.picker.workspace.publish"));
        }
    }

    #[test]
    fn a_query_nothing_answers_orders_nothing() {
        assert!(order_matches(&commands(), "zzz").is_empty());
    }
}
