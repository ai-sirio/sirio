//! Settings → Git Hosting (spec §5.3): one row per forge host seen this
//! session or configured, with its forge, the means in use and the account;
//! the user can pin a means, say which forge an unknown host runs, and save
//! or forget a token. Everything goes through `forge_source`: the host owns
//! where each thing is kept.

use std::collections::HashMap;

use bezel::ui::input::TextField;
use bezel::ui::widgets::{ButtonStyle, Buttons as _, Content as _, Scaffolding as _};
use gpui::Task;
use sirio_forge::{Forge, Means, known_forge};

use super::*;
use crate::change_request_style as style;
use crate::forge_source::{self, ChangeRequestSource, HostRow, TokenWrite};
use crate::text_selection::selectable_text;

pub(crate) enum GitHostStatus {
    Working,
    Saved(String),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct GitHosting {
    pub(crate) hosts: Vec<HostRow>,
    pub(crate) loaded: bool,
    pub(crate) tokens: HashMap<String, Entity<TextField>>,
    pub(crate) status: HashMap<String, GitHostStatus>,
    load_task: Option<Task<()>>,
    actions: Vec<Task<()>>,
}

impl Settings {
    pub(crate) fn refresh_git_hosts(&mut self, cx: &mut Context<Self>) {
        let Some(source) = forge_source::source(cx) else {
            self.git_hosting.loaded = true;
            cx.notify();
            return;
        };
        self.git_hosting.load_task = Some(cx.spawn(async move |this, cx| {
            let hosts = cx.background_spawn(async move { source.hosts() }).await;
            let _ = this.update(cx, |settings, cx| {
                for row in &hosts {
                    settings
                        .git_hosting
                        .tokens
                        .entry(row.host.clone())
                        .or_insert_with(|| {
                            cx.new(|cx| {
                                TextField::new(cx).with_placeholder("Personal access token")
                            })
                        });
                }
                settings.git_hosting.hosts = hosts;
                settings.git_hosting.loaded = true;
                cx.notify();
            });
        }));
    }

