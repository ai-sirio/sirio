use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, Entity, FontWeight, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::*,
};

use super::line::{OnText, send_line};
use crate::{
    buttons::{Button, ButtonVariant},
    data_display::UsageBar,
    forms::TextInput,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
    typography::{format::currency, tabular},
};

/// A question an agent waits on: a few answers to pick, a line to write one, and, once answered, the answer.
#[derive(IntoElement)]
pub struct HumanInputRequest {
    id: ElementId,
    question: SharedString,
    field: Option<Entity<TextInput>>,
    choices: Vec<SharedString>,
    answered: Option<SharedString>,
    on_answer: Option<OnText>,
    body: Option<AnyElement>,
    actions: Vec<AnyElement>,
    header_selector: Option<SharedString>,
}

impl HumanInputRequest {
    /// `field` is the owner's line for a written answer.
    pub fn new(
        id: impl Into<ElementId>,
        question: impl Into<SharedString>,
        field: &Entity<TextInput>,
        on_answer: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            question: question.into(),
            field: Some(field.clone()),
            choices: Vec::new(),
            answered: None,
            on_answer: Some(Rc::new(on_answer)),
            body: None,
            actions: Vec::new(),
            header_selector: None,
        }
    }

    /// Use the host's existing editor and protocol actions without an Ely field.
    pub fn custom(id: impl Into<ElementId>, question: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            question: question.into(),
            field: None,
            choices: Vec::new(),
            answered: None,
            on_answer: None,
            body: None,
            actions: Vec::new(),
            header_selector: None,
        }
    }
    pub fn header_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.header_selector = Some(selector.into());
        self
    }
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Answers to pick with a press.
    pub fn choices(mut self, choices: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.choices = choices.into_iter().map(Into::into).collect();
        self
    }

    /// The answer given; the request stops asking.
    pub fn answered(mut self, answer: impl Into<SharedString>) -> Self {
        self.answered = Some(answer.into());
        self
    }
}

impl RenderOnce for HumanInputRequest {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let asking = self.answered.is_none();
        let choices = (asking && !self.choices.is_empty() && self.on_answer.is_some()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(self.choices.iter().enumerate().map(|(ix, choice)| {
                    let (answer, picked) =
                        (self.on_answer.as_ref().unwrap().clone(), choice.clone());
                    Button::new((self.id.clone(), format!("choice-{ix}")), choice.clone())
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .on_click(move |_, window, cx| {
                            log::info!("human input: picked {ix}");
                            answer(&picked, window, cx)
                        })
                }))
        });
        let line = if asking && self.body.is_none() {
            self.field
                .as_ref()
                .zip(self.on_answer.as_ref())
                .map(|(field, on_answer)| {
                    send_line(
                        field,
                        Button::new((self.id.clone(), "reply"), "Reply")
                            .variant(ButtonVariant::Primary),
                        on_answer.clone(),
                        "human input",
                        cx,
                    )
                })
        } else {
            None
        };
        let theme = cx.theme();
        let colors = theme.colors;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded(theme.radius(Radius::Lg))
            .border_1()
            .border_color(colors.border)
            .bg(colors.surface)
            .text_size(theme.text_size(TextSize::Sm))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div().flex_none().child(
                            Icon::new(IconName::CircleHelp)
                                .size(IconSize::Sm)
                                .color(colors.fg_muted),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.fg)
                            .when_some(self.header_selector, |title, selector| {
                                title.debug_selector(move || selector.to_string())
                            })
                            .child(self.question),
                    ),
            )
            .children(self.answered.map(|answer| {
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .text_color(colors.fg_muted)
                    .child(
                        div().flex_none().child(
                            Icon::new(IconName::Check)
                                .size(IconSize::Sm)
                                .color(colors.success),
                        ),
                    )
                    .child(div().flex_1().min_w_0().child(answer))
            }))
            .children(self.body)
            .children(choices)
            .children(line)
            .when(asking && !self.actions.is_empty(), |card| {
                card.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(self.actions),
                )
            })
    }
}

/// What a session cost, as a whole and part by part: the total, then a bar of each part's share with its amount.
#[derive(IntoElement)]
pub struct CostBreakdown {
    label: SharedString,
    code: &'static str,
    parts: Vec<(SharedString, f64)>,
}

impl CostBreakdown {
    /// `code` is the currency, such as "USD".
    pub fn new(label: impl Into<SharedString>, code: &'static str) -> Self {
        Self {
            label: label.into(),
            code,
            parts: Vec::new(),
        }
    }

    pub fn part(mut self, name: impl Into<SharedString>, cost: f64) -> Self {
        assert!(cost >= 0.0 && cost.is_finite(), "a cost of {cost}");
        self.parts.push((name.into(), cost));
        self
    }
}

impl RenderOnce for CostBreakdown {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors;
        let total: f64 = self.parts.iter().map(|(_, cost)| cost).sum();
        let code = self.code;
        let bar = (total > 0.0).then(|| {
            self.parts.into_iter().fold(
                UsageBar::new(total).amounts(move |amount| currency(amount, code)),
                |bar, (name, cost)| bar.part(name, cost),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_3()
            .text_size(theme.text_size(TextSize::Sm))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .justify_between()
                    .gap_x_3()
                    .child(div().text_color(colors.fg_muted).child(self.label))
                    .child(
                        tabular(div())
                            .text_size(theme.text_size(TextSize::Lg))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.fg)
                            .child(currency(total, code)),
                    ),
            )
            .children(bar)
    }
}
