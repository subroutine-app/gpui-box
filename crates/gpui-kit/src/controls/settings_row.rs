//! The row every preferences page is made of, and the section it sits in.
//!
//! A setting the typist cannot change is the thing settings pages get wrong.
//! `SettingsRow::managed` renders the value the policy holds, names who holds
//! it, and never renders the caller's control — so there is no handler to
//! install and nothing on screen that looks operable and is not.
//!
//! [`SettingsList`] is the matching boundary for a complete settings page. It
//! filters the visible words and caller-supplied search terms through the
//! installed [`SearchMatcher`], preserves the
//! familiar section and row order, counts the answer, and owns the honest
//! no-match state. The caller still owns the query and commonly gets it from a
//! [`SearchField`](crate::controls::search::SearchField).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, EffectScoped, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_kit_assets::{Icon, icon};
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_theme::{ActiveTheme, Radius, Space, Surface, Theme, TypeScale};

use super::select::Select;
use super::toggle::{ActionHandler, Switch};
use crate::display::badge::Badge;
use crate::display::empty::{EmptyKind, EmptyState};
use crate::foundation::direction::{ActiveDirection, DirectionalExt};
use crate::foundation::slot::{self, Slots, Slotted};
use crate::foundation::{Ident, StyledExt, text as foundation_text};
use crate::strings::{ActiveNumbers, ActiveSearch, ActiveStrings, SearchMatcher, StringKey};

/// Why a row is showing its value instead of its control.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Withheld {
    /// Held by policy: someone else decides this, and this is who.
    Managed(SharedString),
    /// The whole group it belongs to does not apply here.
    Inapplicable(SharedString),
}

impl Withheld {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Managed(_) => "managed",
            Self::Inapplicable(_) => "inapplicable",
        }
    }

    /// What the row says where its control would have been.
    ///
    /// An inapplicable row says only that it does not apply: the reason
    /// belongs to the whole section, which states it once above the rows
    /// rather than once per row.
    fn sentence(&self, cx: &App) -> SharedString {
        match self {
            Self::Managed(controller) => cx
                .strings()
                .format(StringKey::SettingsManagedBy, &[controller.as_ref()]),
            Self::Inapplicable(_) => cx.strings().text(StringKey::SettingsInapplicable),
        }
    }

    fn glyph(&self) -> Icon {
        match self {
            Self::Managed(_) => Icon::Key,
            Self::Inapplicable(_) => Icon::Info,
        }
    }
}

enum RowControl {
    Custom(AnyElement),
    Switch(Switch),
    Select(Entity<Select>),
}

/// One setting: a wrapping name/description and a trailing control. Controls
/// move below the name when the available width cannot accommodate both.
#[derive(IntoElement)]
pub struct SettingsRow {
    ident: Ident,
    label: SharedString,
    label_width: Option<Pixels>,
    description: Option<SharedString>,
    badge: Option<SharedString>,
    /// What the setting currently holds, in the caller's words.
    value: Option<SharedString>,
    control: Option<RowControl>,
    control_width: Option<Pixels>,
    stacked: bool,
    withheld: Option<Withheld>,
    search_terms: Vec<SharedString>,
}

impl std::fmt::Debug for SettingsRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsRow")
            .field("ident", &self.ident)
            .field("label", &self.label)
            .field("badge", &self.badge)
            .field("withheld", &self.withheld)
            .field("has_control", &self.control.is_some())
            .finish()
    }
}

