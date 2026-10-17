use crate::pdf_document::{PageText, TextChar};
use eframe::egui;
use std::collections::HashMap;

#[derive(Default)]
pub struct TextSelection {
    pub active: bool,
    pub start_pos: egui::Pos2,
    pub end_pos: egui::Pos2,
    pub start_page: u32,
    pub end_page: u32,
    /// (page_index, char_index) pairs of selected chars, in reading order
    pub selected: Vec<(u32, usize)>,
}

impl TextSelection {
    pub fn begin(&mut self, pos: egui::Pos2, page: u32) {
        self.active = true;
        self.start_pos = pos;
        self.end_pos = pos;
        self.start_page = page;
        self.end_page = page;
        self.selected.clear();
    }

    pub fn update(&mut self, pos: egui::Pos2, page: u32) {
        if self.active {
            self.end_pos = pos;
            self.end_page = page;
        }
    }

    pub fn finish(&mut self) {
        self.active = false;
    }

    /// Select chars by text flow between start and end positions.
    /// `px_to_pt` converts render-pixel char coords to logical screen points.
    pub fn compute_selected_chars(
        &mut self,
        page_num: u32,
        chars: &[TextChar],
        page_offset: egui::Vec2,
        px_to_pt: f32,
    ) {
        // Convert screen positions to page-local logical coordinates
        let start = self.start_pos - page_offset;
        let end = self.end_pos - page_offset;

        let start_idx = closest_char_index(chars, start, px_to_pt);
        let end_idx = closest_char_index(chars, end, px_to_pt);

        if start_idx.is_none() || end_idx.is_none() {
            return;
        }

        let start_idx = start_idx.unwrap();
        let end_idx = end_idx.unwrap();

        let (from, to) = if start_idx <= end_idx {
            (start_idx, end_idx)
        } else {
            (end_idx, start_idx)
        };

        self.selected.clear();
        for i in from..=to {
            self.selected.push((page_num, i));
        }
    }

    pub fn selected_text(&self, page_texts: &HashMap<u32, PageText>) -> String {
        let mut result = String::new();
        let mut last_y: Option<f32> = None;

        for &(page_num, char_idx) in &self.selected {
            if let Some(page_text) = page_texts.get(&page_num) {
                if let Some(ch) = page_text.chars.get(char_idx) {
                    if let Some(prev_y) = last_y {
                        let line_height = ch.y1 - ch.y0;
                        if (ch.y0 - prev_y).abs() > line_height * 0.5 {
                            result.push('\n');
                        }
                    }
                    last_y = Some(ch.y0);
                    result.push(ch.c);
                }
            }
        }
        result
    }
}

/// Find the index of the char closest to a point.
/// Char coords are in render pixels; pos is in logical points.
fn closest_char_index(chars: &[TextChar], pos: egui::Pos2, px_to_pt: f32) -> Option<usize> {
    if chars.is_empty() {
        return None;
    }

    let mut best_idx = 0;
    let mut best_dist = f32::MAX;

    for (i, ch) in chars.iter().enumerate() {
        // Convert char center from render pixels to logical points
        let cx = (ch.x0 + ch.x1) * 0.5 * px_to_pt;
        let cy = (ch.y0 + ch.y1) * 0.5 * px_to_pt;
        let dist = (pos.x - cx).powi(2) + (pos.y - cy).powi(2);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    Some(best_idx)
}
