//! Rendering a keyboard shortcut the way the platform writes it.

use gpui::{
    Action, App, AsKeystroke, FocusHandle, IntoElement, KeyContext, Keystroke, RenderOnce,
    SharedString, Window, div, prelude::*, px, relative,
};
use gpui_kit_semantics::{NodeSpec, Role, Semantic};

use crate::foundation::{ActiveTheme, Ident};
use crate::strings::{ActiveStrings, StringKey, Strings};

/// A compact tag that displays a platform-formatted keyboard shortcut.
///
/// The shortcut is rendered in one pill. macOS uses its conventional modifier
/// glyphs, while other platforms spell modifiers out and join them with `+`.
#[derive(Debug, Clone, IntoElement)]
pub struct Kbd {
    keystroke: SharedString,
    ident: Option<Ident>,
    appearance: bool,
    outline: bool,
}

impl From<Keystroke> for Kbd {
    fn from(stroke: Keystroke) -> Self {
        Self::new(stroke.unparse())
    }
}

impl Kbd {
    /// Takes a GPUI keystroke such as `cmd-shift-p`.
    pub fn new(keystroke: impl Into<SharedString>) -> Self {
        Self {
            keystroke: keystroke.into(),
            ident: None,
            appearance: true,
            outline: false,
        }
    }

    /// Publishes the shortcut, for hints a test needs to assert. A shortcut
    /// shown next to the action it belongs to is decorative and needs no id.
    pub fn id(mut self, ident: impl Into<Ident>) -> Self {
        self.ident = Some(ident.into());
        self
    }

    /// Controls whether the shortcut has its compact key-cap appearance.
    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    /// Draws the key cap with a quiet outline instead of a filled background.
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Returns the first binding for an action in the current focus context.
    pub fn binding_for_action(
        action: &dyn Action,
        context: Option<&str>,
        window: &Window,
    ) -> Option<Self> {
        let key_context = context.and_then(|context| KeyContext::parse(context).ok());
        let binding = match key_context {
            Some(context) => {
                window.highest_precedence_binding_for_action_in_context(action, context)
            }
            None => window.highest_precedence_binding_for_action(action),
        }?;
        binding
            .keystrokes()
            .first()
            .map(|key| Self::from(key.as_keystroke().clone()))
    }

    /// Returns the first binding for an action as if `focus_handle` were focused.
    pub fn binding_for_action_in(
        action: &dyn Action,
        focus_handle: &FocusHandle,
        window: &Window,
    ) -> Option<Self> {
        window
            .highest_precedence_binding_for_action_in(action, focus_handle)?
            .keystrokes()
            .first()
            .map(|key| Self::from(key.as_keystroke().clone()))
    }

    /// Formats a parsed keystroke for the current platform.
    pub fn format(keystroke: &Keystroke) -> String {
        format_for_platform(keystroke, cfg!(target_os = "macos"), &Strings::new())
    }

    /// Returns the one compact label drawn by this component.
    ///
    /// This retains the former `caps` query for source compatibility even
    /// though the component no longer draws one cap per modifier.
    pub fn caps(&self, cx: &App) -> Vec<SharedString> {
        caps(
            self.keystroke.as_ref(),
            cfg!(target_os = "macos"),
            cx.strings(),
        )
    }
}

impl RenderOnce for Kbd {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let label = self
            .caps(cx)
            .into_iter()
            .next()
            .unwrap_or_else(|| self.keystroke.clone());
        let published = self
            .ident
            .as_ref()
            .map(|ident| NodeSpec::new(ident.semantic_id(), Role::Text).text(label.clone()));

        let element = if self.appearance {
            div()
                .text_color(theme.colors.text_muted)
                .bg(theme.colors.hover)
                .when(self.outline, |element| {
                    element
                        .border(px(theme.borders.hairline))
                        .border_color(theme.colors.hairline)
                        .bg(theme.colors.canvas)
                })
                .py(px(theme.spacing.xxs))
                .px(px(theme.spacing.xs))
                .min_w(px(theme.control.xs.height))
                .text_center()
                .rounded(px(theme.radii.control / 2.0))
                .line_height(relative(1.0))
                .text_size(px(theme.control.xs.font_size))
                .whitespace_normal()
                .flex_shrink_0()
                .font_fallbacks(gpui_kit_assets::key_fallbacks())
                .child(label)
        } else {
            div().child(label)
        };

        match published {
            Some(spec) => element.semantic_in(cx, spec).into_any_element(),
            None => element.into_any_element(),
        }
    }
}

/// Formats one GPUI keystroke as the single label drawn by [`Kbd`].
///
/// Invalid input remains visible rather than being silently converted into an
/// empty shortcut. The `macos` argument keeps platform formatting testable on
/// every host.
pub fn caps(keystroke: &str, macos: bool, strings: &Strings) -> Vec<SharedString> {
    if keystroke.is_empty() {
        return Vec::new();
    }

    match Keystroke::parse(keystroke) {
        Ok(keystroke) => vec![format_for_platform(&keystroke, macos, strings).into()],
        Err(_) => vec![SharedString::from(keystroke.to_owned())],
    }
}

