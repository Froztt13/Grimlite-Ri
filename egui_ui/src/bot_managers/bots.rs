//! Bot Manager — Bots tab (Load / Save `.gbot`).

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::text::LayoutJob;
use egui::{
    Color32, ColorImage, FontId, RichText, ScrollArea, Stroke, TextEdit, TextFormat, TextureHandle,
    TextureOptions, Ui,
};
use grimlite_bot::gbot::BotConfig;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

impl BotManagerWindow {
    // ---- Tab 9: Bots (Load / Save .gbot) ----

    pub(super) fn show_bots_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig, dirty: &mut bool) {
        let panel_h = ui.available_height().max(180.0);
        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Bot File (.gbot)", |ui| {
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Load .gbot...", [half, 22.0]).clicked()
                        && let Some(pick) = &self.pick_gbot
                        && let Some(path) = pick()
                    {
                        match self.bot.load_gbot_file(&path) {
                            Ok(()) => {
                                *cfg = self.bot.runner.config();
                                self.selected.clear();
                                self.anchor = None;
                                let name = file_name_or_path(&path);
                                self.status = format!("Loaded: {name}");
                            }
                            Err(e) => self.status = format!("Load error: {e}"),
                        }
                    }
                    if theme::primary_button(ui, "Save .gbot...", [half, 22.0]).clicked()
                        && let Some(save) = &self.save_gbot
                    {
                        self.bot.runner.update_config(cfg.clone());
                        if let Some(path) = save() {
                            match self.bot.save_gbot_file(&path) {
                                Ok(()) => {
                                    let name = file_name_or_path(&path);
                                    self.status = format!("Saved: {name}");
                                }
                                Err(e) => self.status = format!("Save error: {e}"),
                            }
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Author").small().weak());
                *dirty |= ui
                    .add(
                        TextEdit::singleline(&mut cfg.author)
                            .hint_text("Bot author")
                            .desired_width(f32::INFINITY),
                    )
                    .changed();

                if !self.status.is_empty() {
                    ui.add_space(5.0);
                    ui.label(RichText::new(&self.status).small().weak());
                }
            });

            cols[1].set_min_height(panel_h);
            group_box(&mut cols[1], "Description", |ui| {
                let is_rtf = is_rtf_text(&cfg.description);
                let edit_id = ui.id().with("desc_raw_edit");
                let mut edit_raw = is_rtf
                    && ui.ctx().data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);

                if is_rtf {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let lbl = if edit_raw { "Preview RTF" } else { "Edit Raw" };
                            if ui.small_button(lbl).clicked() {
                                edit_raw = !edit_raw;
                                ui.ctx().data_mut(|d| d.insert_temp(edit_id, edit_raw));
                            }
                        });
                    });
                }

                let box_h = (ui.available_height() - 6.0).max(120.0);
                if is_rtf && !edit_raw {
                    let cached = cached_rtf_doc(ui.ctx(), &cfg.description);
                    egui::Frame::new()
                        .fill(theme::Palette::BG_INPUT)
                        .stroke(Stroke::new(1.0, theme::Palette::BORDER_SUBTLE))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_size(egui::vec2(ui.available_width(), box_h));
                            ScrollArea::vertical()
                                .id_salt("bots_desc_rtf_scroll")
                                .auto_shrink([false, false])
                                .max_height(box_h - 16.0)
                                .show(ui, |ui| {
                                    if cached.blocks.is_empty() {
                                        ui.label(
                                            RichText::new("No description.")
                                                .small()
                                                .weak()
                                                .italics(),
                                        );
                                    } else {
                                        let max_w = ui.available_width().max(40.0);
                                        for block in cached.blocks.iter() {
                                            match block {
                                                CachedRtfBlock::Text(job) => {
                                                    ui.label(job.clone());
                                                }
                                                CachedRtfBlock::Image {
                                                    texture,
                                                    goal_size,
                                                } => {
                                                    let tex_sz = texture.size_vec2();
                                                    let mut sz = goal_size
                                                        .map(|[w, h]| egui::vec2(w, h))
                                                        .unwrap_or(tex_sz);
                                                    if sz.x > max_w && sz.x > 0.0 {
                                                        let s = max_w / sz.x;
                                                        sz *= s;
                                                    }
                                                    ui.image(egui::load::SizedTexture::new(
                                                        texture.id(),
                                                        sz,
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                });
                        });
                } else {
                    *dirty |= ui
                        .add_sized(
                            [ui.available_width(), box_h],
                            TextEdit::multiline(&mut cfg.description)
                                .hint_text("Notes or requirements for this bot..."),
                        )
                        .changed();
                }
            });
        });
    }
}