impl SettingsRow {
    pub fn new(ident: impl Into<Ident>, label: impl Into<SharedString>) -> Self {
        Self {
            ident: ident.into(),
            label: label.into(),
            label_width: None,
            description: None,
            badge: None,
            value: None,
            control: None,
            control_width: None,
            stacked: false,
            withheld: None,
            search_terms: Vec::new(),
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Overrides the minimum width of this row's inherited name/description column.
    pub fn label_width(mut self, width: Pixels) -> Self {
        self.label_width = Some(width.max(px(0.0)));
        self
    }

    /// A short note beside the label, such as "Requires restart".
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// What the setting holds, published whether or not the control is drawn.
    /// A managed row has nothing else to show, so it is the only reading the
    /// typist gets.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Arbitrary content, without label activation. Its column has a readable
    /// minimum for fill-style inputs, but can grow to fit intrinsic actions.
    /// Use [`Self::switch`] or [`Self::select`] to bind the name to a control.
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(RowControl::Custom(control.into_any_element()));
        self
    }

    /// Names the switch and makes the name/description focus it and activate
    /// its existing callback. Disabled, handlerless, and withheld controls get
    /// no label handler or additional tab stop.
    pub fn switch(mut self, switch: Switch) -> Self {
        self.control = Some(RowControl::Switch(switch.named(self.label.clone())));
        self
    }

    /// Names the select and makes the name/description toggle that same entity.
    /// Opening focuses it; its selection subscription remains the only callback.
    /// Unless overridden, the column fits the longest option (up to the row's
    /// available width), so accepting a different choice does not move it.
    pub fn select(mut self, select: Entity<Select>) -> Self {
        self.control = Some(RowControl::Select(select));
        self
    }

    /// Overrides the trailing column width, capped by the available row width.
    /// Without an override switches are intrinsic, and arbitrary controls can
    /// grow beyond their readable minimum instead of clipping long actions.
    pub fn control_width(mut self, width: Pixels) -> Self {
        self.control_width = Some(width.max(px(0.0)));
        self
    }

    /// Places a full-width control below the name, for composite editors and
    /// long action rows. An explicit control width still applies to its content.
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    /// Caller-authored aliases and control vocabulary that are not already
    /// visible in the row.
    ///
    /// Label, description, badge, displayed value, and withholding reason are
    /// searched automatically. An arbitrary control is opaque to the row, so
    /// names that exist only inside it — an option label, for example — belong
    /// here rather than in a second downstream filtering implementation.
    pub fn search_terms(
        mut self,
        terms: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.search_terms.extend(terms.into_iter().map(Into::into));
        self
    }

    /// Marks the setting as decided elsewhere, and by whom.
    ///
    /// The control is not rendered at all — a managed setting installs no
    /// handler because there is nothing there to install one on — and the row
    /// states the value and its controller instead.
    pub fn managed(mut self, controller: impl Into<SharedString>) -> Self {
        self.withheld = Some(Withheld::Managed(controller.into()));
        self
    }

    fn inapplicable(mut self, reason: SharedString) -> Self {
        if self.withheld.is_none() {
            self.withheld = Some(Withheld::Inapplicable(reason));
        }
        self
    }

    fn matches(&self, query: &str, matcher: &dyn SearchMatcher, cx: &App) -> bool {
        let visible_match = [
            Some(&self.label),
            self.description.as_ref(),
            self.badge.as_ref(),
            self.value.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.search_terms.iter())
        .any(|text| matcher.rank(query, text.as_ref()).is_some());

        visible_match
            || self
                .withheld
                .as_ref()
                .is_some_and(|withheld| matcher.rank(query, &withheld.sentence(cx)).is_some())
    }

    fn render_in(self, theme: &Theme, window: &mut Window, cx: &mut App) -> AnyElement {
        let direction = cx.layout_direction();
        let withheld = self.withheld.clone();
        let ident = self.ident.clone();
        let select_label =
            matches!(self.control, Some(RowControl::Select(_))) && withheld.is_none();
        let label_hitbox = Rc::new(RefCell::new(None));
        let fill_control =
            matches!(self.control, Some(RowControl::Custom(_))) && withheld.is_none();
        let control_width = self.control_width.or_else(|| {
            if self.stacked || withheld.is_some() {
                return None;
            }
            match &self.control {
                Some(RowControl::Select(select)) => {
                    Some(select.read(cx).preferred_width(window, cx))
                }
                _ => None,
            }
        });
        let (control, activation): (Option<AnyElement>, Option<ActionHandler>) =
            if withheld.is_some() {
                (None, None)
            } else {
                match self.control {
                    Some(RowControl::Custom(control)) => (Some(control), None),
                    Some(RowControl::Switch(switch)) => {
                        if let Some(activate) = switch.activation() {
                            let focus = window
                                .use_keyed_state(
                                    ident.child("switch-focus").element_id(),
                                    cx,
                                    |_, cx| cx.focus_handle().tab_stop(true),
                                )
                                .read(cx)
                                .clone();
                            let switch = switch.with_focus_handle(focus.clone());
                            let activation = Rc::new(move |window: &mut Window, cx: &mut App| {
                                window.focus_from_pointer(&focus, cx);
                                activate(window, cx);
                            }) as ActionHandler;
                            (Some(switch.into_any_element()), Some(activation))
                        } else {
                            (Some(switch.into_any_element()), None)
                        }
                    }
                    Some(RowControl::Select(select)) => {
                        select.update(cx, |select, cx| {
                            select.set_name(self.label.clone(), cx);
                            select.set_label_hitbox(Rc::downgrade(&label_hitbox));
                        });
                        let activation = (!select.read(cx).is_disabled()).then(|| {
                            let select = select.clone();
                            Rc::new(move |window: &mut Window, cx: &mut App| {
                                select.update(cx, |select, cx| select.toggle(window, cx));
                            }) as ActionHandler
                        });
                        (Some(select.into_any_element()), activation)
                    }
                    None => (None, None),
                }
            };
        let label_width = self
            .label_width
            .unwrap_or(px(theme.measures.settings_label));

        let mut spec = NodeSpec::new(ident.semantic_id(), Role::Row).text(self.label.clone());
        if let Some(value) = self.value.clone() {
            spec = spec.value(value);
        }
        if withheld.is_some() {
            spec = spec.disabled(true);
        }

        let names = div()
            .id(ident.child("names").element_id())
            .column()
            .flex_grow(1.0)
            .flex_shrink_0()
            .flex_basis(label_width)
            .min_w(label_width)
            .max_w_full()
            .when(self.stacked, |names| names.flex_none().w_full())
            .gap_token(theme, Space::Xs)
            .when_some(activation, |names, activate| {
                let names = names.cursor_pointer();
                if select_label {
                    let hitbox = label_hitbox.clone();
                    names.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        // Keep the exemption alive exactly as long as this mounted
                        // activation handler. Toggle before pointer focus leaves it.
                        let _keep_alive = hitbox.clone();
                        activate(window, cx);
                        cx.stop_propagation();
                    })
                } else {
                    names.on_click(move |_, window, cx| activate(window, cx))
                }
            })
            .child(
                div()
                    .row_reading(direction)
                    .w_full()
                    .items_center()
                    .flex_wrap()
                    .gap_token(theme, Space::Xs)
                    .child(
                        foundation_text(theme, TypeScale::Body, self.label.clone())
                            .min_w_0()
                            .semantic_in(
                                cx,
                                NodeSpec::new(ident.child("label").semantic_id(), Role::Text)
                                    .parent(ident.semantic_id())
                                    .text(self.label.clone()),
                            ),
                    )
                    .children(
                        self.badge
                            .clone()
                            .map(|badge| Badge::new(badge).id(ident.child("badge")).warning()),
                    ),
            )
            .children(self.description.clone().map(|description| {
                foundation_text(theme, TypeScale::Body, description.clone())
                    .w_full()
                    .min_w_0()
                    .text_tone(theme, gpui_kit_theme::TextTone::Muted)
                    .semantic_in(
                        cx,
                        NodeSpec::new(ident.child("description").semantic_id(), Role::Text)
                            .parent(ident.semantic_id())
                            .text(description),
                    )
            }))
            .semantic_in(
                cx,
                NodeSpec::new(ident.child("names").semantic_id(), Role::Group)
                    .parent(ident.semantic_id()),
            );

