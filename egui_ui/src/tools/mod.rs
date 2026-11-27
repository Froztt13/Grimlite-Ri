//! Grimlite Ri Tools modules: Fast Travel, Loaders, Hotkeys, Cosmetics, and Packets.

pub mod cosmetics;
pub mod hotkeys;
pub mod loaders;
pub mod packets;
pub mod travels;

use crate::bot_managers::FileDialogFn;
use crate::maid::MaidWindow;
use crate::tools::cosmetics::CosmeticsWindow;
use crate::tools::hotkeys::{HotkeyEffects, HotkeysWindow};
use crate::tools::loaders::LoadersWindow;
use crate::tools::packets::PacketsWindows;
use crate::tools::travels::TravelsWindow;
use egui::{Context, Key, Ui};
use grimlite_bot::Bot;
use std::sync::Arc;

pub struct ToolsWindows {
    pub maid: MaidWindow,
    pub travels: TravelsWindow,
    pub loaders: LoadersWindow,
    pub hotkeys: HotkeysWindow,
    pub cosmetics: CosmeticsWindow,
    pub packets: PacketsWindows,
}

impl ToolsWindows {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            maid: MaidWindow::new(bot.clone(), None, None),
            travels: TravelsWindow::new(bot.clone()),
            loaders: LoadersWindow::new(bot.clone()),
            hotkeys: HotkeysWindow::new(bot.clone()),
            cosmetics: CosmeticsWindow::new(bot.clone()),
            packets: PacketsWindows::new(bot),
        }
    }

    pub fn set_maid_dialogs(
        &mut self,
        pick_json: Option<FileDialogFn>,
        save_json: Option<FileDialogFn>,
    ) {
        self.maid.set_dialogs(pick_json, save_json);
    }

    /// Renders the `Tools` dropdown menu items in the top navigation bar.
    pub fn menu_items(&mut self, ui: &mut Ui) {
        if ui
            .selectable_label(self.travels.open, "Fast Travel")
            .clicked()
        {
            self.travels.open = !self.travels.open;
            ui.close();
        }
        if ui
            .selectable_label(self.loaders.open, "Loader / Grabber")
            .clicked()
        {
            self.loaders.open = !self.loaders.open;
            ui.close();
        }
        if ui.selectable_label(self.hotkeys.open, "Hotkeys").clicked() {
            self.hotkeys.open = !self.hotkeys.open;
            ui.close();
        }
        if ui
            .selectable_label(self.cosmetics.open, "Cosmetics")
            .clicked()
        {
            self.cosmetics.open = !self.cosmetics.open;
            ui.close();
        }
        ui.separator();
        self.packets.menu_items(ui);
    }

    /// Polls active hotkeys and renders all open tool windows.
    pub fn show(&mut self, ctx: &Context) -> HotkeyEffects {
        let child_keys: Vec<Key> = ctx.data_mut(|d| {
            let buf = d.get_temp_mut_or_default::<Vec<Key>>(egui::Id::new(
                "grimlite_child_pressed_keys",
            ));
            std::mem::take(buf)
        });

        let effects = self.hotkeys.poll_with_keys(ctx, &child_keys);
        self.maid.poll_quick_hotkeys(ctx, &child_keys);

        if effects.toggle_maid {
            self.maid.open = !self.maid.open;
        }
        if effects.toggle_travels {
            self.travels.open = !self.travels.open;
        }
        if effects.toggle_loaders {
            self.loaders.open = !self.loaders.open;
        }
        if effects.toggle_cosmetics {
            self.cosmetics.open = !self.cosmetics.open;
        }

        self.maid.show(ctx);
        self.travels.show(ctx);
        self.loaders.show(ctx);
        self.hotkeys.show(ctx);
        self.cosmetics.show(ctx);
        self.packets.show(ctx);
        effects
    }
}
