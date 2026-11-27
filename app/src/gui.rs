pub(crate) mod context_menu;
mod controller;
pub mod dialogs;
mod locale;
mod menu_bar;
mod movie;
mod mugetsu_menu;
mod picker;
mod prompt;
mod theme;
mod widgets;

pub use controller::GuiController;
pub use dialogs::DialogDescriptor;
pub use locale::LocalizableText;
pub use locale::available_languages;
pub use locale::optional_text;
pub use locale::text;
pub use locale::text_with_args;
pub use movie::MovieView;
pub use picker::FilePicker;
use ruffle_frontend_utils::content::ContentDescriptor;
pub use theme::ThemePreference;

use crate::custom_event::RuffleEvent;
use crate::gui::context_menu::ContextMenu;
use crate::gui::prompt::{Action, Prompt};
use crate::player::LaunchOptions;
use crate::preferences::GlobalPreferences;
use dialogs::Dialogs;
use egui::*;
use menu_bar::MenuBar;
use rfd::AsyncFileDialog;
use ruffle_core::debug_ui::Message as DebugMessage;
use ruffle_core::{Player, PlayerEvent};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::{fs, mem};
use winit::event_loop::EventLoopProxy;

/// Size of the top chrome (account tab bar + Grimlite navbar) in logical pixels.
/// This is the offset at which the movie will be shown,
/// and added to the window size if trying to match a movie.
pub const MENU_HEIGHT: u32 =
    (grimlite_egui::TAB_BAR_HEIGHT + grimlite_egui::NAV_BAR_HEIGHT) as u32;

fn build_grimlite_ui(bot: Arc<grimlite_bot::Bot>) -> grimlite_egui::GrimliteUi {
    grimlite_egui::GrimliteUi::new(
        bot,
        Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter("Grimoire Bot", &["gbot", "json"])
                .pick_file()
        })),
        Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter("Grimoire Bot", &["gbot"])
                .set_file_name("bot.gbot")
                .save_file()
        })),
    )
    .with_maid_dialogs(
        Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter("Maid Config", &["json"])
                .pick_file()
        })),
        Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter("Maid Config", &["json"])
                .set_file_name("maid.json")
                .save_file()
        })),
    )
    .with_image_dialog(Some(Box::new(|| {
        rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
    })))
}

/// The main controller for the Ruffle GUI.
pub struct RuffleGui {
    event_loop: EventLoopProxy<RuffleEvent>,
    context_menu: Option<ContextMenu>,
    /// AQW build: Mugetsu's questions in the game window (see `prompt`), the first one shown.
    prompts: Vec<Prompt>,
    dialogs: Dialogs,
    menu_bar: MenuBar,

    was_suspended_before_debug: bool,
    preferences: GlobalPreferences,
    tab_uis: HashMap<u64, grimlite_egui::GrimliteUi>,
    tab_bots: Vec<(u64, Arc<grimlite_bot::Bot>)>,
    active_tab_id: u64,
}

impl RuffleGui {
    fn new(
        window: Weak<winit::window::Window>,
        event_loop: EventLoopProxy<RuffleEvent>,
        default_content: Option<ContentDescriptor>,
        default_launch_options: LaunchOptions,
        preferences: GlobalPreferences,
    ) -> Self {
        Self {
            was_suspended_before_debug: false,

            context_menu: None,
            prompts: Vec::new(),
            dialogs: Dialogs::new(
                preferences.clone(),
                default_launch_options.clone(),
                default_content,
                window,
                event_loop.clone(),
            ),
            menu_bar: MenuBar::new(
                event_loop.clone(),
                default_launch_options,
                preferences.clone(),
            ),

            event_loop,
            preferences,
            tab_uis: HashMap::new(),
            tab_bots: Vec::new(),
            active_tab_id: 0,
        }
    }

    pub fn register_tab(&mut self, tab_id: u64, bot: Arc<grimlite_bot::Bot>) {
        self.tab_uis
            .insert(tab_id, build_grimlite_ui(bot.clone()));
        if !self.tab_bots.iter().any(|(id, _)| *id == tab_id) {
            self.tab_bots.push((tab_id, bot));
        }
        self.active_tab_id = tab_id;
        self.context_menu = None;
    }