        // A withheld row shows what is set and who set it. The control never
        // reaches the tree, so nothing can be operated by mistake.
        let right = match (&withheld, control) {
            (Some(withheld), _) => div()
                .column()
                .items_end()
                .flex_none()
                .gap(px(theme.space(Space::Xxs)))
                .children(self.value.clone().map(|value| {
                    foundation_text(theme, TypeScale::Label, value)
                        .text_tone(theme, gpui_kit_theme::TextTone::Muted)
                }))
                .child(
                    div()
                        .row_reading(direction)
                        .gap(px(theme.space(Space::Xs)))
                        .child(
                            icon(withheld.glyph())
                                .size(px(theme.control.xs.icon_size))
                                .text_color(theme.colors.text_faint),
                        )
                        .child(
                            foundation_text(theme, TypeScale::Caption, withheld.sentence(cx))
                                .text_tone(theme, gpui_kit_theme::TextTone::Faint),
                        )
                        .semantic_in(
                            cx,
                            NodeSpec::new(ident.child("managed").semantic_id(), Role::Status)
                                .parent(ident.semantic_id())
                                .text(withheld.sentence(cx))
                                .value(withheld.as_str()),
                        ),
                )
                .into_any_element(),
            (None, Some(control)) => control,
            (None, None) => div()
                .flex_none()
                .children(self.value.clone().map(|value| {
                    foundation_text(theme, TypeScale::Label, value)
                        .text_tone(theme, gpui_kit_theme::TextTone::Muted)
                }))
                .into_any_element(),
        };

