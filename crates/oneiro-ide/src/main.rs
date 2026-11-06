#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Oneiro Studio - the IDE for Oneiro projects.

mod ai;
mod app;
mod builder;
mod cutter;
mod editor;
mod highlight;
mod i18n;
mod inspector;
mod library;
mod logo;
mod record;
mod studio;
mod textops;
mod theme;
mod timeline;
mod viewport;
mod welcome;
mod workspace;

use app::{App, StartupOpts};
use eframe::egui;
use std::path::PathBuf;

fn parse_args() -> StartupOpts {
    let mut o = StartupOpts { after_frames: 30, ..Default::default() };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().unwrap_or_default()
        };
        match a {
            "--screenshot" => o.screenshot = Some(PathBuf::from(val())),
            "--after" => o.after_frames = val().parse().unwrap_or(30),
            "--time" => o.time = val().parse().ok(),
            "--tab" => o.tab = Some(val()),
            "--select" => o.select = Some(val()),
            "--free" => o.free_cam = true,
            "--exit" => o.exit_after = true,
            "--demo-record" => o.demo_record = true,
            "--demo-quick" => o.demo_quick = Some(val()),
            "--demo-new" => o.demo_new = true,
            "--logo" => {
                let path = val();
                let _ = logo::render(512).save_png(&path);
                std::process::exit(0);
            }
            "--demo-insert" => o.demo_insert = Some(val()),
            "--export" => o.export = Some(PathBuf::from(val())),
            "--ai-provider" => o.ai_provider = Some(val()),
            "--ai-request" => o.ai_request = Some(val()),
            "--ai-apply" => o.ai_apply = true,
            "--audio-selftest" => o.audio_selftest = true,
            "--cut-image" => o.cut_image = Some(PathBuf::from(val())),
            "--demo-generate" => o.demo_generate = Some(val()),
            "--size" => {
                let v = val();
                if let Some((w, h)) = v.split_once('x') {
                    o.width = w.parse().ok();
                    o.height = h.parse().ok();
                }
            }
            "-h" | "--help" => {
                println!("oneiro-ide [PROJECT] [--screenshot out.png [--after FRAMES] [--exit]] [--time SECONDS] [--tab script|character|inspector] [--select NAME] [--free] [--size WxH]");
                std::process::exit(0);
            }
            s if !s.starts_with('-') => o.project = Some(PathBuf::from(s)),
            _ => {}
        }
        i += 1;
    }
    o
}

fn main() -> eframe::Result<()> {
    i18n::init();
    let opts = parse_args();
    let size = [opts.width.unwrap_or(1500.0), opts.height.unwrap_or(900.0)];
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(size).with_min_inner_size([900.0, 600.0]).with_title("Oneiro Studio").with_icon(std::sync::Arc::new(icon())),
        ..Default::default()
    };
    eframe::run_native("Oneiro Studio", native, Box::new(move |cc| Ok(Box::new(App::new(cc, opts.clone())))))
}

fn icon() -> egui::IconData {
    egui::IconData { rgba: logo::icon_rgba(64), width: 64, height: 64 }
}
