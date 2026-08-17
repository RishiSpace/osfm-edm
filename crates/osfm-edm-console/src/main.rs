//! Native OSFM-EDM admin console — egui, no browser.

mod api;
mod app;
mod model;

use clap::Parser;
use eframe::egui::{self, Color32};

use crate::api::Api;
use crate::app::Console;

#[derive(Parser, Debug)]
#[command(name = "osfm-edm-console", about = "Native OSFM-EDM console")]
struct Cli {
    /// API base URL (the Axum server, not a web bundle).
    #[arg(long, default_value = "http://localhost:8080")]
    api: String,
}

fn main() -> eframe::Result {
    let cli = Cli::parse();
    let api = Api::new(cli.api).expect("failed to build HTTP client");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([800.0, 520.0])
            .with_title("OSFM-EDM"),
        ..Default::default()
    };

    eframe::run_native(
        "OSFM-EDM",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(Console::new(api)))
        }),
    )
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(0x0e, 0x0e, 0x10);
    visuals.window_fill = Color32::from_rgb(0x0e, 0x0e, 0x10);
    visuals.extreme_bg_color = Color32::from_rgb(0x05, 0x05, 0x05);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x1b, 0x1b, 0x20);
    visuals.selection.bg_fill = Color32::from_rgb(0x15, 0xda, 0xe3);
    visuals.selection.stroke.color = Color32::from_rgb(0x05, 0x05, 0x05);
    ctx.set_visuals(visuals);
}
