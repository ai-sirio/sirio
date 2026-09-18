//! A project's settings as a Secondary-pane tab, not a sidebar overlay.
//!
//! This used to be `Sidebar`'s full-sheet `ProjectSettingsCard` overlay.
//! It moved here so the section opens as a tab in the Secondary half of
//! the center split like every other look-at surface. The durable state
//! is unchanged: every edit emits the card's full
//! [`super::ProjectSettingsUpdate`], which the host persists through the
//! project catalog exactly as before, so closing or losing the tab (a
//! worktree switch drops it — settings tabs are ephemeral) never loses
//! anything.

use std::path::PathBuf;

use gpui::{
    Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    PathPromptOptions, PromptLevel, Render, Window, div, prelude::*, px,
};
use sirio_project::display_path;
use sirio_theme::Theme;

use crate::caret;
use crate::project_identity::{ProjectIcon, ProjectIconPicker};

use super::ProjectSettingsUpdate;
use super::SidebarContextAction;
use super::SidebarContextTarget;
use super::icons::{Icon, IconElement, IconSize};

/// The host-seeded snapshot a [`ProjectSettingsView`] is built from. The
/// drafts start here; row facts (`is_git`/`path`/`primary_branch`) can be
/// re-seeded later through [`ProjectSettingsView::refresh_row_facts`].
#[derive(Clone, Debug)]
pub struct ProjectSettingsSeed {
    pub id: String,
    pub base_name: String,
    pub display_name: String,
    pub path: PathBuf,
    pub is_git: bool,
    pub icon: ProjectIcon,
    pub primary_branch: Option<String>,
    pub default_worktree_base: Option<String>,
    pub worktree_location_override: Option<String>,
}

/// What the host must do on the view's behalf. `Changed` carries the full
/// card state (every edit re-emits everything, so an untouched draft rides
/// along); the rest mirror the sidebar context-menu/host flows the overlay
/// used to reach directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectSettingsEvent {
    Changed(ProjectSettingsUpdate),
    ContextAction {
        target: SidebarContextTarget,
        action: SidebarContextAction,
    },
    RemoveProject(String),
}

pub struct ProjectSettingsView {
    id: String,
    base_name: String,
    display_name: String,
    display_name_focus: FocusHandle,
    path: PathBuf,
    is_git: bool,
    icon: ProjectIcon,
    icon_picker: Entity<ProjectIconPicker>,
    /// F-PRJ-17: the pinned base branch draft; empty means "follow the
    /// primary worktree" (`primary_branch`).
    default_worktree_base: String,
    worktree_base_focus: FocusHandle,
    /// The primary worktree's branch, snapshotted when the tab opens —
    /// display-only, used for the "Following primary (…)" subtitle.
    primary_branch: Option<String>,
    /// F-PRJ-18: the checkout-location override draft; empty means "the
    /// project's sibling directory" (`path`'s parent).
    worktree_location_override: String,
    worktree_location_focus: FocusHandle,
    /// One blink shared by this tab's fields: window focus is unique, so
    /// at most one caret is ever visible.
    field_blink: caret::Blink,
    /// Transient surface error (e.g. the folder picker unavailable).
    notice: Option<String>,
}

impl ProjectSettingsView {
    pub fn new(seed: ProjectSettingsSeed, cx: &mut Context<Self>) -> Self {
        let entity = cx.entity();
        let icon_picker = cx.new(|cx| {
            ProjectIconPicker::with_value_and_repo(seed.icon.clone(), &seed.path, cx)
                .on_change_with_context(move |value, cx| {
                    entity.update(cx, |view, cx| {
                        view.apply_icon_change(value, cx);
                    });
                })
        });
        Self {
            id: seed.id,
            base_name: seed.base_name,
            display_name: seed.display_name,
            display_name_focus: cx.focus_handle(),
            path: seed.path,
            is_git: seed.is_git,
            icon: seed.icon,
            icon_picker,
            default_worktree_base: seed.default_worktree_base.unwrap_or_default(),
            worktree_base_focus: cx.focus_handle(),
            primary_branch: seed.primary_branch,
            worktree_location_override: seed.worktree_location_override.unwrap_or_default(),
            worktree_location_focus: cx.focus_handle(),
            field_blink: caret::Blink::new(),
            notice: None,
        }
    }