        div()
            .row_reading(direction)
            .flex_wrap()
            .w_full()
            .min_w_0()
            .items_start()
            .justify_end()
            .when(direction.is_rtl(), |row| row.justify_start())
            .gap_token(theme, Space::Lg)
            .px_token(theme, Space::Lg)
            .py_token(theme, Space::Md)
            .child(names)
            .child(
                div()
                    .row_reading(direction)
                    .flex_none()
                    .max_w_full()
                    .when(
                        fill_control && !self.stacked && self.control_width.is_none(),
                        |field| field.min_w(px(theme.measures.compact_menu_min_width)),
                    )
                    .when(self.stacked, |field| field.w_full())
                    .when_some(control_width, |field, width| field.w(width))
                    .justify_end()
                    .when(direction.is_rtl(), |field| field.justify_start())
                    .child(right)
                    .semantic_in(
                        cx,
                        NodeSpec::new(ident.child("field").semantic_id(), Role::Group)
                            .parent(ident.semantic_id()),
                    ),
            )
            .when(select_label, |row| {
                row.on_children_prepainted(move |bounds, window, _| {
                    if let Some(names) = bounds.first() {
                        *label_hitbox.borrow_mut() =
                            Some(window.insert_hitbox(*names, gpui::HitboxBehavior::Normal));
                    }
                })
            })
            .semantic_in(cx, spec)
            .into_any_element()
    }
}

impl RenderOnce for SettingsRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        self.render_in(&theme, window, cx)
    }
}

type ActionSlot = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

enum SectionContent {
    Row(Box<EffectScoped<SettingsRow>>),
    Block(AnyElement),
}

/// A headed group of settings rows.
#[derive(IntoElement)]
pub struct SettingsSection {
    ident: Ident,
    title: SharedString,
    description: Option<SharedString>,
    dimmed: Option<SharedString>,
    label_width: Option<Pixels>,
    content: Vec<SectionContent>,
    action: Option<ActionSlot>,
}

impl std::fmt::Debug for SettingsSection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsSection")
            .field("ident", &self.ident)
            .field("title", &self.title)
            .field("dimmed", &self.dimmed)
            .field("rows", &self.row_count())
            .finish()
    }
}

