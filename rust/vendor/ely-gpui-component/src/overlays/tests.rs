use std::{cell::Cell, rc::Rc};

use gpui::{
    Context, FocusHandle, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};

use super::Popover;
use crate::theme::Theme;

/// A popover in a corner, a focusable box elsewhere, and a count of closes.
struct Host {
    closes: Rc<Cell<usize>>,
    elsewhere: FocusHandle,
    heard: usize,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closes = self.closes.clone();
        let this = cx.entity();
        div()
            .size_full()
            .child(
                div()
                    .debug_selector(|| "host".to_owned())
                    .absolute()
                    .top(px(10.0))
                    .left(px(10.0))
                    .child(
                        Popover::new("pop", "Open", |_, _| {
                            div().debug_selector(|| "inside".to_owned()).size(px(80.0)).child("inside")
                        })
                        // Touching the owner proves the hook never runs while
                        // the owner is being drawn (focus leaving is noticed
                        // during a render).
                        .on_close(move |_, cx| {
                            closes.set(closes.get() + 1);
                            this.update(cx, |host, _| host.heard += 1);
                        }),
                    ),
            )
            .child(
                div()
                    .id("elsewhere")
                    .absolute()
                    .bottom(px(10.0))
                    .right(px(10.0))
                    .size(px(20.0))
                    .track_focus(&self.elsewhere),
            )
    }
}

fn open(cx: &mut VisualTestContext) {
    let trigger = cx.debug_bounds("host").expect("the trigger").center();
    cx.simulate_click(trigger, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("inside").is_some(), "the panel did not open");
}

/// The owner learns of every close, whichever way it happened, once each;
/// opening is not a close.
#[gpui::test]
fn every_way_a_popover_closes_tells_its_owner_once(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let closes = Rc::new(Cell::new(0));
    let counted = closes.clone();
    let (host, cx) = cx.add_window_view(move |_, cx| Host {
        closes: counted,
        elsewhere: cx.focus_handle(),
        heard: 0,
    });
    cx.run_until_parked();

    open(cx);
    assert_eq!(closes.get(), 0, "opening is not a close");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(closes.get(), 1, "Escape");

    open(cx);
    cx.simulate_click(point(px(400.0), px(300.0)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(closes.get(), 2, "a press outside");

    open(cx);
    let trigger = cx.debug_bounds("host").expect("the trigger").center();
    cx.simulate_click(trigger, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(closes.get(), 3, "the trigger again");

    open(cx);
    let elsewhere = host.read_with(cx, |host, _| host.elsewhere.clone());
    cx.update(|window, cx| window.focus(&elsewhere, cx));
    cx.run_until_parked();
    assert_eq!(closes.get(), 4, "focus leaving");
    assert!(cx.debug_bounds("inside").is_none());
    assert_eq!(host.read_with(cx, |host, _| host.heard), 4);
}

/// A popover its owner holds open or closed: it follows the owner, and every
/// close still tells the owner.
struct Held {
    want: bool,
    closes: Rc<Cell<usize>>,
}

impl Render for Held {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closes = self.closes.clone();
        let this = cx.entity();
        div().size_full().child(
            div().absolute().top(px(10.0)).left(px(10.0)).child(
                Popover::new("held", "Open", |_, _| {
                    div().debug_selector(|| "held-inside".to_owned()).size(px(80.0))
                })
                .open(self.want)
                .on_close(move |_, cx| {
                    closes.set(closes.get() + 1);
                    this.update(cx, |held, cx| {
                        held.want = false;
                        cx.notify();
                    });
                }),
            ),
        )
    }
}

#[gpui::test]
fn a_popover_follows_its_owner_open_and_closed(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let closes = Rc::new(Cell::new(0));
    let counted = closes.clone();
    let (held, cx) = cx.add_window_view(move |_, _| Held { want: false, closes: counted });
    cx.run_until_parked();
    assert!(cx.debug_bounds("held-inside").is_none());
    held.update(cx, |held, cx| {
        held.want = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("held-inside").is_some(), "the owner's open did not open it");
    assert_eq!(closes.get(), 0);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(closes.get(), 1, "an owner-opened popover still reports its close");
    assert!(cx.debug_bounds("held-inside").is_none());

    held.update(cx, |held, cx| {
        held.want = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("held-inside").is_some(), "it opens again for its owner");
    held.update(cx, |held, cx| {
        held.want = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("held-inside").is_none(), "the owner's close did not close it");
    assert_eq!(closes.get(), 2, "an owner's close is a close too");
}
