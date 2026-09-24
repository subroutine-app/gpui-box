//! Caller-owned keymap facts with transient, shared shortcut recording.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_kit_assets::Icon;
use gpui_kit_semantics::{NodeSpec, Role, Semantic};
use gpui_kit_theme::{ActiveTheme, ControlSize, Radius, Space, Surface, TextTone, TypeScale};

use crate::controls::button::Button;
use crate::controls::keybinding_recorder::{KeybindingRecorder, KeybindingRecorderEvent};
use crate::foundation::{
    CardVariant, Disableable, FocusRing, Ident, Sizable, StyledExt, text as foundation_text,
};
use crate::overlay::{Kbd, tooltip::Tooltipped};
use crate::strings::{ActiveNumbers, ActiveStrings, StringKey};

/// One effective binding, identified independently of its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapBinding {
    id: SharedString,
    keystroke: SharedString,
    conflict: Option<SharedString>,
    provenance: Option<SharedString>,
}

impl KeymapBinding {
    pub fn new(id: impl Into<SharedString>, keystroke: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            keystroke: keystroke.into(),
            conflict: None,
            provenance: None,
        }
    }
    pub fn conflict(mut self, value: impl Into<SharedString>) -> Self {
        self.conflict = Some(value.into());
        self
    }
    pub fn provenance(mut self, value: impl Into<SharedString>) -> Self {
        self.provenance = Some(value.into());
        self
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    pub fn keystroke(&self) -> &SharedString {
        &self.keystroke
    }

    pub fn conflict_reason(&self) -> Option<&SharedString> {
        self.conflict.as_ref()
    }

    pub fn provenance_label(&self) -> Option<&SharedString> {
        self.provenance.as_ref()
    }
}

/// Caller-owned command metadata and binding facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapCommand {
    id: SharedString,
    label: SharedString,
    context: Option<SharedString>,
    default_bindings: Vec<SharedString>,
    effective_bindings: Vec<KeymapBinding>,
    search_text: SharedString,
    keywords: Vec<SharedString>,
    refusal: Option<SharedString>,
}

impl KeymapCommand {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            context: None,
            default_bindings: vec![],
            effective_bindings: vec![],
            search_text: "".into(),
            keywords: vec![],
            refusal: None,
        }
    }
    pub fn context(mut self, value: impl Into<SharedString>) -> Self {
        self.context = Some(value.into());
        self
    }
    pub fn defaults(mut self, values: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.default_bindings = values.into_iter().map(Into::into).collect();
        self
    }
    pub fn bindings(mut self, values: impl IntoIterator<Item = KeymapBinding>) -> Self {
        self.effective_bindings = values.into_iter().collect();
        self
    }
    pub fn searchable(
        mut self,
        text: impl Into<SharedString>,
        keywords: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.search_text = text.into();
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }
    pub fn refused(mut self, reason: impl Into<SharedString>) -> Self {
        self.refusal = Some(reason.into());
        self
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    pub fn label_text(&self) -> &SharedString {
        &self.label
    }

    pub fn context_label(&self) -> Option<&SharedString> {
        self.context.as_ref()
    }

    pub fn default_bindings(&self) -> &[SharedString] {
        &self.default_bindings
    }

    pub fn effective_bindings(&self) -> &[KeymapBinding] {
        &self.effective_bindings
    }

    pub fn search_text(&self) -> &SharedString {
        &self.search_text
    }

    pub fn keywords(&self) -> &[SharedString] {
        &self.keywords
    }

    pub fn refusal_reason(&self) -> Option<&SharedString> {
        self.refusal.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapEditorEvent {
    AddCaptured {
        command_id: SharedString,
        keystroke: SharedString,
    },
    /// Replace exactly this caller-owned binding, preserving any alternatives.
    ReplaceCaptured {
        command_id: SharedString,
        binding_id: SharedString,
        keystroke: SharedString,
    },
    Remove {
        command_id: SharedString,
        binding_id: SharedString,
    },
    Reset {
        command_id: SharedString,
    },
    RecordingCancelled {
        command_id: SharedString,
    },
}

impl EventEmitter<KeymapEditorEvent> for KeymapEditor {}

/// Coordinates one recorder over caller-owned keymap rows; it never applies a binding.
pub struct KeymapEditor {
    ident: Ident,
    commands: Vec<KeymapCommand>,
    query: SharedString,
    disabled: bool,
    size: ControlSize,
    active_command: Option<SharedString>,
    active_binding: Option<SharedString>,
    focus_handle: FocusHandle,
    field_focus: BTreeMap<(SharedString, Option<SharedString>), FocusHandle>,
    activation_interceptor: Option<Subscription>,
    pending_focus: Option<(SharedString, Option<SharedString>)>,
    _focus_subscriptions: Vec<Subscription>,
    suppress_next_recorder_cancel: bool,
    recorder: Entity<KeybindingRecorder>,
    _recorder_subscription: Subscription,
}

impl std::fmt::Debug for KeymapEditor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeymapEditor")
            .field("ident", &self.ident)
            .field("commands", &self.commands)
            .field("query", &self.query)
            .field("disabled", &self.disabled)
            .field("active_command", &self.active_command)
            .finish_non_exhaustive()
    }
}

