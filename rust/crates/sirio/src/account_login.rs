//! Account login uses Sirio's own PTY on every desktop platform. The surface
//! lives in Settings; releasing it cancels the process through TerminalView.

use gpui::{App, Context, Entity, Focusable, Render, WeakEntity, Window, div, prelude::*};
use sirio_terminal::{
    TerminalActivityEvent, TerminalExitStatus, TerminalLinkEvent, TerminalShell, TerminalView,
    command_shell_invocation,
};
use sirio_ui::settings::{AccountLoginRequest, Settings};

fn login_shell(request: &AccountLoginRequest) -> TerminalShell {
    // Both the executable and arguments come from the fixed provider catalog.
    // A login shell supplies the CLI's PATH on Unix; cmd also resolves npm's
    // .cmd shims on Windows. No credentials or user-entered strings go here.
    let command = std::iter::once(request.program)
        .chain(request.args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    #[cfg(not(windows))]
    let command = format!("exec {command}");
    let (program, args) = command_shell_invocation(&command);
    // Match an interactive terminal's startup files as well: npm/nvm and
    // user-local CLI paths are often configured in .bashrc or .zshrc.
    #[cfg(not(windows))]
    let args = std::iter::once("-i".to_string()).chain(args).collect();
    TerminalShell::WithArguments { program, args }
}

pub(crate) fn start(settings: &Entity<Settings>, request: &AccountLoginRequest, cx: &mut App) {
    start_with_shell(settings, request, login_shell(request), cx);
}

fn start_with_shell(
    settings: &Entity<Settings>,
    request: &AccountLoginRequest,
    shell: TerminalShell,
    cx: &mut App,
) {
    if !settings.read(cx).is_account_login_current(request.id) {
        return;
    }
    let directory = std::env::temp_dir();
    let terminal = cx.new(|cx| {
        TerminalView::with_shell(&directory, shell.clone(), cx).unwrap_or_else(|error| {
            TerminalView::failed(&directory, shell.clone(), format!("{error:#}"), cx)
        })
    });
    let view =
        cx.new(|cx| AccountLoginTerminal::new(terminal.clone(), settings.downgrade(), request, cx));
    settings.update(cx, |settings, cx| {
        settings.set_account_login_surface(request.id, view.clone().into(), cx);
    });
    // Start after subscriptions are installed, so even an immediate exit is
    // observed. An unsuccessful spawn is reported to the same Settings card.
    terminal.update(cx, |terminal, cx| terminal.start_from_prompt(shell, cx));
    view.update(cx, |view, cx| view.finish_if_failed(cx));
}

struct AccountLoginTerminal {
    terminal: Entity<TerminalView>,
    settings: WeakEntity<Settings>,
    id: u64,
    program: &'static str,
    finished: bool,
    focus_pending: bool,
}

impl AccountLoginTerminal {
    fn new(
        terminal: Entity<TerminalView>,
        settings: WeakEntity<Settings>,
        request: &AccountLoginRequest,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&terminal, |this, _, event, cx| {
            if let TerminalActivityEvent::ChildExited { status } = event {
                let result = match status {
                    TerminalExitStatus::Success => Ok(()),
                    status => Err(format!(
                        "{} login exited without success ({status:?})",
                        this.program
                    )),
                };
                this.finish(result, cx);
            }
        })
        .detach();
        cx.subscribe(&terminal, |_, _, event: &TerminalLinkEvent, cx| {
            cx.open_url(&event.url);
        })
        .detach();
        cx.observe(&terminal, |this, _, cx| this.finish_if_failed(cx))
            .detach();
        Self {
            terminal,
            settings,
            id: request.id,
            program: request.program,
            finished: false,
            focus_pending: true,
        }
    }

    fn finish_if_failed(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = self.terminal.read(cx).failure_message() {
            self.finish(
                Err(format!("could not start {} login: {message}", self.program)),
                cx,
            );
        }
    }

    fn finish(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let _ = self.settings.update(cx, |settings, cx| {
            settings.complete_account_login(self.id, result, cx);
        });
    }
}

impl Render for AccountLoginTerminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus_pending = false;
            let focus = self.terminal.focus_handle(cx);
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        div().size_full().child(self.terminal.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext};
    use sirio_theme::Theme;
    use sirio_ui::settings::SettingsEvent;
    use std::{
        cell::Cell,
        rc::Rc,
        time::{Duration, Instant},
    };

    #[test]
    fn login_shell_uses_the_platform_interpreter_and_cli_startup_environment() {
        let shell = login_shell(&AccountLoginRequest {
            id: 1,
            program: "claude",
            args: vec!["auth", "login"],
        });
        let TerminalShell::WithArguments { program, args } = shell else {
            panic!("command shell")
        };
        assert!(!program.contains("x-terminal-emulator"));
        #[cfg(windows)]
        assert_eq!(args, ["/C", "claude auth login"]);
        #[cfg(not(windows))]
        assert_eq!(args, ["-i", "-lc", "exec claude auth login"]);
    }

    async fn exercise_login_button(cx: &mut TestAppContext, shell: TerminalShell, fails: bool) {
        cx.update(Theme::init);
        let (settings, cx) = cx.add_window_view(|_, cx| Settings::new(cx));
        let attempt = Rc::new(Cell::new(None));
        let observed = attempt.clone();
        cx.update(|_, cx| {
            cx.subscribe(&settings, move |settings, event: &SettingsEvent, cx| {
                if let SettingsEvent::StartAccountLogin(request) = event {
                    observed.set(Some(request.id));
                    start_with_shell(&settings, request, shell.clone(), cx);
                }
            })
            .detach();
        });
        cx.run_until_parked();
        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("providers category");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();
        let button = cx.debug_bounds("add-claude-account").expect("login button");
        cx.simulate_click(button.center(), Modifiers::none());
        cx.run_until_parked();
        let id = attempt.get().expect("button must emit a login request");
        let deadline = Instant::now() + Duration::from_secs(10);
        while settings.read_with(&cx.cx, |settings, _| settings.is_account_login_current(id)) {
            assert!(Instant::now() < deadline, "login never completed");
            std::thread::sleep(Duration::from_millis(10));
            cx.background_executor
                .advance_clock(Duration::from_millis(10));
            cx.run_until_parked();
        }
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("add-claude-account").is_some(),
            "login must allow a retry"
        );
        assert_eq!(
            cx.debug_bounds("provider-account-error-Claude Code")
                .is_some(),
            fails
        );
        assert!(
            cx.debug_bounds("account-login-terminal").is_none(),
            "completed PTY is released"
        );
    }

    fn fixture_shell(exit_code: i32) -> TerminalShell {
        #[cfg(windows)]
        let command = format!("exit /b {exit_code}");
        #[cfg(not(windows))]
        let command = format!("exit {exit_code}");
        let (program, args) = command_shell_invocation(&command);
        TerminalShell::WithArguments { program, args }
    }

    #[gpui::test]
    async fn login_button_completes_after_real_pty_success(cx: &mut TestAppContext) {
        exercise_login_button(cx, fixture_shell(0), false).await;
    }

    #[gpui::test]
    async fn login_button_reports_real_pty_failure_and_allows_retry(cx: &mut TestAppContext) {
        exercise_login_button(cx, fixture_shell(7), true).await;
    }

    #[gpui::test]
    async fn login_button_reports_spawn_failure_and_allows_retry(cx: &mut TestAppContext) {
        exercise_login_button(
            cx,
            TerminalShell::WithArguments {
                program: "sirio-nonexistent-login-test-program".into(),
                args: vec![],
            },
            true,
        )
        .await;
    }
}
