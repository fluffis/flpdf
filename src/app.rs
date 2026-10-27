use crate::pdf_document::{PageText, PdfDoc};
use crate::state::AppState;
use crate::text_selection::TextSelection;
use eframe::egui;
use std::collections::HashMap;

const ZOOM_LEVELS: &[f32] = &[0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
const DEFAULT_ZOOM_INDEX: usize = 2; // 1.0x
const PDF_TO_SCREEN: f32 = 96.0 / 72.0; // PDF points (72 DPI) to screen points (96 DPI)
const PAGE_GAP: f32 = 12.0;
const RENDER_BUFFER: u32 = 2;
const MAX_CACHED_TEXTURES: usize = 10;

pub struct FlpdfApp {
    doc: Option<PdfDoc>,
    file_path: String,

    // Rendering — lazy, with eviction
    page_textures: HashMap<u32, egui::TextureHandle>,
    page_heights: Vec<f32>,
    page_texts: HashMap<u32, PageText>,
    zoom_index: usize,
    pixels_per_point: f32, // screen DPI scale (e.g. 1.0, 1.25, 2.0)

    // Navigation
    current_page: u32,
    nav_page: Option<u32>, // explicit page set by PageUp/PageDown, overrides visible-based tracking
    scroll_to_page: Option<u32>,
    go_to_page_buf: String,
    show_go_to_dialog: bool,

    // Text selection
    selection: TextSelection,

    // Search
    show_search: bool,
    focus_search: bool,
    search_query: String,
    search_results: Vec<(u32, Vec<[f32; 4]>)>,
    current_match: usize,
    total_matches: usize,

    // Clipboard
    clipboard: Option<arboard::Clipboard>,

    // Persistent state
    state: AppState,

    // Status
    status_msg: String,
    show_full_path: bool,

    // Metadata modal
    show_metadata: bool,
    metadata_cache: Vec<(String, String)>,

    // Arrow key scroll nudge (in logical points, applied next frame)
    arrow_scroll_delta: f32,
    last_scroll_offset: f32,
}

impl FlpdfApp {
    pub fn new(file_path: Option<String>) -> Self {
        let mut app = Self {
            doc: None,
            file_path: String::new(),
            page_textures: HashMap::new(),
            page_heights: Vec::new(),
            page_texts: HashMap::new(),
            zoom_index: DEFAULT_ZOOM_INDEX,
            pixels_per_point: 1.0, // updated each frame from egui
            current_page: 0,
            nav_page: None,
            scroll_to_page: None,
            go_to_page_buf: String::new(),
            show_go_to_dialog: false,
            selection: TextSelection::default(),
            show_search: false,
            focus_search: false,
            search_query: String::new(),
            search_results: Vec::new(),
            current_match: 0,
            total_matches: 0,
            clipboard: arboard::Clipboard::new().ok(),
            state: AppState::load(),
            status_msg: String::new(),
            show_full_path: false,
            show_metadata: false,
            metadata_cache: Vec::new(),
            arrow_scroll_delta: 0.0,
            last_scroll_offset: 0.0,
        };

        // Set MuPDF anti-aliasing to max (8 bits for text and graphics)
        let mut muctx = mupdf::context::Context::get();
        muctx.set_text_aa_level(8);
        muctx.set_graphics_aa_level(8);

        if let Some(path) = file_path {
            app.open_file(&path);
        }

        app
    }

    /// MuPDF render scale in physical pixels.
    /// Render at physical pixel resolution so GPU displays 1:1.
    fn render_scale(&self) -> f32 {
        self.pixels_per_point * ZOOM_LEVELS[self.zoom_index] * PDF_TO_SCREEN
    }

    /// Logical display size for a page (in egui points, not physical pixels).
    fn logical_page_size(&self, page_num: u32) -> Option<egui::Vec2> {
        self.doc.as_ref().and_then(|doc| {
            doc.page_size(page_num).ok().map(|(w, h)| {
                let zoom = ZOOM_LEVELS[self.zoom_index];
                // PDF points to egui logical points: * (96/72) * zoom
                egui::vec2(w * zoom * PDF_TO_SCREEN, h * zoom * PDF_TO_SCREEN)
            })
        })
    }

    fn open_file(&mut self, path: &str) {
        self.page_textures.clear();
        self.page_heights.clear();
        self.page_texts.clear();
        self.search_results.clear();
        self.selection.selected.clear();
        self.current_page = 0;

        match PdfDoc::open(path) {
            Ok(doc) => {
                self.file_path = path.to_string();
                let pc = doc.page_count();
                self.status_msg = format!(
                    "{} — {} pages",
                    std::path::Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    pc
                );
                // Precompute page heights from bounds (very fast, no rendering)
                self.precompute_page_heights(&doc);
                self.doc = Some(doc);
                self.state.add_recent(path);
            }
            Err(e) => {
                self.status_msg = format!("Error opening file: {e}");
                self.doc = None;
            }
        }
    }

    fn precompute_page_heights(&mut self, doc: &PdfDoc) {
        let zoom = ZOOM_LEVELS[self.zoom_index];
        let pt_scale = zoom * PDF_TO_SCREEN; // PDF points to logical points
        self.page_heights.clear();
        for p in 0..doc.page_count() {
            if let Ok((_w, h)) = doc.page_size(p) {
                self.page_heights.push(h * pt_scale);
            } else {
                self.page_heights.push(800.0);
            }
        }
    }

    fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("PDF", &["pdf"]);
        if let Some(dir) = self.state.last_dir_path() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open_file(&path.to_string_lossy());
        }
    }

    fn render_page_texture(&mut self, ctx: &egui::Context, page_num: u32) {
        if self.page_textures.contains_key(&page_num) {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let scale = self.render_scale();

        match doc.render_page(page_num, scale) {
            Ok(rendered) => {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [rendered.width as usize, rendered.height as usize],
                    &rendered.pixels,
                );
                let texture = ctx.load_texture(
                    format!("page_{page_num}_{}", self.zoom_index),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.page_textures.insert(page_num, texture);
            }
            Err(e) => {
                self.status_msg = format!("Render error page {}: {e}", page_num + 1);
            }
        }
    }

    /// Extract text only when needed (selection or copy)
    fn ensure_text_extracted(&mut self, page_num: u32) {
        if self.page_texts.contains_key(&page_num) {
            return;
        }
        let Some(doc) = &self.doc else { return };
        let scale = self.render_scale();
        if let Ok(text) = doc.extract_text(page_num, scale) {
            self.page_texts.insert(page_num, text);
        }
    }

    fn evict_distant_textures(&mut self) {
        if self.page_textures.len() <= MAX_CACHED_TEXTURES {
            return;
        }
        let current = self.current_page;
        let mut pages: Vec<u32> = self.page_textures.keys().copied().collect();
        pages.sort_by_key(|p| (*p as i64 - current as i64).unsigned_abs());
        // Keep the closest MAX_CACHED_TEXTURES, evict the rest
        for &p in pages.iter().skip(MAX_CACHED_TEXTURES) {
            self.page_textures.remove(&p);
            self.page_texts.remove(&p);
        }
    }

    fn invalidate_cache(&mut self) {
        self.page_textures.clear();
        self.page_texts.clear();
        let zoom = ZOOM_LEVELS[self.zoom_index];
        let pt_scale = zoom * PDF_TO_SCREEN;
        if let Some(doc) = &self.doc {
            self.page_heights.clear();
            for p in 0..doc.page_count() {
                if let Ok((_w, h)) = doc.page_size(p) {
                    self.page_heights.push(h * pt_scale);
                } else {
                    self.page_heights.push(800.0);
                }
            }
        }
        if !self.search_query.is_empty() {
            self.run_search();
        }
    }

    fn zoom_in(&mut self) {
        if self.zoom_index < ZOOM_LEVELS.len() - 1 {
            self.zoom_index += 1;
            self.invalidate_cache();
        }
    }

    fn zoom_out(&mut self) {
        if self.zoom_index > 0 {
            self.zoom_index -= 1;
            self.invalidate_cache();
        }
    }

    fn page_count(&self) -> u32 {
        self.doc.as_ref().map(|d| d.page_count()).unwrap_or(0)
    }

    fn page_y_offset(&self, target_page: u32) -> f32 {
        let mut y: f32 = 0.0;
        for p in 0..target_page as usize {
            if let Some(h) = self.page_heights.get(p) {
                y += h + PAGE_GAP;
            }
        }
        y
    }

    fn run_search(&mut self) {
        self.search_results.clear();
        self.total_matches = 0;
        self.current_match = 0;
        let Some(doc) = &self.doc else { return };
        if self.search_query.is_empty() {
            return;
        }
        let scale = self.render_scale();
        for p in 0..doc.page_count() {
            if let Ok(hits) = doc.search_page(p, &self.search_query, scale) {
                if !hits.is_empty() {
                    self.total_matches += hits.len();
                    self.search_results.push((p, hits));
                }
            }
        }
    }

    fn next_match(&mut self) {
        if self.total_matches == 0 {
            return;
        }
        self.current_match = (self.current_match + 1) % self.total_matches;
        self.scroll_to_current_match();
    }

    fn prev_match(&mut self) {
        if self.total_matches == 0 {
            return;
        }
        if self.current_match == 0 {
            self.current_match = self.total_matches - 1;
        } else {
            self.current_match -= 1;
        }
        self.scroll_to_current_match();
    }

    fn scroll_to_current_match(&mut self) {
        let mut count = 0;
        for (page, hits) in &self.search_results {
            if count + hits.len() > self.current_match {
                self.scroll_to_page = Some(*page);
                break;
            }
            count += hits.len();
        }
    }

    fn copy_selection(&mut self) {
        if self.selection.selected.is_empty() {
            return;
        }
        let text = self.selection.selected_text(&self.page_texts);
        if text.is_empty() {
            return;
        }
        if let Some(cb) = &mut self.clipboard {
            if cb.set_text(&text).is_ok() {
                self.status_msg = format!("Copied {} chars", text.len());
            }
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let ctrl = egui::Modifiers::COMMAND;
        let none = egui::Modifiers::NONE;

        // Ctrl+scroll wheel zoom; detect manual scroll to reset nav_page
        let mut zoom_scroll: f32 = 0.0;
        let mut manual_scroll = false;
        ctx.input_mut(|i| {
            let ctrl_held = i.modifiers.ctrl;
            if ctrl_held {
                i.events.retain(|event| {
                    if let egui::Event::MouseWheel { delta, .. } = event {
                        zoom_scroll += delta.y;
                        return false;
                    }
                    true
                });
                if i.smooth_scroll_delta.y != 0.0 {
                    zoom_scroll += i.smooth_scroll_delta.y;
                }
                i.smooth_scroll_delta = egui::Vec2::ZERO;
                i.raw_scroll_delta = egui::Vec2::ZERO;
            } else {
                // Non-ctrl scroll = manual user scroll
                if i.smooth_scroll_delta.y.abs() > 0.0 || i.raw_scroll_delta.y.abs() > 0.0 {
                    manual_scroll = true;
                }
                for event in &i.events {
                    if matches!(event, egui::Event::MouseWheel { .. }) {
                        manual_scroll = true;
                    }
                }
            }
        });
        if manual_scroll {
            self.nav_page = None;
        }
        if zoom_scroll > 0.0 {
            self.zoom_in();
        } else if zoom_scroll < 0.0 {
            self.zoom_out();
        }

        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::Plus))
            || ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::Equals))
        {
            self.zoom_in();
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::Minus)) {
            self.zoom_out();
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::F)) {
            self.show_search = !self.show_search;
            if self.show_search {
                self.focus_search = true;
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::G)) {
            self.show_go_to_dialog = !self.show_go_to_dialog;
            self.go_to_page_buf.clear();
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::M)) {
            if self.doc.is_some() {
                self.show_metadata = !self.show_metadata;
                if self.show_metadata {
                    self.metadata_cache = self.doc.as_ref().unwrap().metadata();
                }
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::O)) {
            self.open_file_dialog();
        }
        let copy_pressed = ctx.input_mut(|i| {
            i.consume_key(ctrl, egui::Key::C)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::C)
                || i.events.iter().any(|e| matches!(e,
                    egui::Event::Copy | egui::Event::Cut
                ))
        });
        if copy_pressed {
            self.copy_selection();
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::A)) {
            self.ensure_text_extracted(self.current_page);
            if let Some(text) = self.page_texts.get(&self.current_page) {
                self.selection.selected.clear();
                for i in 0..text.chars.len() {
                    self.selection.selected.push((self.current_page, i));
                }
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::W)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::Escape)) {
            self.show_search = false;
            self.show_go_to_dialog = false;
            self.show_metadata = false;
            self.selection.selected.clear();
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::PageDown)) {
            let base = self.nav_page.unwrap_or(self.current_page);
            let next = (base + 1).min(self.page_count().saturating_sub(1));
            self.nav_page = Some(next);
            self.scroll_to_page = Some(next);
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::PageUp)) {
            let base = self.nav_page.unwrap_or(self.current_page);
            let prev = base.saturating_sub(1);
            self.nav_page = Some(prev);
            self.scroll_to_page = Some(prev);
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::ArrowDown)) {
            self.arrow_scroll_delta += 1.0;
            self.nav_page = None;
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::ArrowUp)) {
            self.arrow_scroll_delta -= 1.0;
            self.nav_page = None;
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::Home)) {
            self.scroll_to_page = Some(0);
            self.nav_page = None;
        }
        if ctx.input_mut(|i| i.consume_key(none, egui::Key::End)) {
            self.scroll_to_page = Some(self.page_count().saturating_sub(1));
            self.nav_page = None;
        }
    }
}