impl SettingsSection {
    pub fn new(ident: impl Into<Ident>, title: impl Into<SharedString>) -> Self {
        Self {
            ident: ident.into(),
            title: title.into(),
            description: None,
            dimmed: None,
            label_width: None,
            content: Vec::new(),
            action: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// States that nothing in this group applies here, and why.
    ///
    /// A dimmed group renders none of its controls, for the same reason a
    /// managed row renders none of its own: a setting that cannot take effect
    /// must not look as though it can.
    pub fn dimmed_by(mut self, reason: impl Into<SharedString>) -> Self {
        self.dimmed = Some(reason.into());
        self
    }

    pub fn row(mut self, row: impl Into<EffectScoped<SettingsRow>>) -> Self {
        self.content.push(SectionContent::Row(Box::new(row.into())));
        self
    }

    pub fn rows<R: Into<EffectScoped<SettingsRow>>>(
        mut self,
        rows: impl IntoIterator<Item = R>,
    ) -> Self {
        self.content.extend(
            rows.into_iter()
                .map(|row| SectionContent::Row(Box::new(row.into()))),
        );
        self
    }

    /// The shared minimum width for each name/description column; rows with an
    /// explicit width keep their override. Defaults to `measure.settingsLabel`,
    /// scaled with density and local zoom.
    pub fn label_width(mut self, width: Pixels) -> Self {
        self.label_width = Some(width.max(px(0.0)));
        self
    }

    /// A full-width block among rows, preserving builder call order. Blocks
    /// share row padding and inset separators. A dimmed section omits blocks
    /// because their arbitrary controls cannot be made inapplicable safely.
    /// SettingsList includes blocks only when the section itself matches.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.content
            .push(SectionContent::Block(child.into_any_element()));
        self
    }

    fn row_count(&self) -> usize {
        self.content
            .iter()
            .filter(|item| matches!(item, SectionContent::Row(_)))
            .count()
    }

    /// A control in the section heading, such as "Reset to defaults".
    pub fn action(
        mut self,
        action: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.action = Some(Rc::new(action));
        self
    }

    fn filtered(&mut self, query: &str, matcher: &dyn SearchMatcher, cx: &App) -> Option<usize> {
        let query_is_empty = query.trim().is_empty();
        let section_matches = query_is_empty
            || [
                Some(&self.title),
                self.description.as_ref(),
                self.dimmed.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| matcher.rank(query, text.as_ref()).is_some());

        if section_matches {
            let count = self.row_count();
            return Some(count);
        }

        self.content.retain(|item| match item {
            SectionContent::Row(row) => row.as_ref().as_ref().matches(query, matcher, cx),
            SectionContent::Block(_) => false,
        });
        let count = self.row_count();
        (count > 0).then_some(count)
    }
}

impl RenderOnce for SettingsSection {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let direction = cx.layout_direction();
        let dimmed = self.dimmed.clone();
        let ident = self.ident.clone();

        let heading = div()
            .row_reading(direction)
            .w_full()
            .gap_token(&theme, Space::Sm)
            .child(
                div()
                    .column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(theme.space(Space::Xxs)))
                    .child(foundation_text(&theme, TypeScale::Body, self.title.clone()))
                    .children(self.description.clone().map(|description| {
                        foundation_text(&theme, TypeScale::Body, description)
                            .text_tone(&theme, gpui_kit_theme::TextTone::Muted)
                    })),
            )
            .children(
                self.action
                    .as_ref()
                    .filter(|_| dimmed.is_none())
                    .map(|action| action(window, cx)),
            );

        let reason = dimmed.clone().map(|reason| {
            div()
                .row_reading(direction)
                .w_full()
                .gap_token(&theme, Space::Xs)
                .child(
                    icon(Icon::Info)
                        .size(px(theme.control.xs.icon_size))
                        .text_color(theme.colors.text_faint),
                )
                .child(
                    foundation_text(&theme, TypeScale::Caption, reason.clone())
                        .text_tone(&theme, gpui_kit_theme::TextTone::Faint),
                )
                .semantic_in(
                    cx,
                    NodeSpec::new(ident.child("dimmed").semantic_id(), Role::Status)
                        .parent(ident.semantic_id())
                        .text(reason)
                        .value("inapplicable"),
                )
        });

