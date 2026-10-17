mod app;
mod pdf_document;
mod state;
mod text_selection;

use clap::Parser;

#[derive(Parser)]
#[command(name = "flpdf", about = "Fast, minimal PDF viewer")]
struct Args {
    /// PDF file to open
    file: Option<String>,
}

fn main() -> eframe::Result<()> {
    let args = Args::parse();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("flpdf"),
        ..Default::default()
    };

    eframe::run_native(
        "flpdf",
        options,
        Box::new(move |_cc| Ok(Box::new(app::FlpdfApp::new(args.file)))),
    )
}