fn file_name_or_path(path: &std::path::Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn is_rtf_text(s: &str) -> bool {
    s.trim_start().starts_with("{\\rtf")
}

#[derive(Clone, Copy)]
struct RtfStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    font_size: f32,
    color: Color32,
    uc_skip: usize,
}

impl RtfStyle {
    fn default_with(color: Color32) -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            font_size: 13.0,
            color,
            uc_skip: 1,
        }
    }

    fn to_text_format(self) -> TextFormat {
        TextFormat {
            font_id: FontId::proportional(self.font_size),
            color: self.color,
            italics: self.italic,
            underline: if self.underline {
                Stroke::new(1.0, self.color)
            } else {
                Stroke::NONE
            },
            strikethrough: if self.strike {
                Stroke::new(1.0, self.color)
            } else {
                Stroke::NONE
            },
            // egui's proportional font doesn't have a separate bold weight, so brighten slightly
            // when bold is set if using default color.
            ..Default::default()
        }
    }
}

enum RtfBlock {
    Text(LayoutJob),
    Image {
        rgba: ColorImage,
        goal_size: Option<[f32; 2]>,
    },
}

enum CachedRtfBlock {
    Text(LayoutJob),
    Image {
        texture: TextureHandle,
        goal_size: Option<[f32; 2]>,
    },
}

#[derive(Clone)]
struct CachedRtfDoc {
    hash: u64,
    blocks: Arc<Vec<CachedRtfBlock>>,
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn cached_rtf_doc(ctx: &egui::Context, rtf: &str) -> CachedRtfDoc {
    let h = hash_str(rtf);
    let cache_id = egui::Id::new("bots_rtf_doc_cache");
    if let Some(cached) = ctx.data(|d| d.get_temp::<CachedRtfDoc>(cache_id))
        && cached.hash == h
    {
        return cached;
    }

    let parsed = parse_rtf_blocks(rtf, theme::Palette::TEXT_PRIMARY);
    let mut blocks = Vec::with_capacity(parsed.len());
    for (idx, block) in parsed.into_iter().enumerate() {
        match block {
            RtfBlock::Text(job) => {
                if !job.text.trim().is_empty() {
                    blocks.push(CachedRtfBlock::Text(job));
                }
            }
            RtfBlock::Image { rgba, goal_size } => {
                let texture = ctx.load_texture(
                    format!("bots_rtf_img_{h}_{idx}"),
                    rgba,
                    TextureOptions::LINEAR,
                );
                blocks.push(CachedRtfBlock::Image { texture, goal_size });
            }
        }
    }

    let doc = CachedRtfDoc {
        hash: h,
        blocks: Arc::new(blocks),
    };
    ctx.data_mut(|d| d.insert_temp(cache_id, doc.clone()));
    doc
}

struct PictBuilder {
    depth: usize,
    pic_w_goal: Option<i32>,
    pic_h_goal: Option<i32>,
    scale_x: i32,
    scale_y: i32,
    raw_bytes: Vec<u8>,
    hex_buf: Vec<u8>,
}

impl PictBuilder {
    fn new(depth: usize) -> Self {
        Self {
            depth,
            pic_w_goal: None,
            pic_h_goal: None,
            scale_x: 100,
            scale_y: 100,
            raw_bytes: Vec::new(),
            hex_buf: Vec::new(),
        }
    }