fn format_for_platform(keystroke: &Keystroke, macos: bool, strings: &Strings) -> String {
    let mut parts = Vec::new();

    // This is the order users see in platform shortcut notation: ⌃⌥⇧⌘ on
    // macOS and Ctrl+Alt+Shift+Win elsewhere.
    if keystroke.modifiers.control {
        parts.push(if macos {
            "⌃".to_owned()
        } else {
            strings.text(StringKey::KbdControl).to_string()
        });
    }
    if keystroke.modifiers.alt {
        parts.push(if macos {
            "⌥".to_owned()
        } else {
            strings.text(StringKey::KbdAlt).to_string()
        });
    }
    if keystroke.modifiers.shift {
        parts.push(if macos {
            "⇧".to_owned()
        } else {
            strings.text(StringKey::KbdShift).to_string()
        });
    }
    if keystroke.modifiers.platform {
        parts.push(if macos {
            "⌘".to_owned()
        } else {
            strings.text(StringKey::KbdSuper).to_string()
        });
    }
    if keystroke.modifiers.function {
        parts.push(strings.text(StringKey::KbdFunction).to_string());
    }

    parts.push(key_label(&keystroke.key, macos, strings));
    parts.join(if macos { "" } else { "+" })
}

fn key_label(key: &str, macos: bool, strings: &Strings) -> String {
    match (key, macos) {
        ("ctrl" | "control", true) => "⌃".into(),
        ("ctrl" | "control", false) => strings.text(StringKey::KbdControl).to_string(),
        ("alt" | "option", true) => "⌥".into(),
        ("alt" | "option", false) => strings.text(StringKey::KbdAlt).to_string(),
        ("shift", true) => "⇧".into(),
        ("shift", false) => strings.text(StringKey::KbdShift).to_string(),
        ("cmd" | "super" | "win" | "platform", true) => "⌘".into(),
        ("cmd" | "super" | "win" | "platform", false) => {
            strings.text(StringKey::KbdSuper).to_string()
        }
        ("function" | "fn", _) => strings.text(StringKey::KbdFunction).to_string(),
        ("space", true) => "␣".into(),
        ("space", false) => strings.text(StringKey::KbdSpace).to_string(),
        ("backspace", true) => "⌫".into(),
        ("backspace", false) => strings.text(StringKey::KbdBackspace).to_string(),
        ("delete", true) => "⌦".into(),
        ("delete", false) => strings.text(StringKey::KbdDelete).to_string(),
        ("escape", true) => "esc".into(),
        ("escape", false) => strings.text(StringKey::KbdEscape).to_string(),
        ("enter", true) => "⏎".into(),
        ("enter", false) => strings.text(StringKey::KbdEnter).to_string(),
        ("pagedown", _) => strings.text(StringKey::KbdPageDown).to_string(),
        ("pageup", _) => strings.text(StringKey::KbdPageUp).to_string(),
        ("tab", true) => "⇥".into(),
        ("tab", false) => strings.text(StringKey::KbdTab).to_string(),
        ("left", true) => "←".into(),
        ("left", false) => strings.text(StringKey::KbdLeft).to_string(),
        ("right", true) => "→".into(),
        ("right", false) => strings.text(StringKey::KbdRight).to_string(),
        ("up", true) => "↑".into(),
        ("up", false) => strings.text(StringKey::KbdUp).to_string(),
        ("down", true) => "↓".into(),
        ("down", false) => strings.text(StringKey::KbdDown).to_string(),
        (other, _) if other.chars().count() == 1 => other.to_uppercase(),
        (other, _) => capitalize(other),
    }
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_uses_one_compact_label_in_platform_order() {
        assert_eq!(
            caps("cmd-ctrl-shift-alt-a", true, &Strings::new()),
            vec![SharedString::from("⌃⌥⇧⌘A")]
        );
    }

    #[test]
    fn other_platforms_use_one_plus_separated_label() {
        assert_eq!(
            caps("cmd-ctrl-shift-alt-a", false, &Strings::new()),
            vec![SharedString::from("Ctrl+Alt+Shift+Win+A")]
        );
    }

    #[test]
    fn named_keys_follow_gpui_component_notation() {
        let strings = Strings::new();
        assert_eq!(
            caps("escape", true, &strings),
            vec![SharedString::from("esc")]
        );
        assert_eq!(
            caps("shift-delete", true, &strings),
            vec![SharedString::from("⇧⌦")]
        );
        assert_eq!(
            caps("alt-left", false, &strings),
            vec![SharedString::from("Alt+Left")]
        );
        assert_eq!(
            caps("shift-space", false, &strings),
            vec![SharedString::from("Shift+Space")]
        );
    }

    #[test]
    fn parser_handles_literal_separator_keys() {
        let strings = Strings::new();
        assert_eq!(
            caps("cmd--", true, &strings),
            vec![SharedString::from("⌘-")]
        );
        assert_eq!(
            caps("cmd-+", true, &strings),
            vec![SharedString::from("⌘+")]
        );
    }

    #[test]
    fn invalid_source_stays_visible_and_empty_source_draws_nothing() {
        assert_eq!(
            caps("ctrl-not-a-key-extra", false, &Strings::new()),
            vec![SharedString::from("ctrl-not-a-key-extra")]
        );
        assert!(caps("", true, &Strings::new()).is_empty());
    }
}