    pub fn set_active_tab(&mut self, tab_id: u64) {
        self.active_tab_id = tab_id;
        self.context_menu = None;
    }

    pub fn remove_tab(&mut self, tab_id: u64) {
        self.tab_uis.remove(&tab_id);
        self.tab_bots.retain(|(id, _)| *id != tab_id);
    }

    pub fn clear_tabs(&mut self) {
        self.tab_uis.clear();
        self.tab_bots.clear();
        self.active_tab_id = 0;
    }

    /// Renders all of the main Ruffle UI, including the main menu and context menus.
    fn update(
        &mut self,
        egui_ctx: &egui::Context,
        egui_ui: &mut egui::Ui,
        show_menu: bool,
        mut player: Option<&mut Player>,
        menu_height_offset: f64,
    ) {
        let locale = self.preferences.language();

        self.menu_bar
            .consume_shortcuts(egui_ctx, &mut self.dialogs, player.as_deref_mut());
        if show_menu {
            self.menu_bar
                .show(&locale, egui_ui, &mut self.dialogs, player.as_deref_mut());
        }

        self.dialogs.show(&locale, egui_ctx, player.as_deref_mut());

        // Account tab bar (always visible at top for multi-account switching)
        let descriptors: Vec<grimlite_egui::TabDescriptor> = self
            .tab_bots
            .iter()
            .map(|(id, bot)| {
                let snap = bot.player_snapshot();
                let label = if snap.logged_in && !snap.username.trim().is_empty() {
                    snap.username
                } else {
                    format!("Account #{id}")
                };
                let bot_running = bot.runner.is_running()
                    || bot.auto_attack.is_running()
                    || bot.maid.is_running();
                grimlite_egui::TabDescriptor {
                    id: *id,
                    label,
                    logged_in: snap.logged_in,
                    bot_running,
                }
            })
            .collect();
        let tab_action =
            grimlite_egui::show_tab_bar(egui_ui, &descriptors, self.active_tab_id);
        if tab_action != grimlite_egui::TabBarAction::None {
            let _ = self
                .event_loop
                .send_event(RuffleEvent::GrimliteTabAction(tab_action));
        }

        if let Some(active_ui) = self.tab_uis.get_mut(&self.active_tab_id) {
            active_ui.show(egui_ctx, egui_ui);
        }

        if let Some(player) = player {
            let was_suspended = player.debug_ui().should_suspend_player();
            player.show_debug_ui(egui_ctx, menu_height_offset);
            if was_suspended != player.debug_ui().should_suspend_player() {
                if player.debug_ui().should_suspend_player() {
                    self.was_suspended_before_debug = !player.is_playing();
                    player.set_is_playing(false);
                } else {
                    player.set_is_playing(!self.was_suspended_before_debug);
                }
            }
            for item in player.debug_ui().items_to_save() {
                let dialog = AsyncFileDialog::new().set_file_name(&item.suggested_name);
                let picker = self.dialogs.file_picker();
                let result = picker.show_dialog(dialog, |d| d.save_file());
                if let Some(result) = result {
                    tokio::spawn(async move {
                        let Some(handle) = result.await else {
                            return;
                        };
                        let path = handle.path();
                        if let Err(e) = fs::write(path, item.data) {
                            tracing::error!(
                                "Couldn't save {} to {path:?}: {e}",
                                item.suggested_name,
                            );
                        }
                    });
                }
            }

            if let Some(context_menu) = &mut self.context_menu
                && !context_menu.show(&locale, egui_ctx, &self.event_loop, player.is_fullscreen())
            {
                self.close_context_menu(player);
            }
        };

        if let Some(prompt) = self.prompts.first()
            && let Some(picked) = prompt.show(egui_ctx)
        {
            match self.prompts.remove(0).into_action(picked) {
                Some(Action::RestartWithGraphics(choice)) => {
                    let _ = self.event_loop.send_event(RuffleEvent::MugetsuSetting(
                        crate::mugetsu::Setting::RestartWithGraphics(choice),
                    ));
                }
                Some(Action::OpenReleasesPage) => crate::mugetsu::open_releases_page(),
                Some(Action::OpenGraphicsSettings) => crate::mugetsu::open_graphics_settings(),
                Some(Action::SkipVersion(version)) => crate::mugetsu::skip_update(&version),
                Some(Action::StopUpdateReminders) => crate::mugetsu::set_update_reminders(false),
                Some(Action::Restart) => {
                    let _ = self.event_loop.send_event(RuffleEvent::MugetsuSetting(
                        crate::mugetsu::Setting::Restart,
                    ));
                }
                Some(Action::TurnOnPerformance(level)) => {
                    let _ = self.event_loop.send_event(RuffleEvent::MugetsuSetting(
                        crate::mugetsu::Setting::Performance(level),
                    ));
                }
                None => {}
            }
        }
    }