    /// Runs one host action on the background executor, shows its outcome,
    /// tells the host (which reconnects the right panel), and reloads.
    fn git_host_action(
        &mut self,
        host: String,
        action: impl FnOnce(&dyn ChangeRequestSource) -> Result<Option<String>, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        self.git_hosting
            .status
            .insert(host.clone(), GitHostStatus::Working);
        let task = cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { action(&*source) }).await;
            let _ = this.update(cx, |settings, cx| {
                match result {
                    Ok(Some(account)) => {
                        settings
                            .git_hosting
                            .status
                            .insert(host.clone(), GitHostStatus::Saved(account));
                        if let Some(field) = settings.git_hosting.tokens.get(&host) {
                            field.update(cx, |field, cx| field.clear(cx));
                        }
                    }
                    Ok(None) => {
                        settings.git_hosting.status.remove(&host);
                    }
                    Err(why) => {
                        settings
                            .git_hosting
                            .status
                            .insert(host.clone(), GitHostStatus::Failed(why));
                    }
                }
                cx.emit(SettingsEvent::ForgeHostsChanged);
                settings.refresh_git_hosts(cx);
            });
        });
        self.git_hosting.actions.push(task);
        cx.notify();
    }

    pub(crate) fn save_git_host_token(
        &mut self,
        host: String,
        forge: Forge,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.git_hosting.tokens.get(&host) else {
            return;
        };
        let token = field.read(cx).content().to_string();
        if token.trim().is_empty() {
            return;
        }
        let target = host.clone();
        self.git_host_action(
            host,
            move |source| {
                source
                    .save_token(&target, forge, &token)
                    .map(Some)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(crate) fn set_git_host_means(
        &mut self,
        host: String,
        forge: Forge,
        means: Option<Means>,
        cx: &mut Context<Self>,
    ) {
        let target = host.clone();
        self.git_host_action(
            host,
            move |source| {
                source.set_means(&target, forge, means);
                Ok(None)
            },
            cx,
        );
    }

    pub(crate) fn set_git_host_forge(
        &mut self,
        host: String,
        forge: Forge,
        cx: &mut Context<Self>,
    ) {
        let target = host.clone();
        self.git_host_action(
            host,
            move |source| {
                source.set_forge(&target, forge);
                Ok(None)
            },
            cx,
        );
    }

    pub(crate) fn forget_git_host_token(&mut self, host: String, cx: &mut Context<Self>) {
        let target = host.clone();
        self.git_host_action(
            host,
            move |source| {
                source.delete_token(&target);
                Ok(None)
            },
            cx,
        );
    }
}

fn means_text(row: &HostRow) -> String {
    let account = row
        .account
        .as_deref()
        .map(|account| format!(" as {account}"))
        .unwrap_or_default();
    match (row.forge, row.means) {
        (Some(forge), Some(Means::Cli)) => {
            format!("Signed in with {}{account}", style::cli_name(forge))
        }
        (Some(_), Some(Means::Token)) => format!(
            "Token{account}{}",
            match row.write {
                Some(TokenWrite::Yes) => " · can read and write",
                Some(TokenWrite::No) => " · read-only",
                Some(TokenWrite::NotReported) => " · scopes not reported",
                None => "",
            }
        ),
        (Some(_), None) => "Not connected".to_string(),
        (None, _) => "Unknown forge".to_string(),
    }
}

impl Settings {
    pub(crate) fn render_git_hosting(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let bezel_theme = theme.to_bezel_theme();
        let mut card = bezel_theme
            .group_box()
            .debug_selector(|| "settings-git-hosting-card".to_string());
        if self.git_hosting.loaded && self.git_hosting.hosts.is_empty() {
            card = card.child(bezel_theme.empty_state(
                bezel::ui::icons::MAGNIFER,
                "No forge hosts yet",
                "Open the change request view in the right panel on a project with a GitHub or GitLab remote.",
            ));
        }
        for (index, row) in self.git_hosting.hosts.iter().enumerate() {
            let host = row.host.clone();
            let body = div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .when_some(row.forge, |this, forge| {
                    this.child(
                        IconElement::new(style::forge_mark(forge), IconSize::Small)
                            .text_color(theme.text),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(bezel_theme.row_title(host.clone()))
                        .child(
                            div()
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_muted)
                                .child(selectable_text(format!(
                                    "{} · {}",
                                    row.forge.map_or("Unknown forge", Forge::name),
                                    means_text(row)
                                ))),
                        ),
                );
            let mut tail = div().flex().flex_wrap().items_center().gap(px(6.0));
            // The forge of a public host is not a choice.
            if known_forge(&host).is_none() {
                for (label, forge, id) in [
                    ("GitHub", Forge::GitHub, "settings-git-host-forge-github"),
                    ("GitLab", Forge::GitLab, "settings-git-host-forge-gitlab"),
                ] {
                    let chosen = row.forge == Some(forge);
                    let entity = entity.clone();
                    let target = host.clone();
                    tail = tail.child(
                        bezel_theme
                            .button(
                                label,
                                if chosen {
                                    ButtonStyle::Prominent
                                } else {
                                    ButtonStyle::Ghost
                                },
                                None,
                            )
                            .id((id, index))
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |settings, cx| {
                                    settings.set_git_host_forge(target.clone(), forge, cx)
                                })
                            }),
                    );
                }
            }
            if let Some(forge) = row.forge {
                for (label, means, id) in [
                    ("Detect", None, "settings-git-host-means-detect"),
                    ("CLI", Some(Means::Cli), "settings-git-host-means-cli"),
                    ("Token", Some(Means::Token), "settings-git-host-means-token"),
                ] {
                    let chosen = row.pinned_means == means;
                    let entity = entity.clone();
                    let target = host.clone();
                    tail = tail.child(
                        bezel_theme
                            .button(
                                label,
                                if chosen {
                                    ButtonStyle::Prominent
                                } else {
                                    ButtonStyle::Ghost
                                },
                                None,
                            )
                            .id((id, index))
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |settings, cx| {
                                    settings.set_git_host_means(target.clone(), forge, means, cx)
                                })
                            }),
                    );
                }
                if let Some(field) = self.git_hosting.tokens.get(&host) {
                    let entity = entity.clone();
                    let target = host.clone();
                    tail = tail.child(div().w(px(220.0)).child(field.clone())).child(
                        bezel_theme
                            .button("Save token", ButtonStyle::Prominent, None)
                            .id(("settings-git-host-token-save", index))
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |settings, cx| {
                                    settings.save_git_host_token(target.clone(), forge, cx)
                                })
                            }),
                    );
                    tail = tail.child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_muted)
                            .child(selectable_text(format!(
                                "Needs {}",
                                style::token_scopes(forge)
                            ))),
                    );
                }
                if row.has_token {
                    let entity = entity.clone();
                    let target = host.clone();
                    tail = tail.child(
                        bezel_theme
                            .button("Forget token", ButtonStyle::Ghost, None)
                            .id(("settings-git-host-token-forget", index))
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |settings, cx| {
                                    settings.forget_git_host_token(target.clone(), cx)
                                })
                            }),
                    );
                }
            }
            let status = match self.git_hosting.status.get(&host) {
                Some(GitHostStatus::Working) => Some(("Working…".to_string(), theme.text_faint)),
                Some(GitHostStatus::Saved(account)) => {
                    Some((format!("Token saved for {account}"), theme.success))
                }
                Some(GitHostStatus::Failed(why)) => Some((why.clone(), theme.danger)),
                None => None,
            };
            card = card.child(
                bezel_theme
                    .card_row(index == 0)
                    .id(("settings-git-host-row", index))
                    .flex_wrap()
                    .child(body)
                    .child(tail)
                    .when_some(status, |this, (text, tone)| {
                        this.child(
                            div()
                                .w_full()
                                .mt(px(6.0))
                                .text_size(theme.typography.footnote)
                                .text_color(tone)
                                .child(selectable_text(text)),
                        )
                    }),
            );
        }
        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(bezel_theme.page_header("Git Hosting", None))
            .child(bezel_theme.page_subtitle(
                "The forges your projects' remotes live on, and how Sirio signs in to each: the forge's CLI (gh, glab) or a personal access token. Tokens are kept in Sirio's credential store, never in settings.",
            ))
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use sirio_forge::{Forge, ForgeError};

    use super::*;
    use crate::forge_source::testing::FakeSource;
    use crate::forge_source::{self, Connection};

    #[gpui::test]
    fn a_rejected_token_is_explained_and_the_paste_is_handed_over_untouched(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let source = FakeSource::with(Connection::NoForgeRemote);
        *source.token_answer.lock().unwrap() = Err(ForgeError::NotAuthenticated {
            host: "git.corp".into(),
        });
        cx.update(|cx| forge_source::set_source(source.clone(), cx));
        let settings = cx.new(|cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        settings.update(cx, |settings, cx| {
            let field = cx.new(|cx| TextField::new(cx));
            field.update(cx, |field, cx| field.set_content("  glpat-secret \n", cx));
            settings.git_hosting.tokens.insert("git.corp".into(), field);
            settings.save_git_host_token("git.corp".into(), Forge::GitLab, cx);
        });
        cx.executor().allow_parking();
        for _ in 0..600 {
            let failed = settings.read_with(cx, |settings, _| {
                matches!(
                    settings.git_hosting.status.get("git.corp"),
                    Some(GitHostStatus::Failed(_))
                )
            });
            if failed {
                break;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(100));
            cx.run_until_parked();
        }
        settings.read_with(cx, |settings, _| {
            let Some(GitHostStatus::Failed(why)) = settings.git_hosting.status.get("git.corp")
            else {
                panic!("the refusal is shown");
            };
            assert!(why.contains("not signed in to git.corp"), "{why}");
        });
        let tokens = source.tokens.lock().unwrap();
        // NOTE (deviation from the brief's verbatim expectation, per the
        // dispatch's "follow the tree when it differs"): bezel's single-line
        // `TextField` normalizes a pasted newline to a space on `set_content`
        // (`bezel-ui 0.1.4 input.rs`: `normalize` — "a pasted newline becomes
        // a space rather than silently truncating what was pasted"), so by the
        // time the page reads `content()` the trailing `\n` is already a
        // space. The page still hands over exactly what the field holds —
        // leading spaces intact, i.e. nothing trimmed page-side; the host
        // (Task 3) owns trimming before verify/store. Same single-line shape
        // as the right panel's own token field (`change_requests.rs`).
        assert_eq!(
            tokens[0],
            (
                "git.corp".to_string(),
                Forge::GitLab,
                "  glpat-secret  ".to_string()
            ),
            "the page hands the paste over as typed; the host trims it (Task 3)"
        );
    }

    fn token_row(write: Option<TokenWrite>) -> HostRow {
        HostRow {
            host: "git.corp".to_string(),
            forge: Some(Forge::GitLab),
            means: Some(Means::Token),
            pinned_means: None,
            account: Some("me".to_string()),
            write,
            has_token: true,
            configured: true,
        }
    }

    #[test]
    fn a_token_row_says_whether_its_token_may_write() {
        assert_eq!(means_text(&token_row(Some(TokenWrite::Yes))), "Token as me · can read and write");
        assert_eq!(means_text(&token_row(Some(TokenWrite::No))), "Token as me · read-only");
        assert_eq!(means_text(&token_row(Some(TokenWrite::NotReported))), "Token as me · scopes not reported");
        assert_eq!(means_text(&token_row(None)), "Token as me");
    }

    #[test]
    fn a_cli_row_says_nothing_about_scopes() {
        let row = HostRow {
            means: Some(Means::Cli),
            write: None,
            ..token_row(None)
        };
        assert_eq!(means_text(&row), "Signed in with glab as me");
    }
}
