#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // 发布时隐藏控制台

mod config;
mod utils;
mod logging;
mod message;
mod logic;
mod ui {
    pub mod app;
}

use ui::app::MyApp;

fn main() -> eframe::Result {
    // 初始化日志系统（可选）
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([810.0, 500.0])
            .with_title("MC Mod Translator"),
        ..Default::default()
    };
    
    eframe::run_native(
        "MC Translator Rust",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}