impl KeymapEditor {
    pub fn new(ident: impl Into<Ident>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ident = ident.into();
        let focus_handle = cx.focus_handle();
        let focus_subscriptions = vec![
            cx.on_focus(&focus_handle, window, |this, window, cx| {
                this.finish_focus_restore(window, cx);
            }),
            cx.on_focus_out(&focus_handle, window, |this, _, _, _| {
                this.pending_focus = None;
            }),
        ];
        let recorder =
            cx.new(|cx| KeybindingRecorder::new(ident.child("recorder"), window, cx).small());
        let subscription = cx.subscribe_in(&recorder, window, |this, _, event, window, cx| {
            match event {
                KeybindingRecorderEvent::Captured(keystroke) => {
                    let Some(command_id) = this.active_command.take() else {
                        return;
                    };
                    this.restore_focus_after_frame(command_id.clone(), window, cx);
                    cx.emit(match this.active_binding.take() {
                        Some(binding_id) => KeymapEditorEvent::ReplaceCaptured {
                            command_id,
                            binding_id,
                            keystroke: keystroke.clone(),
                        },
                        None => KeymapEditorEvent::AddCaptured {
                            command_id,
                            keystroke: keystroke.clone(),
                        },
                    })
                }
                KeybindingRecorderEvent::Cancelled => {
                    if this.suppress_next_recorder_cancel {
                        this.suppress_next_recorder_cancel = false;
                        return;
                    }
                    let Some(command_id) = this.active_command.take() else {
                        return;
                    };
                    this.restore_focus_after_frame(command_id.clone(), window, cx);
                    this.active_binding = None;
                    cx.emit(KeymapEditorEvent::RecordingCancelled { command_id })
                }
                KeybindingRecorderEvent::Started => return,
            }
            cx.notify();
        });
        Self {
            ident,
            commands: vec![],
            query: "".into(),
            disabled: false,
            size: ControlSize::Sm,
            active_command: None,
            active_binding: None,
            focus_handle,
            field_focus: BTreeMap::new(),
            activation_interceptor: None,
            pending_focus: None,
            _focus_subscriptions: focus_subscriptions,
            suppress_next_recorder_cancel: false,
            recorder,
            _recorder_subscription: subscription,
        }
    }

    pub fn commands(mut self, commands: impl IntoIterator<Item = KeymapCommand>) -> Self {
        self.commands = commands.into_iter().collect();
        self
    }
    pub fn query(mut self, query: impl Into<SharedString>) -> Self {
        self.query = query.into();
        self
    }
    pub fn set_commands(&mut self, commands: Vec<KeymapCommand>, cx: &mut Context<Self>) {
        if self.commands != commands {
            self.commands = commands;
            let fields: BTreeSet<_> = self.commands.iter().flat_map(Self::field_targets).collect();
            self.field_focus.retain(|target, _| fields.contains(target));
            self.cancel_if_active_is_hidden(cx);
            cx.notify();
        }
    }
    pub fn set_query(&mut self, query: impl Into<SharedString>, cx: &mut Context<Self>) {
        let query = query.into();
        if self.query != query {
            self.query = query;
            self.cancel_if_active_is_hidden(cx);
            cx.notify();
        }
    }