impl eframe::App for FlpdfApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Detect screen DPI changes (e.g. moving between monitors)
        let ppp = ctx.pixels_per_point();
        if (ppp - self.pixels_per_point).abs() > 0.01 {
            self.pixels_per_point = ppp;
            if self.doc.is_some() {
                self.invalidate_cache();
            }
        }

        self.handle_keyboard(ctx);

        let page_count = self.page_count();

        // Top bar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.open_file_dialog();
                }
                if !self.state.recent_files.is_empty() {
                    let mut open_path: Option<String> = None;
                    egui::ComboBox::from_id_salt("recent_files")
                        .selected_text("Recent")
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for path in &self.state.recent_files {
                                let label = std::path::Path::new(path)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                if ui.selectable_label(false, label.as_ref())
                                    .on_hover_text(path)
                                    .clicked()
                                {
                                    open_path = Some(path.clone());
                                }
                            }
                        });
                    if let Some(path) = open_path {
                        self.open_file(&path);
                    }
                }
                ui.separator();
                if ui.button("−").clicked() {
                    self.zoom_out();
                }
                ui.label(format!("{}%", (ZOOM_LEVELS[self.zoom_index] * 100.0) as u32));
                if ui.button("+").clicked() {
                    self.zoom_in();
                }
                ui.separator();
                ui.label(format!(
                    "Page {} / {}",
                    self.current_page + 1,
                    page_count
                ));
                ui.separator();
                if ui.button("Search").clicked() {
                    self.show_search = !self.show_search;
                }
            });
        });

        // Search bar
        if self.show_search {
            egui::TopBottomPanel::top("search_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    let response = ui.text_edit_singleline(&mut self.search_query);
                    if self.focus_search {
                        response.request_focus();
                        self.focus_search = false;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if self.search_results.is_empty()
                            || response.changed()
                        {
                            self.run_search();
                        } else if shift {
                            self.prev_match();
                        } else {
                            self.next_match();
                        }
                    }
                    if ui.button("Find").clicked() {
                        self.run_search();
                    }
                    if self.total_matches > 0 {
                        ui.label(format!(
                            "{} / {} matches",
                            self.current_match + 1,
                            self.total_matches
                        ));
                        if ui.button("<").clicked() {
                            self.prev_match();
                        }
                        if ui.button(">").clicked() {
                            self.next_match();
                        }
                    } else if !self.search_query.is_empty() {
                        ui.label("No matches");
                    }
                });
            });
        }

        // Go to page dialog
        if self.show_go_to_dialog {
            egui::TopBottomPanel::top("goto_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Go to page:");
                    let resp = ui.text_edit_singleline(&mut self.go_to_page_buf);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Ok(n) = self.go_to_page_buf.trim().parse::<u32>() {
                            if n >= 1 && n <= page_count {
                                self.scroll_to_page = Some(n - 1);
                                self.show_go_to_dialog = false;
                            }
                        }
                    }
                    if resp.gained_focus() || self.go_to_page_buf.is_empty() {
                        resp.request_focus();
                    }
                });
            });
        }

        // Bottom status bar
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.file_path.is_empty() {
                    let name_part = if self.show_full_path {
                        self.file_path.clone()
                    } else {
                        std::path::Path::new(&self.file_path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    };
                    let page_count_str = self.doc.as_ref()
                        .map(|d| format!(" — {} pages", d.page_count()))
                        .unwrap_or_default();
                    let display_text = format!("{}{}", name_part, page_count_str);
                    let response = ui.add(
                        egui::Label::new(&display_text).sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        self.show_full_path = !self.show_full_path;
                    }
                    if response.secondary_clicked() {
                        if let Some(cb) = &mut self.clipboard {
                            let _ = cb.set_text(&display_text);
                            self.status_msg = "Copied to clipboard".to_string();
                        }
                    }
                } else {
                    ui.label(&self.status_msg);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Ctrl+F: Search | Ctrl+G: Go to | Ctrl+O: Open | Ctrl+C: Copy");
                });
            });
        });

        // Main content
        let bg_color = egui::Color32::from_gray(50);
        let frame = egui::Frame::central_panel(&ctx.style()).fill(bg_color);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if self.doc.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(egui::Color32::WHITE, "Drop a PDF or press Ctrl+O to open");
                });
                return;
            }

            // Scroll-to-page offset
            let mut scrolled_to_page = false;
            let scroll_target = self.scroll_to_page.take().map(|target_page| {
                self.current_page = target_page;
                scrolled_to_page = true;
                self.page_y_offset(target_page)
            });

            let mut scroll_area = egui::ScrollArea::vertical()
                .auto_shrink([false; 2]);

            // Arrow key scroll: 10% of viewport height per press
            let arrow_delta = self.arrow_scroll_delta;
            self.arrow_scroll_delta = 0.0;
            let viewport_height = ui.available_height();

            if let Some(offset) = scroll_target {
                scroll_area = scroll_area.vertical_scroll_offset(offset);
            } else if arrow_delta != 0.0 {
                let step = viewport_height * 0.1;
                let new_offset = (self.last_scroll_offset + arrow_delta * step).max(0.0);
                scroll_area = scroll_area.vertical_scroll_offset(new_offset);
            }

            let clip = ui.clip_rect();
            let mut visible_this_frame: Vec<u32> = Vec::new();

            let scroll_output = scroll_area.show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for page_num in 0..page_count {
                        // Use precomputed height for layout, avoiding a render
                        let page_h = self.page_heights
                            .get(page_num as usize)
                            .copied()
                            .unwrap_or(800.0);

                        // Peek at where this page would be placed
                        let top_y = ui.cursor().top();
                        let bottom_y = top_y + page_h;
                        let is_near_visible = top_y < clip.max.y + page_h * RENDER_BUFFER as f32
                            && bottom_y > clip.min.y - page_h * RENDER_BUFFER as f32;
                        let is_visible = top_y < clip.max.y && bottom_y > clip.min.y;

                        if is_near_visible {
                            // Render this page (or use cached texture)
                            self.render_page_texture(ctx, page_num);
                        }

                        if let Some(texture) = self.page_textures.get(&page_num) {
                            // Display at logical size, not physical pixel size
                            let logical_size = self.logical_page_size(page_num)
                                .unwrap_or(texture.size_vec2());
                            let (rect, response) =
                                ui.allocate_exact_size(logical_size, egui::Sense::click_and_drag());
                            // Scale factor from render pixels to logical points
                            let px_to_pt = if texture.size_vec2().x > 0.0 {
                                logical_size.x / texture.size_vec2().x
                            } else {
                                1.0
                            };

                            if is_visible {
                                visible_this_frame.push(page_num);

                                // White background behind the page
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    egui::Color32::WHITE,
                                );

                                // Page image
                                ui.painter().image(
                                    texture.id(),
                                    rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );

                                // Search highlights (convert render coords to logical)
                                for (sp, hits) in &self.search_results {
                                    if *sp == page_num {
                                        for hit in hits {
                                            let hit_rect = egui::Rect::from_min_max(
                                                rect.min + egui::vec2(hit[0] * px_to_pt, hit[1] * px_to_pt),
                                                rect.min + egui::vec2(hit[2] * px_to_pt, hit[3] * px_to_pt),
                                            );
                                            ui.painter().rect_filled(
                                                hit_rect,
                                                0.0,
                                                egui::Color32::from_rgba_unmultiplied(
                                                    255, 255, 0, 80,
                                                ),
                                            );
                                        }
                                    }
                                }

                                // Text selection highlighting (convert render coords to logical)
                                if let Some(page_text) = self.page_texts.get(&page_num) {
                                    for &(sel_page, char_idx) in &self.selection.selected {
                                        if sel_page == page_num {
                                            if let Some(ch) = page_text.chars.get(char_idx) {
                                                let char_rect = egui::Rect::from_min_max(
                                                    rect.min + egui::vec2(ch.x0 * px_to_pt, ch.y0 * px_to_pt),
                                                    rect.min + egui::vec2(ch.x1 * px_to_pt, ch.y1 * px_to_pt),
                                                );
                                                ui.painter().rect_filled(
                                                    char_rect,
                                                    0.0,
                                                    egui::Color32::from_rgba_unmultiplied(
                                                        100, 149, 237, 100,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            }

                            // Click to deselect
                            if response.clicked() {
                                self.selection.selected.clear();
                            }

                            // Mouse selection — extract text on demand
                            if response.drag_started() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    self.ensure_text_extracted(page_num);
                                    self.selection.begin(pos, page_num);
                                }
                            }
                            if response.dragged() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    self.selection.update(pos, page_num);
                                    self.ensure_text_extracted(page_num);
                                    if let Some(page_text) = self.page_texts.get(&page_num) {
                                        self.selection.compute_selected_chars(
                                            page_num,
                                            &page_text.chars,
                                            rect.min.to_vec2(),
                                            px_to_pt,
                                        );
                                    }
                                }
                            }
                            if response.drag_stopped() {
                                self.selection.finish();
                            }
                        } else {
                            // No texture yet — allocate placeholder space using precomputed height
                            // so scroll area has correct total height
                            let page_w = self.logical_page_size(page_num)
                                .map(|s| s.x)
                                .unwrap_or(600.0);
                            let size = egui::vec2(page_w, page_h);
                            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                            // Draw placeholder
                            if is_visible {
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    egui::Color32::from_gray(220),
                                );
                            }
                        }

                        ui.add_space(PAGE_GAP);
                    }
                });
            });
            self.last_scroll_offset = scroll_output.state.offset.y;

            // Update current page from visible pages, but not on the frame
            // where we just programmatically scrolled (the old page may
            // still be partially visible at the top).
            if !scrolled_to_page {
                if let Some(first_visible) = visible_this_frame.first() {
                    self.current_page = *first_visible;
                }
            }
            self.evict_distant_textures();
        });

        // Metadata modal
        if self.show_metadata {
            let mut open = self.show_metadata;
            egui::Window::new("PDF Metadata")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::Grid::new("metadata_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (label, value) in &self.metadata_cache {
                                ui.strong(label);
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                });
            self.show_metadata = open;
        }

        // Handle dropped files
        let dropped: Option<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .find_map(|f| {
                    f.path
                        .as_ref()
                        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
                        .map(|p| p.to_string_lossy().to_string())
                })
        });
        if let Some(path) = dropped {
            self.open_file(&path);
        }

        // Update window title
        if !self.file_path.is_empty() {
            let fname = std::path::Path::new(&self.file_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{} — Page {} / {} — flpdf",
                fname,
                self.current_page + 1,
                page_count
            )));
        }
    }
}
