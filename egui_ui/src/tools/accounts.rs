//! Compact Account Manager window for saving AQW credentials, filling the login form,
//! or triggering direct login when the player is logged out.

use crate::show_detached_window_sized;
use crate::theme::{self, Palette};
use egui::{Color32, Context, RichText, ScrollArea, Stroke, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const ACCOUNTS_FILE_NAME: &str = "saved_accounts.json";
const ACCOUNTS_KEY_FILE_NAME: &str = ".accounts.key";
const ENCRYPTED_PREFIX: &str = "GRIMLITE_ACCOUNTS_V1:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedAccount {
    pub username: String,
    pub password: String,
}

#[derive(Default)]
struct ImportedForm {
    username: String,
    password: String,
}

pub struct AccountsWindow {
    bot: Arc<Bot>,
    pub open: bool,
    was_open: bool,

    accounts: Vec<SavedAccount>,
    filter: String,
    revealed_rows: HashSet<String>,
    pending_delete: Option<(usize, String)>,

    add_username: String,
    add_password: String,
    show_add_password: bool,
    status: String,

    pending_import: Arc<Mutex<Option<ImportedForm>>>,
}

impl AccountsWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        let accounts = load_accounts();
        Self {
            bot,
            open: false,
            was_open: false,
            accounts,
            filter: String::new(),
            revealed_rows: HashSet::new(),
            pending_delete: None,
            add_username: String::new(),
            add_password: String::new(),
            show_add_password: false,
            status: String::new(),
            pending_import: Arc::new(Mutex::new(None)),
        }
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            self.was_open = false;
            self.pending_delete = None;
            return;
        }

        // Reload from disk whenever the window is opened so accounts saved in other tabs appear immediately.
        if !self.was_open {
            self.was_open = true;
            self.accounts = load_accounts();
        }

        // Apply async "Import from Form" result if available.
        if let Ok(mut slot) = self.pending_import.lock()
            && let Some(form) = slot.take()
        {
            if form.username.is_empty() && form.password.is_empty() {
                self.status = "Login form is empty".to_string();
            } else {
                self.add_username = form.username;
                self.add_password = form.password;
                self.status = "Imported from login form".to_string();
            }
        }

        let mut open = self.open;
        let mut close_after_login = false;
        show_detached_window_sized(
            ctx,
            "grimlite_account_manager",
            "Account Manager",
            [450.0, 355.0],
            Some([395.0, 275.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // 1. Top bar: Search filter + total saved count
                egui::Panel::top("grimlite_accounts_top")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Saved Accounts")
                                    .size(11.0)
                                    .strong()
                                    .color(Palette::TEXT_TITLE),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{} saved", self.accounts.len()))
                                        .small()
                                        .weak(),
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.filter)
                                        .hint_text("Search username...")
                                        .desired_width(150.0),
                                );
                            });
                        });
                        ui.add_space(4.0);
                    });

                // 2. Bottom panel: Add Account card
                egui::Panel::bottom("grimlite_accounts_bottom")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        theme::card(ui, "Add Account", |ui| {
                            let show_w = 46.0;
                            let avail = ui.available_width();
                            let field_w = ((avail - show_w - 10.0) * 0.5).max(80.0);

                            ui.horizontal(|ui| {
                                ui.add(
                                    TextEdit::singleline(&mut self.add_username)
                                        .hint_text("Username")
                                        .desired_width(field_w),
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.add_password)
                                        .hint_text("Password")
                                        .password(!self.show_add_password)
                                        .desired_width(field_w),
                                );
                                let lbl = if self.show_add_password { "Hide" } else { "Show" };
                                if ui
                                    .add_sized([show_w, 20.0], egui::Button::new(lbl))
                                    .clicked()
                                {
                                    self.show_add_password = !self.show_add_password;
                                }
                            });

                            ui.horizontal(|ui| {
                                let can_save = !self.add_username.trim().is_empty()
                                    && !self.add_password.is_empty();
                                if green_action_button(ui, "Save Account", can_save, [96.0, 22.0])
                                    .clicked()
                                {
                                    self.save_current_form();
                                }
                                if ui
                                    .add_sized([110.0, 22.0], egui::Button::new("Import from Form"))
                                    .on_hover_text(
                                        "Read username & password currently typed in AQW login screen",
                                    )
                                    .clicked()
                                {
                                    self.import_from_flash_form();
                                }
                                if !self.status.is_empty() {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&self.status).small().weak(),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                }
                            });
                        });
                    });

                // 3. Center panel: Scrollable account list filling exact remaining space
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let list_h = (ui.available_height() - 8.0).max(60.0);
                        let mut remove_idx: Option<usize> = None;
                        let mut fill_target: Option<SavedAccount> = None;
                        let mut login_target: Option<SavedAccount> = None;

                        theme::list_box(ui, Some(list_h), |ui| {
                            ScrollArea::vertical()
                                .id_salt("grimlite_saved_accounts_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    let q = self.filter.trim().to_ascii_lowercase();

                                    if self.accounts.is_empty() {
                                        ui.label(
                                            RichText::new(
                                                "No saved accounts yet. Add one below or click \"Import from Form\".",
                                            )
                                            .small()
                                            .weak(),
                                        );
                                    } else {
                                        let mut visible_idx = 0usize;
                                        for (idx, acc) in self.accounts.iter().enumerate() {
                                            if !q.is_empty()
                                                && !acc.username.to_ascii_lowercase().contains(&q)
                                            {
                                                continue;
                                            }
                                            let key = acc.username.to_ascii_lowercase();
                                            let revealed = self.revealed_rows.contains(&key);
                                            theme::list_row(
                                                ui,
                                                visible_idx,
                                                false,
                                                0,
                                                |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 4.0;

                                                        // Keep username/password on left and buttons pinned neatly on right.
                                                        let actions_w = 42.0 + 46.0 + 44.0 + 22.0 + 16.0;
                                                        let left_w =
                                                            (ui.available_width() - actions_w).max(90.0);
                                                        let user_w = (left_w * 0.56).max(56.0);
                                                        let pass_w = (left_w - user_w - 4.0).max(36.0);

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(user_w, 20.0),
                                                            egui::Layout::left_to_right(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if ui
                                                                    .add(
                                                                        egui::Label::new(
                                                                            RichText::new(&acc.username)
                                                                                .strong()
                                                                                .size(12.0)
                                                                                .color(Color32::WHITE),
                                                                        )
                                                                        .truncate()
                                                                        .selectable(false)
                                                                        .sense(egui::Sense::click()),
                                                                    )
                                                                    .double_clicked()
                                                                {
                                                                    fill_target = Some(acc.clone());
                                                                }
                                                            },
                                                        );

                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(pass_w, 20.0),
                                                            egui::Layout::left_to_right(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                let pass_txt = if revealed {
                                                                    acc.password.as_str()
                                                                } else {
                                                                    "••••••••"
                                                                };
                                                                if ui
                                                                    .add(
                                                                        egui::Label::new(
                                                                            RichText::new(pass_txt)
                                                                                .monospace()
                                                                                .size(11.0)
                                                                                .color(if revealed {
                                                                                    Palette::TEXT_PRIMARY
                                                                                } else {
                                                                                    Palette::TEXT_MUTED
                                                                                }),
                                                                        )
                                                                        .truncate()
                                                                        .selectable(false)
                                                                        .sense(egui::Sense::click()),
                                                                    )
                                                                    .double_clicked()
                                                                {
                                                                    fill_target = Some(acc.clone());
                                                                }
                                                            },
                                                        );

                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if theme::danger_button(
                                                                    ui,
                                                                    "×",
                                                                    true,
                                                                    [22.0, 20.0],
                                                                )
                                                                .on_hover_text("Delete account")
                                                                .clicked()
                                                                {
                                                                    remove_idx = Some(idx);
                                                                }

                                                                if theme::primary_button(
                                                                    ui,
                                                                    "Fill",
                                                                    [42.0, 20.0],
                                                                )
                                                                .on_hover_text(
                                                                    "Fill username & password into AQW login form",
                                                                )
                                                                .clicked()
                                                                {
                                                                    fill_target = Some(acc.clone());
                                                                }

                                                                if green_action_button(
                                                                    ui,
                                                                    "Login",
                                                                    true,
                                                                    [48.0, 20.0],
                                                                )
                                                                .on_hover_text(
                                                                    "Direct login with this account",
                                                                )
                                                                .clicked()
                                                                {
                                                                    login_target =
                                                                        Some(acc.clone());
                                                                }

                                                                let show_lbl = if revealed {
                                                                    "Hide"
                                                                } else {
                                                                    "Show"
                                                                };
                                                                if ui
                                                                    .add_sized(
                                                                        [40.0, 20.0],
                                                                        egui::Button::new(show_lbl),
                                                                    )
                                                                    .clicked()
                                                                {
                                                                    if revealed {
                                                                        self.revealed_rows
                                                                            .remove(&key);
                                                                    } else {
                                                                        self.revealed_rows
                                                                            .insert(key);
                                                                    }
                                                                }
                                                            },
                                                        );
                                                    });
                                                },
                                            );
                                            visible_idx += 1;
                                        }

                                        if visible_idx == 0 {
                                            ui.label(
                                                RichText::new("No matching accounts.")
                                                    .small()
                                                    .weak(),
                                            );
                                        }
                                    }
                                });
                        });

                        if let Some(idx) = remove_idx
                            && let Some(acc) = self.accounts.get(idx)
                        {
                            self.pending_delete = Some((idx, acc.username.clone()));
                        }
                        if let Some(acc) = fill_target {
                            self.fill_login_form(&acc);
                        }
                        if let Some(acc) = login_target {
                            self.direct_login(&acc);
                            close_after_login = true;
                        }
                    });
            },
        );
        if close_after_login {
            open = false;
        }
        self.open = open;

        if self.open
            && let Some((idx, username)) = self.pending_delete.clone()
        {
            match theme::confirm_dialog(
                ctx,
                "grimlite_confirm_delete_account",
                "Delete Account",
                &format!("Remove saved account \"{username}\"?"),
                "Delete",
                true,
            ) {
                theme::ConfirmAction::Confirmed => {
                    if idx < self.accounts.len() && self.accounts[idx].username == username {
                        let removed = self.accounts.remove(idx);
                        self.revealed_rows
                            .remove(&removed.username.to_ascii_lowercase());
                        save_accounts(&self.accounts);
                        self.status = format!("Removed '{}'", removed.username);
                    }
                    self.pending_delete = None;
                }
                theme::ConfirmAction::Cancelled => {
                    self.pending_delete = None;
                }
                theme::ConfirmAction::Pending => {}
            }
        }
    }

    fn save_current_form(&mut self) {
        let username = self.add_username.trim().to_string();
        let password = self.add_password.clone();
        if username.is_empty() || password.is_empty() {
            return;
        }
        upsert_account(&mut self.accounts, SavedAccount { username: username.clone(), password });
        save_accounts(&self.accounts);
        self.add_username.clear();
        self.add_password.clear();
        self.status = format!("Saved '{username}'");
    }

    fn fill_login_form(&mut self, acc: &SavedAccount) {
        self.bot.spawn_flash_action(
            "setGameObject",
            vec![
                FlashArg::from("mcLogin.ni.text"),
                FlashArg::from(acc.username.as_str()),
            ],
        );
        self.bot.spawn_flash_action(
            "setGameObject",
            vec![
                FlashArg::from("mcLogin.pi.text"),
                FlashArg::from(acc.password.as_str()),
            ],
        );
        self.status = format!("Filled '{}'", acc.username);
    }

    fn direct_login(&mut self, acc: &SavedAccount) {
        self.fill_login_form(acc);
        self.bot.spawn_flash_action(
            "login",
            vec![
                FlashArg::from(acc.username.as_str()),
                FlashArg::from(acc.password.as_str()),
            ],
        );
        self.status = format!("Logging in as '{}'...", acc.username);
    }

    fn import_from_flash_form(&mut self) {
        self.status = "Reading login form...".to_string();
        let bot = Arc::clone(&self.bot);
        let slot = Arc::clone(&self.pending_import);
        let _ = std::thread::Builder::new()
            .name("grimlite-accounts-import".into())
            .spawn(move || {
                let Some(flash) = bot.flash() else {
                    return;
                };
                let username = flash
                    .call_str("getGameObject", &[FlashArg::from("mcLogin.ni.text")])
                    .ok()
                    .map(|s| clean_flash_text(&s))
                    .unwrap_or_default();
                let password = flash
                    .call_str("getGameObject", &[FlashArg::from("mcLogin.pi.text")])
                    .ok()
                    .map(|s| clean_flash_text(&s))
                    .unwrap_or_default();
                if let Ok(mut guard) = slot.lock() {
                    *guard = Some(ImportedForm { username, password });
                }
            });
    }
}

