use cracker_desktop::{Snapshot, TransactionKind};
use gpui_kit::component::{
    ActiveTheme,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, IntoElement, div, prelude::*};

use super::{TrackerApp, category_palette};

pub(super) fn categories_panel(
    snapshot: &Snapshot,
    cx: &mut Context<TrackerApp>,
) -> impl IntoElement {
    let theme = cx.theme();
    let mut list = div()
        .id("categories-list")
        .flex()
        .flex_col()
        .gap_6()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll();
    for kind in [TransactionKind::Income, TransactionKind::Expense] {
        let title = if kind == TransactionKind::Income {
            "Income"
        } else {
            "Expense"
        };
        let count = snapshot
            .categories
            .iter()
            .filter(|category| category.kind == kind)
            .count();
        let mut group = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().child(format!("{title} · {count}")));
        if count == 0 {
            group = group.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .text_color(theme.muted_foreground)
                    .child(format!("No {} categories yet.", title.to_lowercase())),
            );
        }
        for (index, category) in snapshot
            .categories
            .iter()
            .enumerate()
            .filter(|(_, category)| category.kind == kind)
        {
            let id = category.id;
            group = group.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(category_palette::swatch(
                                category.color,
                                theme.is_dark(),
                                theme.background,
                                theme.foreground,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(category.name.clone())
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.muted_foreground)
                                            .child(category_palette::name(category.color)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new(("edit-category", index))
                                    .label("Edit")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_category(id, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(("delete-category", index))
                                    .label("Delete")
                                    .danger()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_delete_category(id, window, cx)
                                    })),
                            ),
                    ),
            );
        }
        list = list.child(group);
    }
    div()
        .flex()
        .flex_col()
        .gap_4()
        .flex_1()
        .min_h_0()
        .child(
            div().flex().justify_end().child(
                Button::new("add-category")
                    .primary()
                    .label("Add category")
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.open_category_form(None, window, cx)
                        }),
                    ),
            ),
        )
        .child(list)
}