        let label_width = self
            .label_width
            .unwrap_or(px(theme.measures.settings_label));
        let mut content = Vec::new();
        for item in self.content {
            let item = match item {
                SectionContent::Row(row) => (*row)
                    .map(|mut row| {
                        row.label_width.get_or_insert(label_width);
                        if let Some(reason) = dimmed.clone() {
                            row = row.inapplicable(reason);
                        }
                        row
                    })
                    .into_any_element(),
                SectionContent::Block(block) if dimmed.is_none() => div()
                    .w_full()
                    .min_w_0()
                    .px_token(&theme, Space::Lg)
                    .py_token(&theme, Space::Md)
                    .child(block)
                    .into_any_element(),
                SectionContent::Block(_) => continue,
            };
            if !content.is_empty() {
                content.push(
                    div()
                        .w_full()
                        .px_token(&theme, Space::Lg)
                        .child(crate::foundation::inset_rule(&theme).w_full())
                        .into_any_element(),
                );
            }
            content.push(item);
        }

        div()
            .column()
            .w_full()
            .gap_token(&theme, Space::Md)
            .child(heading)
            .children(reason)
            .child(
                div()
                    .column()
                    .w_full()
                    .surface(&theme, Surface::Raised)
                    .radius(&theme, Radius::Card)
                    .border(px(theme.borders.hairline))
                    .border_color(theme.colors.control_hairline)
                    .when(dimmed.is_some(), |element| {
                        element.opacity(theme.opacity.disabled)
                    })
                    .children(content),
            )
            .semantic_in(
                cx,
                NodeSpec::new(ident.semantic_id(), Role::Group)
                    .text(self.title.clone())
                    .disabled(dimmed.is_some()),
            )
    }
}

/// Settings sections filtered by one caller-owned query.
///
/// The list searches every visible row phrase plus [`SettingsRow::search_terms`]
/// with the active locale matcher. It does not reorder matches: a settings
/// page remains spatially familiar while it narrows. The query field is kept
/// outside so a host can place it in its own page chrome without rebuilding a
/// second filtering state machine.
#[derive(IntoElement)]
pub struct SettingsList {
    ident: Ident,
    query: SharedString,
    sections: Vec<EffectScoped<SettingsSection>>,
    slots: Slots,
}

impl std::fmt::Debug for SettingsList {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsList")
            .field("ident", &self.ident)
            .field("query", &self.query)
            .field("sections", &self.sections.len())
            .finish()
    }
}

impl SettingsList {
    pub fn new(ident: impl Into<Ident>) -> Self {
        Self {
            ident: ident.into(),
            query: SharedString::default(),
            sections: Vec::new(),
            slots: Slots::default(),
        }
    }

    pub fn query(mut self, query: impl Into<SharedString>) -> Self {
        self.query = query.into();
        self
    }

    pub fn section(mut self, section: impl Into<EffectScoped<SettingsSection>>) -> Self {
        self.sections.push(section.into());
        self
    }

    pub fn sections<S: Into<EffectScoped<SettingsSection>>>(
        mut self,
        sections: impl IntoIterator<Item = S>,
    ) -> Self {
        self.sections.extend(sections.into_iter().map(Into::into));
        self
    }
}

impl Slotted for SettingsList {
    /// Page chrome is independent of filtering and remains mounted for empty
    /// and no-match results. Put the caller's search field in `header`, category
    /// navigation in `sidebar`, and save/reset actions in `footer`.
    const SLOTS: &'static [&'static str] = &[slot::EMPTY, "header", "sidebar", "footer"];

    fn slots_mut(&mut self) -> &mut Slots {
        &mut self.slots
    }
}