    pub fn project_id(&self) -> &str {
        &self.id
    }

    /// The tab title's name half for a seed, before any view exists.
    pub fn title_name(seed: &ProjectSettingsSeed) -> String {
        if seed.display_name.trim().is_empty() {
            seed.base_name.clone()
        } else {
            seed.display_name.clone()
        }
    }

    /// The tab title's name half: the display draft, or the base name.
    pub fn heading_name(&self) -> String {
        if self.display_name.trim().is_empty() {
            self.base_name.clone()
        } else {
            self.display_name.clone()
        }
    }

    /// Re-seeds only the row facts a catalog refresh may have changed
    /// (e.g. "Initialize Git" flipping a folder into a repo). Drafts are
    /// never touched: they are already newer than anything persisted.
    pub fn refresh_row_facts(
        &mut self,
        is_git: bool,
        path: PathBuf,
        primary_branch: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.is_git = is_git;
        self.path = path;
        self.primary_branch = primary_branch;
        cx.notify();
    }

    fn to_update(&self) -> ProjectSettingsUpdate {
        ProjectSettingsUpdate {
            id: self.id.clone(),
            display_name: {
                let value = self.display_name.trim().to_string();
                (!value.is_empty()).then_some(value)
            },
            is_git: self.is_git,
            icon: self.icon.clone(),
            default_worktree_base: {
                let value = self.default_worktree_base.trim().to_string();
                (!value.is_empty()).then_some(value)
            },
            worktree_location_override: {
                let value = self.worktree_location_override.trim().to_string();
                (!value.is_empty()).then_some(value)
            },
        }
    }

    fn emit_changed(&self, cx: &mut Context<Self>) {
        cx.emit(ProjectSettingsEvent::Changed(self.to_update()));
    }

    fn apply_icon_change(&mut self, icon: ProjectIcon, cx: &mut Context<Self>) {
        self.icon = icon;
        self.emit_changed(cx);
        cx.notify();
    }

