//! Native OSFM-EDM admin console — egui, no browser.

mod api;
mod app;
mod model;

use clap::Parser;
use eframe::egui::{self, Color32};

use crate::api::{Api, TlsOpts};
use crate::app::Console;

#[derive(Parser, Debug)]
#[command(name = "osfm-edm-console", about = "Native OSFM-EDM console")]
struct Cli {
    /// API base URL (the Axum server, not a web bundle).
    #[arg(long, default_value = "https://localhost:8080")]
    api: String,
    /// PEM file of the server CA.
    #[arg(long)]
    ca: Option<String>,
    /// SHA-256 hex of the CA DER (from the server log).
    #[arg(long)]
    ca_fingerprint: Option<String>,
    /// Accept any TLS certificate (MITM). Opt-in.
    #[arg(long, default_value_t = false)]
    insecure: bool,
}

fn main() -> eframe::Result {
    let cli = Cli::parse();
    let tls = resolve_tls(&cli.api, cli.ca.as_deref(), cli.ca_fingerprint.as_deref(), cli.insecure)
        .expect("TLS setup");
    let api = Api::new(cli.api, tls).expect("failed to build HTTP client");

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

fn resolve_tls(
    url: &str,
    ca_path: Option<&str>,
    fingerprint: Option<&str>,
    insecure: bool,
) -> Result<TlsOpts, String> {
    if insecure {
        eprintln!("warning: --insecure, TLS not verified");
        return Ok(TlsOpts {
            ca_pem: None,
            insecure: true,
        });
    }
    if let Some(path) = ca_path {
        let pem = std::fs::read(path).map_err(|e| e.to_string())?;
        return Ok(TlsOpts {
            ca_pem: Some(pem),
            insecure: false,
        });
    }
    if url.starts_with("https://") {
        let fp = fingerprint.ok_or_else(|| {
            "HTTPS requires --ca, --ca-fingerprint (from the server log), or --insecure".to_string()
        })?;
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?;
        let pem = client
            .get(format!("{}/ca.crt", url.trim_end_matches('/')))
            .send()
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .bytes()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&pem);
        let got = fingerprint_der(&text).ok_or_else(|| "CA PEM had no cert".to_string())?;
        let want: String = fp.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_ascii_lowercase();
        if got != want {
            return Err(format!("CA fingerprint mismatch (got {got})"));
        }
        return Ok(TlsOpts {
            ca_pem: Some(pem.to_vec()),
            insecure: false,
        });
    }
    Ok(TlsOpts {
        ca_pem: None,
        insecure: false,
    })
}

fn fingerprint_der(pem: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut bytes = pem.as_bytes();
    let certs: Vec<_> = rustls_pemfile::certs(&mut bytes).filter_map(|c| c.ok()).collect();
    certs.first().map(|c| format!("{:x}", Sha256::digest(c.as_ref())))
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