fn green_action_button(
    ui: &mut Ui,
    label: &str,
    enabled: bool,
    size: [f32; 2],
) -> egui::Response {
    ui.scope(|ui| {
        if enabled {
            let w = &mut ui.style_mut().visuals.widgets;
            w.inactive.weak_bg_fill = Palette::SUCCESS_BG;
            w.inactive.bg_fill = Palette::SUCCESS_BG;
            w.inactive.bg_stroke = Stroke::new(1.0, Palette::SUCCESS);
            w.hovered.weak_bg_fill = Color32::from_rgb(26, 82, 54);
            w.hovered.bg_fill = Color32::from_rgb(26, 82, 54);
            w.hovered.bg_stroke = Stroke::new(1.0, Palette::SUCCESS);
        }
        let text = if enabled {
            RichText::new(label).strong().color(Palette::SUCCESS)
        } else {
            RichText::new(label)
        };
        ui.add_enabled(enabled, egui::Button::new(text).min_size(size.into()))
    })
    .inner
}

fn clean_flash_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed == "null" || trimmed == "undefined" {
        return String::new();
    }
    serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_string())
}

fn upsert_account(accounts: &mut Vec<SavedAccount>, entry: SavedAccount) {
    if let Some(existing) = accounts
        .iter_mut()
        .find(|a| a.username.eq_ignore_ascii_case(&entry.username))
    {
        existing.username = entry.username;
        existing.password = entry.password;
    } else {
        accounts.push(entry);
    }
}