    /// AQW build: asks in the game window before restarting with `choice`. A Windows message
    /// box opened from winit's event handler was never shown: Windows shows one when its message
    /// queue goes idle, and inside the handler winit answered every repaint with another. The
    /// game looked frozen (its window disabled behind the invisible box) until it was ended in
    /// Task Manager.
    pub fn ask_graphics_restart(&mut self, choice: Option<crate::mugetsu::Graphics>) {
        // A later pick replaces an earlier one still being asked about.
        self.prompts.retain(|prompt| !prompt.is_graphics());
        self.prompts.push(Prompt::graphics(choice));
    }

    /// AQW build: offers the restart that gives frame interpolation its full speed (see
    /// `Prompt::interpolation_restart`), or takes the offer back (`wanted` false:
    /// interpolation was turned off again).
    pub fn offer_interpolation_restart(&mut self, wanted: bool) {
        self.prompts
            .retain(|prompt| !prompt.is_interpolation_restart());
        if wanted {
            self.prompts.push(Prompt::interpolation_restart());
        }
    }

    /// AQW build: tells the player something in the game window (see `mugetsu::Notice`).
    pub fn show_notice(&mut self, notice: crate::mugetsu::Notice) {
        self.prompts.push(Prompt::notice(notice));
    }

    pub fn show_context_menu(
        &mut self,
        menu: Vec<ruffle_core::ContextMenuItem>,
        settings: Option<crate::mugetsu::MenuSettings>,
        close_event: PlayerEvent,
    ) {
        if !menu.is_empty() || settings.is_some() {
            self.context_menu = Some(ContextMenu::new(menu, settings, close_event));
        }
    }

    pub fn close_context_menu(&mut self, player: &mut Player) {
        if let Some(context_menu) = mem::take(&mut self.context_menu) {
            player.handle_event(context_menu.close_event());
        }
    }

    pub fn is_context_menu_visible(&self) -> bool {
        self.context_menu.is_some()
    }

    /// Whether an interactive UI window, dialog, prompt, or dropdown popup is currently open.
    /// Static top chrome (`TabBar` + `NavBar`) alone does not need unthrottled repaints when
    /// events occur inside the Flash game area.
    pub fn has_interactive_overlay(&self, ctx: &egui::Context) -> bool {
        let active_open = self
            .tab_uis
            .get(&self.active_tab_id)
            .is_some_and(grimlite_egui::GrimliteUi::has_open_window);
        active_open
            || !self.prompts.is_empty()
            || self.context_menu.is_some()
            || egui::Popup::is_any_open(ctx)
            || ctx.egui_wants_keyboard_input()
    }

    /// Notifies the GUI that the player has been destroyed.
    fn on_player_destroyed(&mut self) {
        self.dialogs.close_dialogs_with_notifiers();
    }

    /// Notifies the GUI that a new player was created.
    fn on_player_created(
        &mut self,
        opt: LaunchOptions,
        content_descriptor: ContentDescriptor,
        mut player: grimlite_frontend::gpu::TabPlayerGuard<'_>,
    ) {
        self.menu_bar.currently_opened = Some((content_descriptor.clone(), opt.clone()));

        // Update dialog state to reflect the newly-opened movie's options.
        self.dialogs
            .recreate_open_dialog(opt, Some(content_descriptor), self.event_loop.clone());

        player.set_volume(self.dialogs.volume_controls.get_volume());
    }
}
