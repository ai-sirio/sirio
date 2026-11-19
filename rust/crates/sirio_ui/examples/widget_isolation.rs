//! The isolation proof's harness: builds a widget webview with `build_widget_child`, lets the
//! probe view (`Scripts/Tests/fixtures/mcp-apps/isolation-view.html`) report, drops it, builds a
//! second view on the same origin, and writes what it saw as JSON lines. `widget-isolation.sh`
//! runs it twice and judges the output. Linux only.

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("widget_isolation runs on Linux only");
}

#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        rc::Rc,
        sync::Arc,
        time::{Duration, Instant},
    };

    use gpui::{
        App, AppContext, Bounds, Context, IntoElement, Render, Task, Window, WindowBounds,
        Styled, WindowOptions, div, px, size,
    };
    use gpui_platform::application;
    use serde_json::json;
    use webkit2gtk::{SettingsExt, WebViewExt};
    use sirio_ui::mcp_widget::{
        Inbox, WidgetInbox, WidgetPage, accepts_drops, build_widget_child, flush_x, new_secret,
        pump_gtk,
    };
    use wry::{
        Rect, WebView, WebViewExtUnix,
        dpi::{LogicalPosition, LogicalSize},
    };

    /// The view CSP of the plan's Global Constraints, with no resource or connect domains.
    const VIEW_CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; \
        style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
        media-src 'self' data:; connect-src 'none'; frame-src 'none'; object-src 'none'; \
        base-uri 'none'; form-action 'none'";

    /// The control's view CSP: permissive, so the restrictive one is what the run shows blocking.
    const CONTROL_CSP: &str = "default-src * 'unsafe-inline' data: blob:";

    /// `csp`: true sends `VIEW_CSP`, false sends `CONTROL_CSP`.
    struct Args {
        logger: String,
        html: String,
        out: String,
        csp: bool,
        incognito: bool,
        /// The addresses of the hint probes (`preconnect`, `prefetch`, `ping`); each defaults to
        /// `logger`.
        hints: [String; 3],
        /// `on` turns hyperlink auditing and DNS prefetching back on after the build, so the
        /// control shows the probes can reach their loggers when production's settings are off.
        auditing: bool,
    }

    fn parse_args() -> Args {
        let mut values = std::collections::HashMap::new();
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            if let Some(value) = args.next() {
                values.insert(flag, value);
            }
        }
        let get = |name: &str| {
            values
                .get(name)
                .cloned()
                .unwrap_or_else(|| panic!("a required flag is missing"))
        };
        let on = |name: &str| get(name) == "on";
        let logger = get("--logger");
        let hint = |name: &str| values.get(name).cloned().unwrap_or_else(|| logger.clone());
        Args {
            hints: [hint("--logger-preconnect"), hint("--logger-prefetch"), hint("--logger-ping")],
            html: get("--html"),
            out: get("--out"),
            csp: on("--csp"),
            incognito: on("--incognito"),
            auditing: values.get("--auditing").is_some_and(|value| value == "on"),
            logger,
        }
    }

    enum Phase {
        Build,
        WaitA,
        Settle(Instant),
        WaitB,
    }

    struct Harness {
        args: Args,
        template: String,
        key: String,
        out: File,
        phase: Phase,
        started: Instant,
        a: Option<(WebView, WidgetInbox)>,
        a_forged: u32,
        a_reported_at: Option<Instant>,
        b: Option<(WebView, WidgetInbox)>,
        _tick: Option<Task<()>>,
    }

    impl Harness {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let args = parse_args();
            let template = std::fs::read_to_string(&args.html).expect("the probe view is readable");
            let out = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&args.out)
                .expect("the output file opens");
            let tick = cx.spawn_in(window, async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(16)).await;
                    if this.update_in(cx, |harness, window, cx| harness.tick(window, cx)).is_err()
                    {
                        break;
                    }
                }
            });
            Self {
                args,
                template,
                key: new_secret()[..16].to_owned(),
                out,
                phase: Phase::Build,
                started: Instant::now(),
                a: None,
                a_forged: 0,
                a_reported_at: None,
                b: None,
                _tick: Some(tick),
            }
        }

        fn line(&mut self, value: serde_json::Value) {
            let _ = writeln!(self.out, "{value}");
            let _ = self.out.flush();
        }

        fn spawn_view(&self, window: &mut Window, role: &str) -> Option<(WebView, WidgetInbox)> {
            let html = self
                .template
                .replace("{{LOGGER}}", &self.args.logger)
                .replace("{{LOGGER_PRECONNECT}}", &self.args.hints[0])
                .replace("{{LOGGER_PREFETCH}}", &self.args.hints[1])
                .replace("{{LOGGER_PING}}", &self.args.hints[2])
                .replace("{{ROLE}}", role);
            let page = Arc::new(WidgetPage {
                key: self.key.clone(),
                secret: new_secret(),
                html,
                view_csp: if self.args.csp { VIEW_CSP } else { CONTROL_CSP }.to_owned(),
            });
            let inbox: WidgetInbox = Rc::new(std::cell::RefCell::new(Inbox::default()));
            let webview =
                build_widget_child(window, page, inbox.clone(), self.args.incognito).ok()?;
            if self.args.auditing
                && let Some(settings) = WebViewExt::settings(&webview.webview())
            {
                settings.set_enable_hyperlink_auditing(true);
                settings.set_enable_dns_prefetching(true);
            }
            // As production does at `place`.
            let _ = webview.set_bounds(Rect {
                position: LogicalPosition::new(20_i32, 20_i32).into(),
                size: LogicalSize::new(400_u32, 300_u32).into(),
            });
            let _ = webview.set_visible(true);
            Some((webview, inbox))
        }

        /// Writes the new inbox lines; true when one of them was a probe report.
        fn drain(&mut self, role: &str) -> bool {
            let inbox = match role {
                "A" => self.a.as_ref().map(|(_, inbox)| inbox.clone()),
                _ => self.b.as_ref().map(|(_, inbox)| inbox.clone()),
            };
            let Some(inbox) = inbox else { return false };
            let messages: Vec<String> = inbox.borrow_mut().messages.drain(..).collect();
            let mut reported = false;
            for text in messages {
                let message: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                reported |= message["method"] == "probe";
                self.line(json!({ "kind": "message", "role": role, "message": message }));
            }
            reported
        }

        fn finish(&mut self, cx: &mut Context<Self>, reason: &str) {
            self.line(json!({ "kind": "forged", "role": "A", "count": self.a_forged }));
            self.line(json!({ "kind": "end", "reason": reason }));
            cx.quit();
        }

        fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            pump_gtk(Duration::from_millis(4));
            if self.started.elapsed() > Duration::from_secs(30) {
                self.finish(cx, "timeout");
                return;
            }
            match self.phase {
                Phase::Build => {
                    self.a = self.spawn_view(window, "A");
                    match &self.a {
                        Some((webview, _)) => {
                            let drops = accepts_drops(webview);
                            self.line(
                                json!({ "kind": "acceptsDrops", "role": "A", "value": drops }),
                            );
                            self.phase = Phase::WaitA;
                        }
                        None => self.finish(cx, "build-failed"),
                    }
                }
                Phase::WaitA => {
                    if self.drain("A") && self.a_reported_at.is_none() {
                        self.a_reported_at = Some(Instant::now());
                    }
                    if self
                        .a_reported_at
                        .is_some_and(|at| at.elapsed() >= Duration::from_secs(1))
                    {
                        self.drain("A");
                        self.a_forged = self.a.as_ref().map_or(0, |(_, i)| i.borrow().forged);
                        self.a = None;
                        flush_x();
                        let extra = if self.args.incognito { 0 } else { 3 };
                        self.phase = Phase::Settle(Instant::now() + Duration::from_secs(extra));
                    }
                }
                Phase::Settle(until) => {
                    if Instant::now() >= until {
                        self.b = self.spawn_view(window, "B");
                        if self.b.is_some() {
                            self.phase = Phase::WaitB;
                        } else {
                            self.finish(cx, "build-failed");
                        }
                    }
                }
                Phase::WaitB => {
                    if self.drain("B") {
                        self.finish(cx, "done");
                    }
                }
            }
        }
    }

    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    pub fn main() {
        application().run(|cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(800.0), px(500.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Harness::new(window, cx)),
            )
            .expect("open the harness window");
            cx.activate(true);
        });
    }
}