fn accounts_file_path() -> PathBuf {
    grimlite_bot::options::app_data_dir().join(ACCOUNTS_FILE_NAME)
}

fn parse_accounts_json(raw: &str) -> Vec<SavedAccount> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| {
            let username = v.get("username")?.as_str()?.trim().to_string();
            let password = v.get("password")?.as_str()?.to_string();
            if username.is_empty() {
                None
            } else {
                Some(SavedAccount { username, password })
            }
        })
        .collect()
}

fn serialize_accounts_json(accounts: &[SavedAccount]) -> String {
    let arr: Vec<serde_json::Value> = accounts
        .iter()
        .map(|a| {
            serde_json::json!({
                "username": a.username,
                "password": a.password,
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string())
}

fn accounts_key_path() -> PathBuf {
    grimlite_bot::options::app_data_dir().join(ACCOUNTS_KEY_FILE_NAME)
}

fn fill_random_bytes(out: &mut [u8]) {
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom")
            && f.read_exact(out).is_ok()
        {
            return;
        }
    }
    // Fallback entropy mixer using OS time, stack address, PID, and HashMap RandomState.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut ctr = 0u32;
    for chunk in out.chunks_mut(32) {
        let mut h = Sha256::new();
        h.update(ctr.to_be_bytes());
        h.update(std::process::id().to_be_bytes());
        if let Ok(d) = SystemTime::now().duration_since(UNIX_EPOCH) {
            h.update(d.as_nanos().to_be_bytes());
        }
        let rs = RandomState::new();
        let mut bh = rs.build_hasher();
        bh.write_u32(ctr);
        h.update(bh.finish().to_be_bytes());
        let digest = h.finalize();
        chunk.copy_from_slice(&digest[..chunk.len()]);
        ctr = ctr.wrapping_add(1);
    }
}

fn load_or_create_master_key() -> [u8; 32] {
    let path = accounts_key_path();
    if let Ok(raw) = std::fs::read(&path)
        && raw.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        return key;
    }

    let mut key = [0u8; 32];
    fill_random_bytes(&mut key);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    key
}

fn derive_subkey(master: &[u8; 32], domain: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(master);
    h.update(domain);
    h.finalize().into()
}

fn hmac_sha256(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..32 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

fn xor_keystream(enc_key: &[u8; 32], nonce: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (block_idx, chunk) in data.chunks(32).enumerate() {
        let mut h = Sha256::new();
        h.update(enc_key);
        h.update(nonce);
        h.update((block_idx as u32).to_be_bytes());
        let stream = h.finalize();
        for (b, s) in chunk.iter().zip(stream.iter()) {
            out.push(b ^ s);
        }
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(LUT[(b >> 4) as usize] as char);
        out.push(LUT[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    let bytes = hex.trim().as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

fn encrypt_payload(master_key: &[u8; 32], plaintext: &[u8]) -> String {
    let enc_key = derive_subkey(master_key, b"grimlite-accounts-enc-v1");
    let mac_key = derive_subkey(master_key, b"grimlite-accounts-mac-v1");
    let mut nonce = [0u8; 16];
    fill_random_bytes(&mut nonce);

    let ciphertext = xor_keystream(&enc_key, &nonce, plaintext);
    let mut body = Vec::with_capacity(16 + ciphertext.len() + 32);
    body.extend_from_slice(&nonce);
    body.extend_from_slice(&ciphertext);
    let tag = hmac_sha256(&mac_key, &body);
    body.extend_from_slice(&tag);

    format!("{ENCRYPTED_PREFIX}{}", hex_encode(&body))
}

fn decrypt_payload(master_key: &[u8; 32], encoded: &str) -> Option<Vec<u8>> {
    let hex_part = encoded.trim().strip_prefix(ENCRYPTED_PREFIX)?;
    let body = hex_decode(hex_part)?;
    if body.len() < 16 + 32 {
        return None;
    }
    let split_tag = body.len() - 32;
    let (nonce_and_ct, stored_tag) = body.split_at(split_tag);

    let enc_key = derive_subkey(master_key, b"grimlite-accounts-enc-v1");
    let mac_key = derive_subkey(master_key, b"grimlite-accounts-mac-v1");
    let expected_tag = hmac_sha256(&mac_key, nonce_and_ct);

    // Constant-time tag comparison
    let mut diff = 0u8;
    for (&a, &b) in stored_tag.iter().zip(expected_tag.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return None;
    }

    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&nonce_and_ct[..16]);
    let ciphertext = &nonce_and_ct[16..];
    Some(xor_keystream(&enc_key, &nonce, ciphertext))
}

fn load_accounts() -> Vec<SavedAccount> {
    let path = accounts_file_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let trimmed = raw.trim();
    if trimmed.starts_with(ENCRYPTED_PREFIX) {
        let key = load_or_create_master_key();
        let Some(plain) = decrypt_payload(&key, trimmed) else {
            return Vec::new();
        };
        let Ok(json) = String::from_utf8(plain) else {
            return Vec::new();
        };
        return parse_accounts_json(&json);
    }

    // Legacy plaintext migration: read once and immediately re-save encrypted.
    let legacy = parse_accounts_json(trimmed);
    if !legacy.is_empty() {
        save_accounts(&legacy);
    }
    legacy
}

fn save_accounts(accounts: &[SavedAccount]) {
    let path = accounts_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serialize_accounts_json(accounts);
    let key = load_or_create_master_key();
    let encrypted = encrypt_payload(&key, json.as_bytes());
    let _ = std::fs::write(&path, encrypted);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_updates_existing_case_insensitively() {
        let mut list = vec![SavedAccount {
            username: "Hero".into(),
            password: "old".into(),
        }];
        upsert_account(
            &mut list,
            SavedAccount {
                username: "hero".into(),
                password: "new".into(),
            },
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].password, "new");
    }

    #[test]
    fn clean_flash_text_handles_json_and_null() {
        assert_eq!(clean_flash_text("\"remake\""), "remake");
        assert_eq!(clean_flash_text("null"), "");
        assert_eq!(clean_flash_text("undefined"), "");
        assert_eq!(clean_flash_text("plain"), "plain");
    }

    #[test]
    fn encrypts_and_decrypts_accounts_with_authentication() {
        let key = [0x42u8; 32];
        let accounts = vec![
            SavedAccount {
                username: "remake".into(),
                password: "super_secret_password!".into(),
            },
            SavedAccount {
                username: "ferretto".into(),
                password: "another_password_123".into(),
            },
        ];
        let json = serialize_accounts_json(&accounts);
        let enc1 = encrypt_payload(&key, json.as_bytes());
        let enc2 = encrypt_payload(&key, json.as_bytes());

        assert!(enc1.starts_with(ENCRYPTED_PREFIX));
        assert!(!enc1.contains("remake"));
        assert!(!enc1.contains("super_secret_password"));
        assert_ne!(enc1, enc2, "each encryption should use a fresh random nonce");

        let decrypted = decrypt_payload(&key, &enc1).expect("valid ciphertext should decrypt");
        let parsed = parse_accounts_json(std::str::from_utf8(&decrypted).unwrap());
        assert_eq!(parsed, accounts);

        // Tampering a single hex char must fail MAC verification.
        let mut tampered = enc1.clone();
        let last = tampered.pop().unwrap();
        tampered.push(if last == '0' { '1' } else { '0' });
        assert!(decrypt_payload(&key, &tampered).is_none());
    }

    #[test]
    fn accounts_window_does_not_grow_in_height() {
        let ctx = egui::Context::default();
        let bot = Bot::new();
        let mut win = AccountsWindow::new(bot);
        win.open = true;
        win.accounts = vec![
            SavedAccount {
                username: "remake".into(),
                password: "secret".into(),
            },
            SavedAccount {
                username: "ferretto".into(),
                password: "secret2".into(),
            },
        ];

        let mut heights = Vec::new();
        for _ in 0..10 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            ));
            let mut out = ctx.run_ui(input, |ui| {
                win.show(ui.ctx());
            });
            out.textures_delta.clear();
            if let Some(r) = ctx.memory(|m| m.area_rect(egui::Id::new("grimlite_account_manager")))
            {
                heights.push(r.height());
            }
        }
        assert!(heights.len() >= 6);
        let base = heights[3];
        let last = *heights.last().unwrap();
        assert!(
            (last - base).abs() <= 1.0,
            "accounts window height drifted from {base} to {last}: {heights:?}"
        );
    }
}
