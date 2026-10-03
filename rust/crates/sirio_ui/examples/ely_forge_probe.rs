//! Native probe for the Ely components the change-request surfaces use:
//! `Tabs`, `GitStatusBadge` and `DiffStat`, with Sirio's theme, on Sirio's
//! GPUI packages. Driven by `Scripts/Tests/test-ely-forge-probe.sh`.
//!
//! The script clicks at coordinates, so the layout is a contract: a 900×600
//! window, 16 px padding, 16 px between rows; the badges at y 16–56, the diff
//! stats at y 72–112, then three tab strips in boxes of fixed height — the
//! main one (with the panel) at y 128, one rotated so Checks comes first at
//! y 300, one rotated so the disabled Files comes first at y 400. The first
//! tab of a strip starts at x 16, so x 30 is inside it at any interface size.
//!
//! stdout: `probe tab: <value>` for every `on_change`.
//! Env: `ELY_PROBE_APPEARANCE` (`light`, otherwise dark), `ELY_PROBE_UI_SIZE`.
//!
//! `Tabs` panics if `selected` names no tab: `selected` here only ever holds
//! a value taken from `TABS`, and every strip carries all of `TABS`.
use ely_gpui_component::{
    forms::Choice,
    git::{DiffStat, GitStatus, GitStatusBadge},
    navigation::Tabs,
    primitives::IconName,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    App, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};

/// (value, label, icon, note, disabled). Files is disabled so the script can
/// prove that a disabled tab ignores a click.
const TABS: [(&str, &str, IconName, &str, bool); 4] = [
    (
        "conversation",
        "Conversation",
        IconName::GitPullRequest,
        "3",
        false,
    ),
    (
        "commits",
        "Commits",
        IconName::GitCommitHorizontal,
        "5",
        false,
    ),
    ("checks", "Checks", IconName::CircleCheck, "3/7", false),
    ("files", "Files", IconName::FileText, "12", true),
];

const STATUSES: [GitStatus; 6] = [
    GitStatus::Modified,
    GitStatus::Added,
    GitStatus::Deleted,
    GitStatus::Untracked,
    GitStatus::Renamed,
    GitStatus::Conflicted,
];

/// Nothing changed, only added, only removed, a mix, a sliver, and counts
/// wide enough to show whether the row wraps.
const STATS: [(usize, usize); 6] = [(0, 0), (10, 0), (0, 10), (120, 45), (1, 9), (123_456, 7)];

struct Probe {
    selected: SharedString,
}

impl Probe {
    /// Every tab, starting from `TABS[first]`.
    fn choices(first: usize) -> Vec<Choice> {
        let mut choices: Vec<Choice> = TABS
            .iter()
            .map(|(value, label, icon, note, disabled)| {
                let choice = Choice::new(*value, *label).icon(*icon).note(*note);
                if *disabled { choice.disabled() } else { choice }
            })
            .collect();
        choices.rotate_left(first);
        choices
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (colors, font, base) = {
            let theme = cx.theme();
            (
                theme.colors.clone(),
                theme.font_family.clone(),
                theme.text_size(TextSize::Base),
            )
        };
        let owner = cx.entity();
        let selected = self.selected.clone();
        let strip = |id: &'static str, first: usize| {
            let owner = owner.clone();
            Tabs::new(id, Self::choices(first), selected.clone()).on_change(move |value, _, cx| {
                println!("probe tab: {value}");
                owner.update(cx, |probe, cx| {
                    probe.selected = value.clone();
                    cx.notify();
                });
            })
        };
        let row = || div().h(px(40.0)).flex().items_center().gap_3();
        div()
            .size_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .bg(colors.bg)
            .text_color(colors.fg)
            .font_family(font)
            .text_size(base)
            .child(
                row().children(
                    STATUSES
                        .iter()
                        .enumerate()
                        .map(|(ix, status)| GitStatusBadge::new(("badge", ix), *status)),
                ),
            )
            .child(
                row().children(
                    STATS
                        .iter()
                        .map(|(added, removed)| DiffStat::new(*added, *removed)),
                ),
            )
            .child(
                div().h(px(156.0)).child(
                    strip("probe-tabs", 0).panel(
                        div()
                            .p_3()
                            .text_color(colors.fg_muted)
                            .child(format!("Panel of {selected}")),
                    ),
                ),
            )
            .child(div().h(px(84.0)).child(strip("probe-tabs-checks", 2)))
            .child(div().h(px(84.0)).child(strip("probe-tabs-files", 3)))
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(sirio_ui::ely::AppAssets)
        .run(|cx: &mut App| {
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            sirio_theme::Theme::init(cx);
            sirio_ui::chat::init(cx);
            if std::env::var("ELY_PROBE_APPEARANCE").as_deref() == Ok("light") {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Light, cx);
            } else {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Dark, cx);
            }
            if let Some(size) = std::env::var("ELY_PROBE_UI_SIZE")
                .ok()
                .and_then(|size| size.parse::<i32>().ok())
            {
                sirio_theme::Theme::set_interface_font_size(size, cx);
            }
            // Theme observers run when this closure returns; Ely's palette must
            // already follow the mode and size chosen above when the window draws.
            sirio_ui::ely::init(cx);
            let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Ely Forge Probe");
                    cx.new(|_| Probe {
                        selected: TABS[0].0.into(),
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
