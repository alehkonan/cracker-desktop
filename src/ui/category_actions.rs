use cracker_desktop::{Category, CategoryColor, Storage};
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, Window, div, prelude::*, px};
use uuid::Uuid;

use super::{
    TrackerApp,
    category_delete::{CategoryDelete, DeleteCategory},
    category_form::{CategoryForm, SaveCategory},
};

impl TrackerApp {
    pub(super) fn edit_category(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(category) = self
            .snapshot
            .categories
            .iter()
            .find(|category| category.id == id)
        {
            self.open_category_form(Some(category.clone()), window, cx);
        }
    }

    pub(super) fn open_category_form(
        &mut self,
        category: Option<Category>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locked = category.as_ref().is_some_and(|category| {
            self.snapshot
                .transactions
                .iter()
                .any(|transaction| transaction.category_id == Some(category.id))
        });
        let default_color = CategoryColor::ALL
            .into_iter()
            .min_by_key(|color| {
                self.snapshot
                    .categories
                    .iter()
                    .filter(|category| category.color == *color)
                    .count()
            })
            .unwrap_or_default();
        let title = if category.is_some() {
            "Edit category"
        } else {
            "Add category"
        };
        let form = cx.new(|cx| CategoryForm::new(category, locked, default_color, window, cx));
        cx.subscribe_in(
            &form,
            window,
            |this, form, event: &SaveCategory, window, cx| {
                let form = form.clone();
                let path = this.database_path.clone();
                let category = event.category.clone();
                let editing = event.editing;
                let write = cx.background_executor().spawn(async move {
                    let mut storage = Storage::open(&path)?;
                    if editing {
                        storage.update_category(&category)
                    } else {
                        storage.create_category(&category)
                    }
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = write.await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(snapshot) => {
                            this.snapshot = snapshot;
                            window.close_dialog(cx);
                            cx.notify();
                        }
                        Err(error) => {
                            form.update(cx, |form, cx| form.save_failed(format!("{error:#}"), cx))
                        }
                    });
                })
                .detach();
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancel_form = form.clone();
            dialog
                .title(title)
                .w(px(560.0))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel_form.read(cx).saving())
                .child(form.clone())
                .footer(div())
        });
    }

    pub(super) fn confirm_delete_category(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(category) = self
            .snapshot
            .categories
            .iter()
            .find(|category| category.id == id)
            .cloned()
        else {
            return;
        };
        let count = self
            .snapshot
            .transactions
            .iter()
            .filter(|transaction| transaction.category_id == Some(id))
            .count();
        let confirmation = cx.new(|_| CategoryDelete::new(category, count));
        cx.subscribe_in(
            &confirmation,
            window,
            |this, confirmation, event: &DeleteCategory, window, cx| {
                let confirmation = confirmation.clone();
                let path = this.database_path.clone();
                let id = event.0;
                let write = cx
                    .background_executor()
                    .spawn(async move { Storage::open(&path)?.delete_category(id) });
                cx.spawn_in(window, async move |this, cx| {
                    let result = write.await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(snapshot) => {
                            this.snapshot = snapshot;
                            window.close_dialog(cx);
                            cx.notify();
                        }
                        Err(error) => confirmation.update(cx, |confirmation, cx| {
                            confirmation.delete_failed(format!("{error:#}"), cx)
                        }),
                    });
                })
                .detach();
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancel_confirmation = confirmation.clone();
            dialog
                .title("Delete category")
                .w(px(520.0))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel_confirmation.read(cx).deleting())
                .child(confirmation.clone())
                .footer(div())
        });
    }
}