    fn push_keystroke(draft: &mut String, event: &KeyDownEvent) {
        match event.keystroke.key.as_str() {
            "backspace" | "delete" => {
                draft.pop();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    draft.push_str(character);
                }
            }
        }
    }

    fn on_display_name_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        Self::push_keystroke(&mut self.display_name, event);
        self.emit_changed(cx);
        cx.notify();
    }

    /// F-PRJ-17: keystrokes typed into the "Default Worktree Base" field.
    fn on_worktree_base_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        Self::push_keystroke(&mut self.default_worktree_base, event);
        self.emit_changed(cx);
        cx.notify();
    }

    /// F-PRJ-17: "Use Primary" clears the pin, restoring the "follow the
    /// primary worktree" fallback.
    fn use_primary_worktree_base(&mut self, cx: &mut Context<Self>) {
        self.default_worktree_base.clear();
        self.emit_changed(cx);
        cx.notify();
    }

    /// F-PRJ-18: keystrokes typed into the "Worktree Location" field.
    fn on_worktree_location_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        Self::push_keystroke(&mut self.worktree_location_override, event);
        self.emit_changed(cx);
        cx.notify();
    }

    /// F-PRJ-18: "Restore Default" clears the override, restoring the
    /// project's sibling directory as the parent for new worktrees.
    fn restore_default_worktree_location(&mut self, cx: &mut Context<Self>) {
        self.worktree_location_override.clear();
        self.emit_changed(cx);
        cx.notify();
    }

    /// Blink timer tick for this tab's text fields.
    fn flip_field_blink(&mut self, cx: &mut Context<Self>) {
        self.field_blink.flip();
        cx.notify();
    }

    /// F-PRJ-18: "Choose…" opens the same platform folder picker project
    /// creation uses and writes the chosen path into the draft.
    fn choose_worktree_location(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder for new worktrees".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let outcome = receiver.await;
            let _ = view.update(cx, |view, cx| {
                let chosen: Option<PathBuf> = match outcome {
                    // A real selection. Asked for one directory, so take
                    // the last and ignore any surprise extras.
                    Ok(Ok(Some(mut found))) => found.pop(),
                    // Cancelled, empty selection, or the window went away
                    // with the prompt still open: nothing to do.
                    Ok(Ok(None)) | Err(_) => None,
                    // The platform could not open a chooser at all — never
                    // silent: a control that is drawn, clicked, and then
                    // does nothing is indistinguishable from a broken app.
                    Ok(Err(error)) => {
                        view.notice = Some(format!("could not open the folder picker: {error}"));
                        cx.notify();
                        return;
                    }
                };
                if let Some(path) = chosen {
                    view.worktree_location_override = super::Sidebar::worktree_location_text(&path);
                    view.emit_changed(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn request_remove_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project_id = self.id.clone();
        let receiver = window.prompt(
            PromptLevel::Warning,
            "Remove project from Sirio?",
            Some("This only removes the project from Sirio's sidebar. Files on disk will not be deleted."),
            &["Remove from Sirio", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if receiver.await.unwrap_or(1) == 0 {
                let _ = view.update(cx, |_, cx| {
                    cx.emit(ProjectSettingsEvent::RemoveProject(project_id));
                });
            }
        })
        .detach();
    }

    /// F-PRJ-17: "Default Worktree Base" section.
    fn render_worktree_base_section(
        &self,
        entity: &Entity<Self>,
        theme: &Theme,
        focused: bool,
        caret_visible: bool,
    ) -> impl gpui::IntoElement {
        let draft = self.default_worktree_base.clone();
        let effective_base = if !draft.trim().is_empty() {
            draft.clone()
        } else {
            self.primary_branch
                .clone()
                .unwrap_or_else(|| "—".to_string())
        };
        let subtitle = if !draft.trim().is_empty() {
            "Pinned".to_string()
        } else if let Some(branch) = &self.primary_branch {
            format!("Following primary branch ({branch})")
        } else {
            "No primary worktree set".to_string()
        };
        let base_focus = self.worktree_base_focus.clone();
        let focus_entity = entity.clone();
        let key_entity = entity.clone();
        let primary_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Default Worktree Base"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.text)
                                    .child(effective_base),
                            )
                            .child(
                                div()
                                    .text_size(theme.typography.scaled(11.0))
                                    .text_color(theme.text_faint)
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .id("project-worktree-base-use-primary")
                            .debug_selector(|| "project-worktree-base-use-primary".to_owned())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_faint)
                            .hover(|style| style.text_color(theme.text))
                            .on_click(move |_, _, cx| {
                                primary_entity.update(cx, |view, cx| {
                                    view.use_primary_worktree_base(cx);
                                });
                            })
                            .child("Use Primary"),
                    ),
            )
            .child({
                let draft_is_empty = draft.trim().is_empty();
                div()
                    .id("project-worktree-base-field")
                    .debug_selector(|| "project-worktree-base-field".to_owned())
                    .track_focus(&base_focus)
                    .w_full()
                    .h(px(28.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(theme.typography.footnote)
                    .text_color(if draft.trim().is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_entity.update(cx, |view, cx| {
                            view.worktree_base_focus.focus(window, cx);
                        });
                    })
                    .on_key_down(move |event, window, cx| {
                        key_entity.update(cx, |view, cx| {
                            view.on_worktree_base_key(event, window, cx);
                        });
                    })
                    // #212: clip inside the field; must not grow, or the caret leaves the text.
                    .overflow_hidden()
                    .child(
                        if draft_is_empty {
                            caret::field_placeholder(
                                "Search branches by name…".to_owned(),
                                focused.then(|| caret::bar(px(14.0), theme.text, caret_visible)),
                            )
                        } else {
                            caret::field_value(draft)
                        }
                        .id("sidebar-branch-search-text")
                        .debug_selector(|| "sidebar-branch-search-text".to_owned()),
                    )
                    .when(focused && !draft_is_empty, |this| {
                        this.child(caret::bar(px(14.0), theme.text, caret_visible))
                    })
            })
    }

    /// F-PRJ-18: "Worktree Location" section.
    fn render_worktree_location_section(
        &self,
        entity: &Entity<Self>,
        theme: &Theme,
        focused: bool,
        caret_visible: bool,
    ) -> impl gpui::IntoElement {
        let draft = self.worktree_location_override.clone();
        let default_location = self
            .path
            .parent()
            .map(super::Sidebar::worktree_location_text)
            .unwrap_or_else(|| super::Sidebar::worktree_location_text(&self.path));
        let location_focus = self.worktree_location_focus.clone();
        let focus_entity = entity.clone();
        let key_entity = entity.clone();
        let choose_entity = entity.clone();
        let restore_entity = entity.clone();
        let has_override = !draft.trim().is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Worktree Location"),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.0))
                    .text_color(theme.text_faint)
                    .child(format!(
                        "Parent folder for new worktrees. Empty uses the default: {default_location}"
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("project-worktree-location-field")
                            .debug_selector(|| "project-worktree-location-field".to_owned())
                            .track_focus(&location_focus)
                            .flex_1()
                            .h(px(28.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(theme.radii.control)
                            .bg(theme.input_bg)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.typography.footnote)
                            .text_color(if has_override { theme.text } else { theme.text_faint })
                            .cursor(gpui::CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus_entity.update(cx, |view, cx| {
                                    view.worktree_location_focus.focus(window, cx);
                                });
                            })
                            .on_key_down(move |event, window, cx| {
                                key_entity.update(cx, |view, cx| {
                                    view.on_worktree_location_key(event, window, cx);
                                });
                            })
                            // #212: this one renders a filesystem path, so it is the likeliest to overflow.
                            .overflow_hidden()
                            .child(
                                caret::field_value(if has_override {
                                    draft
                                } else {
                                    default_location.clone()
                                })
                                .id("sidebar-location-override-text")
                                .debug_selector(|| "sidebar-location-override-text".to_owned()),
                            )
                            .when(focused, |this| {
                                this.child(caret::bar(px(14.0), theme.text, caret_visible))
                            }),
                    )
                    .child(
                        div()
                            .id("project-worktree-location-choose")
                            .debug_selector(|| "project-worktree-location-choose".to_owned())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .px(px(8.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme.radii.control)
                            .bg(theme.surface_raised)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text)
                            .on_click(move |_, window, cx| {
                                choose_entity.update(cx, |view, cx| {
                                    view.choose_worktree_location(window, cx);
                                });
                            })
                            .child("Choose…"),
                    ),
            )
            .when(has_override, |this| {
                this.child(
                    div()
                        .id("project-worktree-location-restore")
                        .debug_selector(|| "project-worktree-location-restore".to_owned())
                        .cursor(gpui::CursorStyle::PointingHand)
                        .text_size(theme.typography.scaled(11.0))
                        .text_color(theme.text_faint)
                        .hover(|style| style.text_color(theme.text))
                        .on_click(move |_, _, cx| {
                            restore_entity.update(cx, |view, cx| {
                                view.restore_default_worktree_location(cx);
                            });
                        })
                        .child("Restore Default"),
                )
            })
    }
}

impl EventEmitter<ProjectSettingsEvent> for ProjectSettingsView {}

impl Render for ProjectSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let theme = *Theme::get(cx);
        if cx.try_global::<bezel::theme::Theme>().is_none() {
            theme.install_into_bezel(cx);
        }
        let entity = cx.entity();
        let name_entity = entity.clone();
        let focus_entity = entity.clone();
        let initialize_entity = entity.clone();
        let remove_entity = entity.clone();
        let heading_name = self.heading_name();
        let display_name = self.display_name.clone();
        let name_focused = self.display_name_focus.is_focused(window);
        let base_focused = self.worktree_base_focus.is_focused(window);
        let location_focused = self.worktree_location_focus.is_focused(window);
        let field_focused = name_focused || base_focused || location_focused;
        caret::schedule(
            &mut self.field_blink,
            field_focused,
            Self::flip_field_blink,
            cx,
        );
        let caret_visible = field_focused && self.field_blink.visible();
        let display_name_focus = self.display_name_focus.clone();
        let is_git = self.is_git;
        let target = SidebarContextTarget::Project {
            id: self.id.clone(),
            path: self.path.clone(),
            is_git: self.is_git,
        };
        div()
            .id("project-settings-tab")
            .debug_selector(|| "project-settings-tab".to_owned())
            .size_full()
            .overflow_y_scroll()
            .p(px(16.0))
            .bg(theme.surface)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(format!("Project Settings · {heading_name}")),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(display_path(&self.path)),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .child(if is_git {
                        "Repository: Git"
                    } else {
                        "Repository: Folder"
                    }),
            )
            .child({
                let name_is_empty = display_name.trim().is_empty();
                div()
                    .id("project-display-name-field")
                    .debug_selector(|| "project-display-name-field".to_owned())
                    .track_focus(&display_name_focus)
                    .w_full()
                    .h(px(32.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(theme.typography.footnote)
                    .text_color(if display_name.trim().is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_entity.update(cx, |view, cx| {
                            view.display_name_focus.focus(window, cx);
                        });
                    })
                    .on_key_down(move |event, window, cx| {
                        name_entity.update(cx, |view, cx| {
                            view.on_display_name_key(event, window, cx);
                        });
                    })
                    // #212: clip inside the field; must not grow, or the caret leaves the text.
                    .overflow_hidden()
                    .child(
                        if name_is_empty {
                            caret::field_placeholder(
                                "Display name".to_owned(),
                                name_focused
                                    .then(|| caret::bar(px(14.0), theme.text, caret_visible)),
                            )
                        } else {
                            caret::field_value(display_name)
                        }
                        .id("sidebar-display-name-text")
                        .debug_selector(|| "sidebar-display-name-text".to_owned()),
                    )
                    .when(name_focused && !name_is_empty, |this| {
                        this.child(caret::bar(px(14.0), theme.text, caret_visible))
                    })
            })
            .when(!is_git, |this| {
                this.child(
                    div()
                        .id("project-settings-initialize-git")
                        .debug_selector(|| "project-settings-initialize-git".to_owned())
                        .h(px(30.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radii.control)
                        .bg(theme.surface_raised)
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text)
                        .on_click(move |_, _, cx| {
                            initialize_entity.update(cx, |_, cx| {
                                cx.emit(ProjectSettingsEvent::ContextAction {
                                    target: target.clone(),
                                    action: SidebarContextAction::InitializeGit,
                                });
                            });
                        })
                        .child("Initialize Git"),
                )
            })
            .child(
                div()
                    .id("project-icon-picker")
                    .debug_selector(|| "project-icon-picker".to_owned())
                    .w_full()
                    .p(px(8.0))
                    .rounded(theme.radii.control)
                    .bg(theme.surface)
                    .child(self.icon_picker.clone()),
            )
            .when(is_git, |this| {
                this.child(self.render_worktree_base_section(
                    &entity,
                    &theme,
                    base_focused,
                    caret_visible,
                ))
                .child(self.render_worktree_location_section(
                    &entity,
                    &theme,
                    location_focused,
                    caret_visible,
                ))
            })
            .when_some(self.notice.clone(), |this, notice| {
                this.child(
                    div()
                        .text_size(theme.typography.scaled(11.0))
                        .text_color(theme.diff_del)
                        .child(notice),
                )
            })
            .child(
                div()
                    .id("project-settings-remove")
                    .debug_selector(|| "project-settings-remove".to_owned())
                    .cursor(gpui::CursorStyle::PointingHand)
                    .mt(px(4.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.diff_del)
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, window, cx| {
                        remove_entity.update(cx, |view, cx| {
                            view.request_remove_project(window, cx);
                        });
                    })
                    .child(
                        IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.diff_del),
                    )
                    .child("Remove Project"),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.0))
                    .text_color(theme.text_faint)
                    .child(self.id.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn seed() -> ProjectSettingsSeed {
        ProjectSettingsSeed {
            id: "project".to_string(),
            base_name: "sirio".to_string(),
            display_name: String::new(),
            path: PathBuf::from("/tmp/sirio"),
            is_git: true,
            icon: ProjectIcon::default(),
            primary_branch: Some("main".to_string()),
            default_worktree_base: None,
            worktree_location_override: None,
        }
    }

    fn mount(
        seed: ProjectSettingsSeed,
        cx: &mut gpui::TestAppContext,
    ) -> (
        gpui::WindowHandle<ProjectSettingsView>,
        gpui::VisualTestContext,
        Entity<ProjectSettingsView>,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| ProjectSettingsView::new(seed, cx));
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let view = cx.update(|window, _| {
            window
                .root::<ProjectSettingsView>()
                .flatten()
                .expect("settings root")
        });
        (window, cx, view)
    }

    #[gpui::test]
    async fn empty_drafts_emit_no_overrides(cx: &mut gpui::TestAppContext) {
        let (window, mut cx, _view) = mount(seed(), cx);
        let update = window
            .update(&mut cx, |view, _, _| view.to_update())
            .unwrap();
        assert_eq!(update.id, "project");
        assert_eq!(update.display_name, None);
        assert_eq!(update.default_worktree_base, None);
        assert_eq!(update.worktree_location_override, None);
    }

    #[gpui::test]
    async fn heading_falls_back_to_base_name(cx: &mut gpui::TestAppContext) {
        let (window, mut cx, _view) = mount(seed(), cx);
        let heading = window
            .update(&mut cx, |view, _, _| view.heading_name())
            .unwrap();
        assert_eq!(heading, "sirio");
        window
            .update(&mut cx, |view, _, cx| {
                view.display_name = "Custom".to_string();
                cx.notify();
            })
            .unwrap();
        let heading = window
            .update(&mut cx, |view, _, _| view.heading_name())
            .unwrap();
        assert_eq!(heading, "Custom");
        assert!(
            cx.debug_bounds("project-settings-tab").is_some(),
            "the settings tab surface is drawn"
        );
    }

    #[gpui::test]
    async fn git_projects_draw_base_and_location_fields(cx: &mut gpui::TestAppContext) {
        let (_window, mut cx, _view) = mount(seed(), cx);
        assert!(
            cx.debug_bounds("project-worktree-base-field").is_some(),
            "a git project's settings tab draws the Default Worktree Base field"
        );
        assert!(
            cx.debug_bounds("project-worktree-location-field").is_some(),
            "a git project's settings tab draws the Worktree Location field"
        );
    }

    #[gpui::test]
    async fn folders_offer_initialize_git_instead(cx: &mut gpui::TestAppContext) {
        let (_window, mut cx, _view) = mount(
            ProjectSettingsSeed {
                id: "folder".to_string(),
                is_git: false,
                ..seed()
            },
            cx,
        );
        assert!(
            cx.debug_bounds("project-worktree-base-field").is_none(),
            "a non-git project has no worktrees, so it offers no base/location controls"
        );
        assert!(
            cx.debug_bounds("project-settings-initialize-git").is_some(),
            "a non-git project offers Initialize Git instead"
        );
    }

    /// F-PRJ-17/F-PRJ-18: typing into either field, and the two clearing
    /// controls ("Use Primary", "Restore Default"), each emit a durable
    /// `Changed` carrying the new value.
    #[gpui::test]
    async fn typing_worktree_base_and_location_emits_a_durable_update(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_window, mut cx, view) = mount(seed(), cx);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &ProjectSettingsEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
        });
        cx.run_until_parked();

        let base_field = cx
            .debug_bounds("project-worktree-base-field")
            .expect("worktree-base field is drawn");
        cx.simulate_click(base_field.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("develop");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ProjectSettingsEvent::Changed(update)
                    if update.default_worktree_base.as_deref() == Some("develop")
            )),
            "typing a base branch emits it on the durable update"
        );

        let location_field = cx
            .debug_bounds("project-worktree-location-field")
            .expect("worktree-location field is drawn");
        cx.simulate_click(location_field.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("/srv/worktrees");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ProjectSettingsEvent::Changed(update)
                    if update.worktree_location_override.as_deref() == Some("/srv/worktrees")
            )),
            "typing a location override emits it on the durable update"
        );

        let use_primary = cx
            .debug_bounds("project-worktree-base-use-primary")
            .expect("Use Primary is drawn");
        cx.simulate_click(use_primary.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ProjectSettingsEvent::Changed(update)
                    if update.default_worktree_base.is_none()
            )),
            "Use Primary clears the pinned base"
        );

        let restore = cx
            .debug_bounds("project-worktree-location-restore")
            .expect("Restore Default is drawn once an override is set");
        cx.simulate_click(restore.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ProjectSettingsEvent::Changed(update)
                    if update.worktree_location_override.is_none()
            )),
            "Restore Default clears the location override"
        );
    }

    #[gpui::test]
    async fn remove_project_confirms_before_emitting(cx: &mut gpui::TestAppContext) {
        let (_window, mut cx, view) = mount(seed(), cx);
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &ProjectSettingsEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        cx.run_until_parked();

        let remove = cx
            .debug_bounds("project-settings-remove")
            .expect("settings tab draws a Remove Project control");
        cx.simulate_click(remove.center(), gpui::Modifiers::none());
        cx.run_until_parked();

        assert!(cx.has_pending_prompt(), "removal asks for confirmation");
        assert!(
            events.borrow().is_empty(),
            "nothing may be emitted before the user answers"
        );

        cx.simulate_prompt_answer("Remove from Sirio");
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ProjectSettingsEvent::RemoveProject(id) if id == "project"
            )),
            "accepting the prompt emits RemoveProject with the project's id"
        );
    }

    #[gpui::test]
    async fn saved_defaults_seed_the_drafts_on_open(cx: &mut gpui::TestAppContext) {
        // A reopen after a live persist seeds from the saved catalog
        // values, so the fields show what was saved.
        let (window, mut cx, _view) = mount(
            ProjectSettingsSeed {
                default_worktree_base: Some("develop".to_string()),
                worktree_location_override: Some("/srv/worktrees".to_string()),
                ..seed()
            },
            cx,
        );
        let update = window
            .update(&mut cx, |view, _, _| view.to_update())
            .unwrap();
        assert_eq!(update.default_worktree_base.as_deref(), Some("develop"));
        assert_eq!(
            update.worktree_location_override.as_deref(),
            Some("/srv/worktrees")
        );
    }

    #[gpui::test]
    async fn refresh_row_facts_never_clobbers_drafts(cx: &mut gpui::TestAppContext) {
        let (window, mut cx, _view) = mount(seed(), cx);
        window
            .update(&mut cx, |view, _, cx| {
                view.display_name = "Custom".to_string();
                view.refresh_row_facts(false, PathBuf::from("/tmp/other"), None, cx);
            })
            .unwrap();
        window
            .update(&mut cx, |view, _, _| {
                assert_eq!(view.display_name, "Custom");
                assert!(!view.is_git);
                assert_eq!(view.path, PathBuf::from("/tmp/other"));
            })
            .unwrap();
    }
}