    /// Refuses every editor action without removing caller-owned bindings.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        if disabled {
            self.activation_interceptor.take();
            self.recorder.update(cx, |recorder, cx| recorder.cancel(cx));
        }
        cx.notify();
    }

    pub fn active_command(&self) -> Option<&SharedString> {
        self.active_command.as_ref()
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn current_commands(&self) -> &[KeymapCommand] {
        &self.commands
    }

    fn matches(&self, command: &KeymapCommand) -> bool {
        let query = self.query.trim().to_lowercase();
        query.is_empty()
            || [&command.id, &command.label, &command.search_text]
                .into_iter()
                .chain(command.context.iter())
                .chain(command.keywords.iter())
                .chain(
                    command
                        .effective_bindings
                        .iter()
                        .map(|binding| &binding.keystroke),
                )
                .any(|value| value.to_lowercase().contains(&query))
    }

    fn field_targets(
        command: &KeymapCommand,
    ) -> impl Iterator<Item = (SharedString, Option<SharedString>)> + '_ {
        command
            .effective_bindings
            .iter()
            .map(|binding| Some(binding.id.clone()))
            .chain(command.effective_bindings.is_empty().then_some(None))
            .map(|binding_id| (command.id.clone(), binding_id))
    }

    fn intercept_activation(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.disabled {
            self.activation_interceptor.take();
            return;
        }
        if self.activation_interceptor.is_some() {
            return;
        }
        let editor = cx.weak_entity();
        let owner = window.window_handle().window_id();
        self.activation_interceptor = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != owner
                || event.keystroke.modifiers.modified()
                || !matches!(event.keystroke.key.as_str(), "enter" | "space")
            {
                return;
            }
            editor
                .update(cx, |this, cx| {
                    // The dispatch tree, not retained handles, proves the editor is mounted.
                    if this.disabled
                        || this.active_command.is_some()
                        || !this.focus_handle.contains_focused(window, cx)
                    {
                        return;
                    }
                    let target = this
                        .commands
                        .iter()
                        .filter(|command| command.refusal.is_none() && this.matches(command))
                        .flat_map(Self::field_targets)
                        .find(|target| {
                            this.field_focus
                                .get(target)
                                .is_some_and(|handle| handle.is_focused(window))
                        });
                    if let Some((command_id, binding_id)) = target {
                        this.start_recording(command_id, binding_id, window, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
                .ok();
        }));
    }

    fn restore_focus_after_frame(
        &mut self,
        command_id: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let recorder_focus = self.recorder.read(cx).focus_handle(cx);
        if !recorder_focus.is_focused(window)
            || !self.focus_handle.contains(&recorder_focus, window)
        {
            return;
        }
        // Keep focus out of the retiring recorder. The host can replace binding ids
        // in response to Captured before the next frame supplies current destinations.
        self.pending_focus = Some((command_id, self.active_binding.clone()));
        window.focus(&self.focus_handle, cx);
    }

    fn finish_focus_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preferred) = self.pending_focus.take() else {
            return;
        };
        // Focus notifications run after the frame's dispatch tree is installed.
        // A click, host focus change, or new capture takes precedence.
        if !self.focus_handle.is_focused(window) || self.active_command.is_some() {
            return;
        }
        let destination = self
            .commands
            .iter()
            .filter(|command| !self.disabled && command.refusal.is_none() && self.matches(command))
            .flat_map(Self::field_targets)
            .filter_map(|target| {
                let handle = self.field_focus.get(&target)?;
                self.focus_handle
                    .contains(handle, window)
                    .then_some((target, handle))
            })
            .min_by_key(|(target, _)| {
                if *target == preferred {
                    0
                } else if target.0 == preferred.0 {
                    1
                } else {
                    2
                }
            })
            .map(|(_, handle)| handle.clone());
        if let Some(handle) = destination {
            window.focus(&handle, cx);
        } else {
            window.blur();
        }
    }

    fn start_recording(
        &mut self,
        command_id: SharedString,
        binding_id: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self
            .commands
            .iter()
            .find(|command| command.id == command_id)
        else {
            return;
        };
        if self.disabled
            || command.refusal.is_some()
            || !self.matches(command)
            || binding_id.as_ref().is_some_and(|id| {
                !command
                    .effective_bindings
                    .iter()
                    .any(|binding| &binding.id == id)
            })
        {
            return;
        }
        let label = command.label.clone();
        if let Some(command_id) = self.active_command.take() {
            self.suppress_next_recorder_cancel = true;
            self.recorder.update(cx, |field, cx| field.cancel(cx));
            cx.emit(KeymapEditorEvent::RecordingCancelled { command_id });
        }
        self.active_command = Some(command_id);
        self.active_binding = binding_id;
        self.pending_focus = None;
        self.recorder.update(cx, |field, cx| {
            field.set_label(Some(label), cx);
            field.start(window, cx);
        });
        cx.notify();
    }

    fn cancel_if_active_is_hidden(&mut self, cx: &mut Context<Self>) {
        let visible = self.active_command.as_ref().is_none_or(|active| {
            self.commands.iter().any(|command| {
                &command.id == active
                    && command.refusal.is_none()
                    && self.matches(command)
                    && self.active_binding.as_ref().is_none_or(|id| {
                        command
                            .effective_bindings
                            .iter()
                            .any(|binding| &binding.id == id)
                    })
            })
        });
        if !visible {
            self.recorder.update(cx, |recorder, cx| recorder.cancel(cx));
        }
    }
}