impl RenderOnce for SettingsList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !["header", "sidebar", "footer"]
            .iter()
            .any(|name| self.slots.holds(name))
        {
            return self.render_content(window, cx).into_any_element();
        }
        let theme = cx.theme().clone();
        let direction = cx.layout_direction();
        let ident = self.ident.clone();
        let header = self.slots.render("header", window, cx);
        let sidebar = self.slots.render("sidebar", window, cx);
        let footer = self.slots.render("footer", window, cx);
        let content = self.render_content(window, cx);
        div()
            .id(ident.child("page").element_id())
            .column()
            .w_full()
            .min_w_0()
            .gap_token(&theme, Space::Md)
            .children(header.map(|header| {
                div().w_full().child(header).semantic_in(
                    cx,
                    NodeSpec::new(ident.child("header").semantic_id(), Role::Group),
                )
            }))
            .child(
                div()
                    .row_reading(direction)
                    .w_full()
                    .items_start()
                    .gap_token(&theme, Space::Lg)
                    .children(sidebar.map(|sidebar| {
                        div().flex_none().child(sidebar).semantic_in(
                            cx,
                            NodeSpec::new(ident.child("sidebar").semantic_id(), Role::Group),
                        )
                    }))
                    .child(div().flex_1().min_w_0().child(content)),
            )
            .children(footer.map(|footer| {
                div().w_full().child(footer).semantic_in(
                    cx,
                    NodeSpec::new(ident.child("footer").semantic_id(), Role::Group),
                )
            }))
            .semantic_in(
                cx,
                NodeSpec::new(ident.child("page").semantic_id(), Role::Group),
            )
            .into_any_element()
    }
}

impl SettingsList {
    fn render_content(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = cx.theme().clone();
        let matcher = cx.search();
        let query_is_empty = self.query.trim().is_empty();
        let had_sections = !self.sections.is_empty();
        let mut count = 0;
        let sections: Vec<_> = self
            .sections
            .into_iter()
            .filter_map(|section| {
                let mut matched = None;
                let section = section.map(|mut section| {
                    matched = section.filtered(self.query.as_ref(), matcher.as_ref(), cx);
                    section
                });
                count += matched?;
                Some(section)
            })
            .collect();

        let root_id = self.ident.semantic_id();
        if sections.is_empty() {
            let key = if !query_is_empty && had_sections {
                StringKey::SettingsNoResults
            } else {
                StringKey::SettingsEmpty
            };
            return div()
                .id(self.ident.element_id())
                .child(self.slots.or_else(slot::EMPTY, window, cx, |_, cx| {
                    EmptyState::new(self.ident.child("empty"), cx.strings().text(key))
                        .kind(EmptyKind::Empty)
                        .into_any_element()
                }))
                .semantic_in(
                    cx,
                    NodeSpec::new(root_id, Role::Group).value(cx.numbers().count(0)),
                )
                .into_any_element();
        }

        let status = (!query_is_empty).then(|| {
            let sentence = cx.strings().format_plural(
                StringKey::SettingsResultOne,
                StringKey::SettingsResultMany,
                cx.numbers().plural(count),
                &[cx.numbers().count(count).as_ref()],
            );
            foundation_text(&theme, TypeScale::Caption, sentence.clone())
                .text_tone(&theme, gpui_kit_theme::TextTone::Muted)
                .semantic_in(
                    cx,
                    NodeSpec::new(self.ident.child("status").semantic_id(), Role::Status)
                        .parent(root_id.clone())
                        .text(sentence)
                        .value(cx.numbers().count(count)),
                )
        });

        div()
            .id(self.ident.element_id())
            .column()
            .w_full()
            .gap_token(&theme, Space::Md)
            .children(status)
            .children(sections)
            .semantic_in(
                cx,
                NodeSpec::new(root_id, Role::Group).value(cx.numbers().count(count)),
            )
            .into_any_element()
    }
}
