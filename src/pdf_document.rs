use mupdf::pdf::PdfDocument;
use mupdf::{Colorspace, Matrix, TextPageOptions};
use std::path::Path;

pub struct PdfDoc {
    doc: PdfDocument,
    page_count: u32,
}

pub struct RenderedPage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct TextChar {
    pub c: char,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Clone, Debug)]
pub struct PageText {
    pub chars: Vec<TextChar>,
}

impl PdfDoc {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, mupdf::Error> {
        let doc = PdfDocument::open(path.as_ref().to_str().unwrap_or(""))?;
        let page_count = doc.page_count()? as u32;
        Ok(Self { doc, page_count })
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn render_page(&self, page_num: u32, scale: f32) -> Result<RenderedPage, mupdf::Error> {
        let page = self.doc.load_page(page_num as i32)?;
        let matrix = Matrix::new_scale(scale, scale);
        let pixmap = page.to_pixmap(&matrix, &Colorspace::device_rgb(), 1.0, true)?;
        let width = pixmap.width() as u32;
        let height = pixmap.height() as u32;
        let samples = pixmap.samples().to_vec();

        // MuPDF returns RGBA when alpha=true
        Ok(RenderedPage {
            pixels: samples,
            width,
            height,
        })
    }

    pub fn page_size(&self, page_num: u32) -> Result<(f32, f32), mupdf::Error> {
        let page = self.doc.load_page(page_num as i32)?;
        let bounds = page.bounds()?;
        Ok((bounds.x1 - bounds.x0, bounds.y1 - bounds.y0))
    }

    pub fn extract_text(&self, page_num: u32, scale: f32) -> Result<PageText, mupdf::Error> {
        let page = self.doc.load_page(page_num as i32)?;
        let text_page = page.to_text_page(TextPageOptions::empty())?;
        let mut chars = Vec::new();

        for block in text_page.blocks() {
            for line in block.lines() {
                for ch in line.chars() {
                    if let Some(c) = ch.char() {
                        let quad = ch.quad();
                        chars.push(TextChar {
                            c,
                            x0: quad.ul.x * scale,
                            y0: quad.ul.y * scale,
                            x1: quad.lr.x * scale,
                            y1: quad.lr.y * scale,
                        });
                    }
                }
            }
        }

        Ok(PageText { chars })
    }

    pub fn search_page(&self, page_num: u32, query: &str, scale: f32) -> Result<Vec<[f32; 4]>, mupdf::Error> {
        let page = self.doc.load_page(page_num as i32)?;
        let hits = page.search(query, 256)?;
        Ok(hits
            .into_iter()
            .map(|q| {
                [
                    q.ul.x * scale,
                    q.ul.y * scale,
                    q.lr.x * scale,
                    q.lr.y * scale,
                ]
            })
            .collect())
    }
}