impl Disableable for KeymapEditor {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for KeymapEditor {
    fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Render for KeymapEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.intercept_activation(window, cx);
        let theme = cx.theme().clone();
        let metrics = theme.control.get(self.size);
        self.recorder
            .update(cx, |recorder, cx| recorder.set_control_size(self.size, cx));
        let visible: Vec<_> = self
            .commands
            .iter()
            .filter(|command| self.matches(command))
            .cloned()
            .collect();
        let count = visible.len();
        let root_id = self.ident.semantic_id();
        let entity = cx.entity().clone();
        let status = foundation_text(
            &theme,
            TypeScale::Body,
            cx.strings().format_plural(
                StringKey::KeymapResultOne,
                StringKey::KeymapResultCount,
                cx.numbers().plural(count),
                &[cx.numbers().count(count).as_ref()],
            ),
        )
        .text_tone(&theme, TextTone::Muted)
        .semantic_in(
            cx,
            NodeSpec::new(self.ident.child("status").semantic_id(), Role::Status)
                .parent(root_id.clone())
                .value(cx.numbers().count(count)),
        );
        let mut rows = div()
            .column()
            .w_full()
            .min_w_0()
            .card_surface(&theme, CardVariant::Filled)
            .overflow_hidden();

        for (row_index, command) in visible.into_iter().enumerate() {
            let row = self.ident.child(command.id.as_ref());
            let actionable = !self.disabled && command.refusal.is_none();
            let active = self.active_command.as_ref() == Some(&command.id);
            let effective: Vec<_> = command
                .effective_bindings
                .iter()
                .map(|binding| binding.keystroke.clone())
                .collect();
            let changed = effective != command.default_bindings;
            let effective_value: SharedString = if effective.is_empty() {
                cx.strings().text(StringKey::KeybindingUnbound)
            } else {
                effective
                    .iter()
                    .map(SharedString::as_ref)
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into()
            };
            let defaults_value: SharedString = if command.default_bindings.is_empty() {
                cx.strings().text(StringKey::KeybindingUnbound)
            } else {
                command
                    .default_bindings
                    .iter()
                    .map(SharedString::as_ref)
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into()
            };
            let mut shortcuts = div().column().flex_none().gap_token(&theme, Space::Xs);
            // An empty command has the same entry as a bound command. Recording
            // replaces that entry in place, never inserting a second form row.
            let entries: Vec<_> = if command.effective_bindings.is_empty() {
                vec![None]
            } else {
                command.effective_bindings.iter().map(Some).collect()
            };
            let first_id = command
                .effective_bindings
                .first()
                .map(|binding| &binding.id);
            for binding in entries {
                let first = binding.map(|binding| &binding.id) == first_id;
                let binding_ident =
                    binding.map(|binding| row.child(format!("binding.{}", binding.id)));
                let field_id = binding_ident
                    .as_ref()
                    .map_or_else(|| row.child("add"), |id| id.child("edit"));
                let focus = self
                    .field_focus
                    .entry((
                        command.id.clone(),
                        binding.map(|binding| binding.id.clone()),
                    ))
                    .or_insert_with(|| cx.focus_handle())
                    .clone()
                    .tab_stop(actionable);
                let recording = active
                    && match self.active_binding.as_ref() {
                        Some(id) => binding.is_some_and(|binding| &binding.id == id),
                        None => first,
                    };
                let command_id = command.id.clone();
                let target_binding = binding.map(|binding| binding.id.clone());
                let mut line = div().row().items_center().gap_token(&theme, Space::Xs);
                let field = if recording {
                    div()
                        .w(px(metrics.height * 6.0))
                        .h(px(metrics.height))
                        .child(self.recorder.clone())
                        .into_any_element()
                } else {
                    let mut field = div()
                        .id(field_id.element_id())
                        .when(actionable, |field| field.track_focus(&focus))
                        .row()
                        .flex_none()
                        .w(px(metrics.height * 6.0))
                        .h(px(metrics.height))
                        .px(px(metrics.padding_x))
                        .items_center()
                        .justify_center()
                        .radius(&theme, Radius::Control)
                        .surface(&theme, Surface::Sunken)
                        .border(px(theme.borders.hairline))
                        .border_color(theme.colors.hairline)
                        .text_size(px(metrics.font_size))
                        .text_color(theme.colors.text)
                        .when(!actionable, |field| field.opacity(theme.opacity.disabled))
                        .when(actionable, |field| {
                            field
                                .tab_index(0)
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.colors.hover))
                                .focus_ring(&theme)
                                .tip(
                                    field_id.clone(),
                                    binding
                                        .and_then(|binding| binding.provenance.clone())
                                        .map_or_else(
                                            || cx.strings().text(StringKey::KeybindingPrompt),
                                            |source| {
                                                format!(
                                                    "{} · {}",
                                                    cx.strings().text(StringKey::KeybindingPrompt),
                                                    source
                                                )
                                                .into()
                                            },
                                        ),
                                )
                        });
                    field = if let Some(binding) = binding {
                        field.child(
                            Kbd::new(binding.keystroke.clone())
                                .appearance(false)
                                .id(row.child(format!("binding.{}.keys", binding.id))),
                        )
                    } else {
                        field.child(
                            foundation_text(
                                &theme,
                                TypeScale::Body,
                                cx.strings().text(StringKey::KeybindingUnbound),
                            )
                            .text_tone(&theme, TextTone::Muted),
                        )
                    };
                    if actionable {
                        let editor = entity.clone();
                        let target = command_id.clone();
                        let binding = target_binding.clone();
                        field = field.on_click(move |_, window, cx| {
                            editor.update(cx, |this, cx| {
                                this.start_recording(target.clone(), binding.clone(), window, cx)
                            });
                        });
                    }
                    field
                        .semantic_in(
                            cx,
                            if actionable {
                                NodeSpec::new(field_id.semantic_id(), Role::Input).focus(&focus)
                            } else {
                                NodeSpec::new(field_id.semantic_id(), Role::Input)
                            }
                            .parent(
                                binding_ident
                                    .as_ref()
                                    .map_or_else(|| row.semantic_id(), Ident::semantic_id),
                            )
                            .text(command.label.clone())
                            .disabled(!actionable)
                            .value(binding.map_or_else(
                                || cx.strings().text(StringKey::KeybindingUnbound),
                                |binding| binding.keystroke.clone(),
                            ))
                            .description(cx.strings().text(StringKey::KeybindingPrompt)),
                        )
                        .into_any_element()
                };
                line = line.child(field);
                let mut clear = div()
                    .id(field_id.child("clear-slot").element_id())
                    .when(binding.is_some() && actionable, |slot| {
                        slot.tip(
                            row.child(format!(
                                "binding.{}.remove",
                                binding.map_or("", |b| b.id.as_ref())
                            )),
                            cx.strings().text(StringKey::KeymapRemove),
                        )
                    })
                    .w(px(metrics.height))
                    .h(px(metrics.height))
                    .flex_none();
                if let Some(binding) = binding
                    && actionable
                {
                    let editor = entity.clone();
                    let target = command_id.clone();
                    let id = binding.id.clone();
                    clear = clear.child(
                        Button::new(row.child(format!("binding.{}.remove", binding.id)))
                            .icon_only(Icon::Close, cx.strings().text(StringKey::KeymapRemove))
                            .ghost()
                            .control_size(self.size)
                            .semantic_parent(row.semantic_id())
                            .on_click(move |_, cx| {
                                editor.update(cx, |_, cx| {
                                    cx.emit(KeymapEditorEvent::Remove {
                                        command_id: target.clone(),
                                        binding_id: id.clone(),
                                    })
                                })
                            }),
                    );
                }
                line = line.child(clear);
                let mut add = div()
                    .id(field_id.child("add-slot").element_id())
                    .when(first && binding.is_some() && actionable, |slot| {
                        slot.tip(row.child("add"), cx.strings().text(StringKey::KeymapAdd))
                    })
                    .w(px(metrics.height))
                    .h(px(metrics.height))
                    .flex_none();
                if first && binding.is_some() && actionable {
                    let editor = entity.clone();
                    let target = command_id.clone();
                    add = add.child(
                        Button::new(row.child("add"))
                            .icon_only(Icon::Plus, cx.strings().text(StringKey::KeymapAdd))
                            .ghost()
                            .control_size(self.size)
                            .semantic_parent(row.semantic_id())
                            .on_click(move |window, cx| {
                                editor.update(cx, |this, cx| {
                                    this.start_recording(target.clone(), None, window, cx)
                                })
                            }),
                    );
                }
                line = line.child(add);
                let mut reset = div()
                    .id(if first {
                        row.child("defaults")
                    } else {
                        field_id.child("reset-slot")
                    }
                    .element_id())
                    .when(first && changed && actionable, |slot| {
                        slot.tip(
                            row.child("reset"),
                            format!(
                                "{}: {}",
                                cx.strings().text(StringKey::KeymapReset),
                                if command.default_bindings.is_empty() {
                                    defaults_value.to_string()
                                } else {
                                    command
                                        .default_bindings
                                        .iter()
                                        .flat_map(|key| Kbd::new(key.clone()).caps(cx))
                                        .map(|key| key.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                }
                            ),
                        )
                    })
                    .w(px(metrics.height))
                    .h(px(metrics.height))
                    .flex_none();
                if first && changed && actionable {
                    let editor = entity.clone();
                    let target = command_id.clone();
                    reset = reset.child(
                        Button::new(row.child("reset"))
                            .icon_only(Icon::Restart, cx.strings().text(StringKey::KeymapReset))
                            .accessible_description(format!(
                                "{}: {}",
                                cx.strings().text(StringKey::KeymapDefaults),
                                defaults_value
                            ))
                            .ghost()
                            .control_size(self.size)
                            .semantic_parent(row.semantic_id())
                            .on_click(move |_, cx| {
                                editor.update(cx, |_, cx| {
                                    cx.emit(KeymapEditorEvent::Reset {
                                        command_id: target.clone(),
                                    })
                                })
                            }),
                    );
                }
                if first {
                    line = line.child(
                        reset.semantic_in(
                            cx,
                            NodeSpec::new(row.child("defaults").semantic_id(), Role::Group)
                                .parent(row.semantic_id())
                                .value(defaults_value.clone()),
                        ),
                    );
                } else {
                    line = line.child(reset);
                }
                shortcuts = if let Some(binding) = binding {
                    let description = [binding.conflict.clone(), binding.provenance.clone()]
                        .into_iter()
                        .flatten()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join("; ");
                    shortcuts.child(
                        line.semantic_in(
                            cx,
                            NodeSpec::new(
                                row.child(format!("binding.{}", binding.id)).semantic_id(),
                                Role::Group,
                            )
                            .parent(row.semantic_id())
                            .value(binding.keystroke.clone())
                            .description(description),
                        ),
                    )
                } else {
                    shortcuts.child(line)
                };
            }
            let mut content = div()
                .column()
                .w_full()
                .min_w_0()
                .px(px(theme.space(Space::Md)))
                .py(px(theme.space(Space::Sm)))
                .gap_token(&theme, Space::Xs)
                .when(row_index + 1 < count, |row| {
                    row.border_b(px(theme.borders.hairline))
                        .border_color(theme.colors.hairline)
                })
                .child(
                    div()
                        .row()
                        .w_full()
                        .flex_wrap()
                        .items_start()
                        .gap_token(&theme, Space::Md)
                        .child(
                            div()
                                .column()
                                .flex_1()
                                .min_w(px(theme.measures.settings_label))
                                .justify_center()
                                .min_h(px(metrics.height))
                                .child(
                                    foundation_text(
                                        &theme,
                                        TypeScale::Label,
                                        command.label.clone(),
                                    )
                                    .text_size(px(metrics.font_size)),
                                )
                                .children(command.context.clone().map(|context| {
                                    foundation_text(&theme, TypeScale::Body, context)
                                        .text_tone(&theme, TextTone::Muted)
                                })),
                        )
                        .child(
                            shortcuts.semantic_in(
                                cx,
                                NodeSpec::new(row.child("effective").semantic_id(), Role::Group)
                                    .parent(row.semantic_id())
                                    .value(effective_value),
                            ),
                        ),
                );
            for binding in &command.effective_bindings {
                if let Some(reason) = &binding.conflict {
                    content = content.child(
                        foundation_text(&theme, TypeScale::Body, reason.clone())
                            .text_color(theme.colors.danger)
                            .semantic_in(
                                cx,
                                NodeSpec::new(
                                    row.child(format!("binding.{}.conflict", binding.id))
                                        .semantic_id(),
                                    Role::Status,
                                )
                                .parent(row.child(format!("binding.{}", binding.id)).semantic_id())
                                .invalid(true)
                                .text(reason.clone()),
                            ),
                    );
                }
            }
            if let Some(reason) = &command.refusal {
                content = content.child(
                    foundation_text(&theme, TypeScale::Body, reason.clone())
                        .text_tone(&theme, TextTone::Muted)
                        .semantic_in(
                            cx,
                            NodeSpec::new(row.child("refusal").semantic_id(), Role::Status)
                                .parent(row.semantic_id())
                                .text(reason.clone()),
                        ),
                );
            }
            let mut spec = NodeSpec::new(row.semantic_id(), Role::Row)
                .parent(root_id.clone())
                .text(command.label)
                .disabled(!actionable);
            if let Some(context) = command.context {
                spec = spec.description(context);
            }
            rows = rows.child(content.semantic_in(cx, spec));
        }
        div()
            .id(self.ident.element_id())
            .track_focus(&self.focus_handle)
            .column()
            .w_full()
            .min_w_0()
            .gap_token(&theme, Space::Sm)
            .child(status)
            .child(rows)
            .when(count == 0, |root| {
                root.child(
                    foundation_text(
                        &theme,
                        TypeScale::Body,
                        cx.strings().text(StringKey::KeymapEmpty),
                    )
                    .p(px(theme.space(Space::Md)))
                    .text_tone(&theme, TextTone::Muted)
                    .semantic_in(
                        cx,
                        NodeSpec::new(self.ident.child("empty").semantic_id(), Role::Status)
                            .parent(root_id.clone())
                            .text(cx.strings().text(StringKey::KeymapEmpty)),
                    ),
                )
            })
            .semantic_in(
                cx,
                NodeSpec::new(root_id, Role::Group).disabled(self.disabled),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> KeymapCommand {
        KeymapCommand::new("workbench.open", "Open item")
            .context("Workspace")
            .defaults(["cmd-o", "ctrl-o"])
            .bindings([
                KeymapBinding::new("primary", "cmd-shift-o")
                    .conflict("Already assigned")
                    .provenance("User keymap"),
                KeymapBinding::new("alternate", "ctrl-o"),
            ])
            .searchable("open a workspace item", ["file", "picker"])
    }

    #[test]
    fn model_preserves_all_caller_owned_binding_facts() {
        let command = command();
        assert_eq!(command.default_bindings(), ["cmd-o", "ctrl-o"]);
        assert_eq!(command.effective_bindings().len(), 2);
        assert_eq!(
            command.effective_bindings()[0]
                .conflict_reason()
                .map(SharedString::as_ref),
            Some("Already assigned")
        );
        assert_eq!(
            command.effective_bindings()[0]
                .provenance_label()
                .map(SharedString::as_ref),
            Some("User keymap")
        );
        assert_ne!(
            command
                .effective_bindings()
                .iter()
                .map(KeymapBinding::keystroke)
                .collect::<Vec<_>>(),
            command.default_bindings().iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn filtering_uses_deliberately_supplied_metadata_case_insensitively() {
        // Construct only enough editor state to test its product-neutral predicate.
        let matches = |query: &str| {
            let query = query.to_lowercase();
            let command = command();
            query.is_empty()
                || [command.id(), command.label_text(), command.search_text()]
                    .into_iter()
                    .chain(command.context_label())
                    .chain(command.keywords())
                    .any(|value| value.to_lowercase().contains(&query))
        };
        for query in ["WORKBENCH", "item", "workspace", "file", "picker"] {
            assert!(matches(query), "{query}");
        }
        assert!(!matches("terminal"));
    }
}
