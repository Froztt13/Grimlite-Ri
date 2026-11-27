//! Fast Travel tool window (quick map presets + custom join / cell jump / spawn point).

use crate::show_detached_window;
use egui::{Context, RichText, TextEdit};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use std::sync::Arc;

struct TravelPreset {
    label: &'static str,
    map: &'static str,
    cell: &'static str,
    pad: &'static str,
}

const TRAVEL_PRESETS: &[TravelPreset] = &[
    TravelPreset { label: "Battleon", map: "battleon", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Yulgar", map: "yulgar", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Tercessuinotlim", map: "tercessuinotlim", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Nulgath (Tercess)", map: "tercessuinotlim", cell: "Boss2", pad: "Right" },
    TravelPreset { label: "Swindle (Tercess)", map: "tercessuinotlim", cell: "Swindle", pad: "Left" },
    TravelPreset { label: "Bamboozle (Tercess)", map: "tercessuinotlim", cell: "Taro", pad: "Left" },
    TravelPreset { label: "Icestormarena", map: "icestormarena", cell: "r3c", pad: "Left" },
    TravelPreset { label: "Shadowblast", map: "shadowblast", cell: "r10", pad: "Left" },
    TravelPreset { label: "Time Inn", map: "timeinn", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Ultra Dage", map: "ultradage", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Champion Drakath", map: "championdrakath", cell: "Enter", pad: "Spawn" },
    TravelPreset { label: "Ultra Speaker", map: "ultraspeaker", cell: "Enter", pad: "Spawn" },
];

pub struct TravelsWindow {
    bot: Arc<Bot>,
    pub open: bool,
    map: String,
    cell: String,
    pad: String,
    private_room: bool,
}

impl TravelsWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            open: false,
            map: "yulgar".into(),
            cell: "Enter".into(),
            pad: "Spawn".into(),
            private_room: false,
        }
    }

    fn join(&self, map: &str, cell: &str, pad: &str) {
        let target = if self.private_room && !map.contains('-') {
            format!("{map}-99999")
        } else {
            map.to_string()
        };
        self.bot.spawn_flash_action(
            "joinMap",
            vec![
                FlashArg::from(target),
                FlashArg::from(cell),
                FlashArg::from(pad),
            ],
        );
    }

    pub fn show(&mut self, ctx: &Context) {
        let mut open = self.open;
        show_detached_window(
            ctx,
            "grimlite_fast_travel",
            "Fast Travel",
            [400.0, 340.0],
            &mut open,
            |ui| {
                ui.checkbox(&mut self.private_room, "Private room (-99999)");
                ui.horizontal(|ui| {
                    ui.label("Map:");
                    ui.add(TextEdit::singleline(&mut self.map).desired_width(110.0));
                    ui.label("Cell:");
                    ui.add(TextEdit::singleline(&mut self.cell).desired_width(65.0));
                    ui.label("Pad:");
                    ui.add(TextEdit::singleline(&mut self.pad).desired_width(65.0));
                });
                ui.horizontal(|ui| {
                    if ui.button("Join Map").clicked() {
                        self.join(&self.map, &self.cell, &self.pad);
                    }
                    if ui.button("Jump Cell Only").clicked() {
                        self.bot.spawn_flash_action(
                            "moveToCell",
                            vec![
                                FlashArg::from(self.cell.as_str()),
                                FlashArg::from(self.pad.as_str()),
                            ],
                        );
                    }
                    if ui.button("Set Spawn").clicked() {
                        self.bot.spawn_flash_action("setSpawnPoint", vec![]);
                    }
                });

                ui.separator();
                ui.label(RichText::new("Quick Presets:").strong());
                egui::Grid::new("travel_presets_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for (i, preset) in TRAVEL_PRESETS.iter().enumerate() {
                            if ui
                                .add_sized([175.0, 24.0], egui::Button::new(preset.label))
                                .clicked()
                            {
                                self.join(preset.map, preset.cell, preset.pad);
                            }
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            },
        );
        self.open = open;
    }
}