    fn flush_hex(&mut self) {
        for pair in self.hex_buf.chunks_exact(2) {
            if let Ok(s) = std::str::from_utf8(pair)
                && let Ok(b) = u8::from_str_radix(s, 16)
            {
                self.raw_bytes.push(b);
            }
        }
        self.hex_buf.clear();
    }

    fn build(mut self) -> Option<RtfBlock> {
        self.flush_hex();
        if self.raw_bytes.is_empty() {
            return None;
        }
        let img = image::load_from_memory(&self.raw_bytes).ok()?;
        let rgba8 = img.to_rgba8();
        let (w, h) = (rgba8.width() as usize, rgba8.height() as usize);
        let rgba = ColorImage::from_rgba_unmultiplied([w, h], rgba8.as_raw());
        let goal_size = match (self.pic_w_goal, self.pic_h_goal) {
            (Some(wg), Some(hg)) if wg > 0 && hg > 0 => {
                let sx = (self.scale_x.max(1) as f32) / 100.0;
                let sy = (self.scale_y.max(1) as f32) / 100.0;
                Some([(wg as f32 / 20.0) * sx, (hg as f32 / 20.0) * sy])
            }
            _ => None,
        };
        Some(RtfBlock::Image { rgba, goal_size })
    }
}

fn parse_rtf_blocks(rtf: &str, default_color: Color32) -> Vec<RtfBlock> {
    let mut blocks: Vec<RtfBlock> = Vec::new();
    let mut job = LayoutJob::default();
    let bytes = rtf.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    let mut color_table: Vec<Color32> = vec![default_color];
    let mut style = RtfStyle::default_with(default_color);
    let mut stack: Vec<(RtfStyle, usize)> = Vec::new();
    let mut skip_depth: Option<usize> = None;
    let mut depth: usize = 0;

    // Color table & picture parsing state
    let mut in_colortbl = false;
    let mut colortbl_depth = 0usize;
    let mut cur_rgb: (Option<u8>, Option<u8>, Option<u8>) = (None, None, None);
    let mut pict: Option<PictBuilder> = None;

    let push_text = |job: &mut LayoutJob, text: &str, style: RtfStyle| {
        if text.is_empty() {
            return;
        }
        let mut fmt = style.to_text_format();
        if style.bold && style.color == default_color {
            fmt.color = Color32::WHITE;
        }
        job.append(text, 0.0, fmt);
    };

    while i < len {
        let b = bytes[i];
        match b {
            b'{' => {
                depth += 1;
                stack.push((style, depth));
                i += 1;
                // Check for ignorable destination `{\*` (keep `{\*\shppict` so embedded images render)
                if skip_depth.is_none()
                    && pict.is_none()
                    && i + 1 < len
                    && bytes[i] == b'\\'
                    && bytes[i + 1] == b'*'
                {
                    if bytes[i + 2..].starts_with(b"\\shppict") {
                        i += 2;
                    } else {
                        skip_depth = Some(depth);
                        i += 2;
                    }
                }
            }
            b'}' => {
                if let Some(ref p) = pict
                    && depth == p.depth
                {
                    let finished = pict.take().unwrap();
                    if let Some(img_block) = finished.build() {
                        if !job.text.trim().is_empty() {
                            blocks.push(RtfBlock::Text(std::mem::take(&mut job)));
                        } else {
                            job = LayoutJob::default();
                        }
                        blocks.push(img_block);
                    }
                }
                if in_colortbl && depth == colortbl_depth {
                    in_colortbl = false;
                }
                if skip_depth == Some(depth) {
                    skip_depth = None;
                }
                if let Some((prev_style, _)) = stack.pop() {
                    style = prev_style;
                }
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= len {
                    break;
                }
                let next = bytes[i];
                if next == b'\\' || next == b'{' || next == b'}' {
                    if skip_depth.is_none() && !in_colortbl && pict.is_none() {
                        let ch = next as char;
                        let mut buf = [0u8; 4];
                        push_text(&mut job, ch.encode_utf8(&mut buf), style);
                    }
                    i += 1;
                } else if next == b'\'' {
                    // Hex escape \'hh (Windows-1252)
                    i += 1;
                    if i + 2 <= len
                        && let Ok(hex_str) = std::str::from_utf8(&bytes[i..i + 2])
                        && let Ok(code) = u8::from_str_radix(hex_str, 16)
                    {
                        if skip_depth.is_none() && !in_colortbl && pict.is_none() {
                            let ch = decode_win1252(code);
                            let mut buf = [0u8; 4];
                            push_text(&mut job, ch.encode_utf8(&mut buf), style);
                        }
                        i += 2;
                    }
                } else if next == b'~' {
                    if skip_depth.is_none() && !in_colortbl && pict.is_none() {
                        push_text(&mut job, " ", style);
                    }
                    i += 1;
                } else if next == b'\r' || next == b'\n' {
                    if skip_depth.is_none() && !in_colortbl && pict.is_none() {
                        push_text(&mut job, "\n", style);
                    }
                    i += 1;
                } else if next.is_ascii_alphabetic() {
                    let word_start = i;
                    while i < len && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&bytes[word_start..i]).unwrap_or("");
                    let mut param: Option<i32> = None;
                    if i < len && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                        let p_start = i;
                        if bytes[i] == b'-' {
                            i += 1;
                        }
                        while i < len && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                        param = std::str::from_utf8(&bytes[p_start..i])
                            .ok()
                            .and_then(|s| s.parse::<i32>().ok());
                    }
                    // Single space delimiter after control word is consumed
                    if i < len && bytes[i] == b' ' {
                        i += 1;
                    }

                    if skip_depth.is_some() {
                        continue;
                    }

                    if let Some(ref mut p) = pict {
                        match word {
                            "picwgoal" => p.pic_w_goal = param,
                            "pichgoal" => p.pic_h_goal = param,
                            "picscalex" => {
                                if let Some(v) = param {
                                    p.scale_x = v;
                                }
                            }
                            "picscaley" => {
                                if let Some(v) = param {
                                    p.scale_y = v;
                                }
                            }
                            "bin" => {
                                if let Some(n) = param
                                    && n > 0
                                {
                                    let n = n as usize;
                                    let end = (i + n).min(len);
                                    p.raw_bytes.extend_from_slice(&bytes[i..end]);
                                    i = end;
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if in_colortbl {
                        match word {
                            "red" => cur_rgb.0 = param.map(|v| v.clamp(0, 255) as u8),
                            "green" => cur_rgb.1 = param.map(|v| v.clamp(0, 255) as u8),
                            "blue" => cur_rgb.2 = param.map(|v| v.clamp(0, 255) as u8),
                            _ => {}
                        }
                        continue;
                    }

                    match word {
                        "fonttbl" | "stylesheet" | "info" | "object" | "fldinst" | "header"
                        | "footer" | "nonshppict" => {
                            skip_depth = Some(depth);
                        }
                        "pict" => {
                            pict = Some(PictBuilder::new(depth));
                        }
                        "colortbl" => {
                            in_colortbl = true;
                            colortbl_depth = depth;
                            color_table.clear();
                            cur_rgb = (None, None, None);
                        }
                        "par" | "line" => push_text(&mut job, "\n", style),
                        "tab" => push_text(&mut job, "    ", style),
                        "emdash" => push_text(&mut job, "—", style),
                        "endash" => push_text(&mut job, "–", style),
                        "bullet" => push_text(&mut job, "•", style),
                        "lquote" => push_text(&mut job, "‘", style),
                        "rquote" => push_text(&mut job, "’", style),
                        "ldblquote" => push_text(&mut job, "“", style),
                        "rdblquote" => push_text(&mut job, "”", style),
                        "b" => style.bold = param != Some(0),
                        "i" => style.italic = param != Some(0),
                        "ul" => style.underline = param != Some(0),
                        "ulnone" => style.underline = false,
                        "strike" => style.strike = param != Some(0),
                        "fs" => {
                            if let Some(hp) = param {
                                style.font_size = (hp as f32 * 0.5).clamp(10.0, 22.0);
                            }
                        }
                        "cf" => {
                            let idx = param.unwrap_or(0).max(0) as usize;
                            style.color = color_table.get(idx).copied().unwrap_or(default_color);
                        }
                        "plain" => {
                            style = RtfStyle::default_with(default_color);
                        }
                        "uc" => {
                            if let Some(n) = param {
                                style.uc_skip = n.max(0) as usize;
                            }
                        }
                        "u" => {
                            if let Some(code) = param {
                                let cp = if code < 0 {
                                    (code + 65536) as u32
                                } else {
                                    code as u32
                                };
                                if let Some(ch) = char::from_u32(cp) {
                                    let mut buf = [0u8; 4];
                                    push_text(&mut job, ch.encode_utf8(&mut buf), style);
                                }
                                // Skip `uc_skip` fallback ASCII characters
                                let mut skipped = 0;
                                while skipped < style.uc_skip && i < len {
                                    if bytes[i] == b'{' || bytes[i] == b'}' {
                                        break;
                                    }
                                    i += 1;
                                    skipped += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    i += 1;
                }
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            _ => {
                if let Some(ref mut p) = pict {
                    if b.is_ascii_hexdigit() {
                        p.hex_buf.push(b);
                    }
                    i += 1;
                } else if in_colortbl {
                    if b == b';' {
                        let c = match cur_rgb {
                            (Some(r), Some(g), Some(b)) => Color32::from_rgb(r, g, b),
                            _ => default_color,
                        };
                        color_table.push(c);
                        cur_rgb = (None, None, None);
                    }
                    i += 1;
                } else if skip_depth.is_some() {
                    i += 1;
                } else {
                    let start = i;
                    while i < len && !matches!(bytes[i], b'{' | b'}' | b'\\' | b'\r' | b'\n') {
                        i += 1;
                    }
                    let slice = String::from_utf8_lossy(&bytes[start..i]);
                    push_text(&mut job, &slice, style);
                }
            }
        }
    }

    if !job.text.trim().is_empty() {
        blocks.push(RtfBlock::Text(job));
    }
    blocks
}

#[cfg(test)]
fn rtf_to_layout_job(rtf: &str, default_color: Color32) -> LayoutJob {
    parse_rtf_blocks(rtf, default_color)
        .into_iter()
        .find_map(|b| match b {
            RtfBlock::Text(job) => Some(job),
            RtfBlock::Image { .. } => None,
        })
        .unwrap_or_default()
}

fn decode_win1252(code: u8) -> char {
    match code {
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        _ => code as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grimoire_rtf_description() {
        let rtf = r"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fnil Microsoft Sans Serif;}}{\colortbl ;\red255\green100\blue50;}{\*\generator Riched20}\pard\cf1\b\fs28 Note:\cf0\b0\par \i Use CAV!\par}";
        let job = rtf_to_layout_job(rtf, Color32::LIGHT_GRAY);
        assert_eq!(job.text, "Note:\nUse CAV!\n");
        assert!(job.sections.len() >= 2);
        assert_eq!(job.sections[0].format.color, Color32::from_rgb(255, 100, 50));
    }

    #[test]
    fn parses_rtf_embedded_pict_image() {
        // 1x1 red PNG in hex
        let png_hex = "89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c49444154789c63f8cfc0000003010100c9fe92ef0000000049454e44ae426082";
        let rtf = format!(
            r"{{\rtf1\ansi Hello\par {{\*\shppict{{\pict\pngblip\picwgoal400\pichgoal400 {png_hex}}}}}\par World}}"
        );
        let blocks = parse_rtf_blocks(&rtf, Color32::LIGHT_GRAY);
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], RtfBlock::Text(j) if j.text.trim() == "Hello"));
        assert!(matches!(
            &blocks[1],
            RtfBlock::Image {
                rgba,
                goal_size: Some([w, h]),
            } if rgba.size == [1, 1] && (*w - 20.0).abs() < 0.01 && (*h - 20.0).abs() < 0.01
        ));
        assert!(matches!(&blocks[2], RtfBlock::Text(j) if j.text.trim() == "World"));
    }
}
