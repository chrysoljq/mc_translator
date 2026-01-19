#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // 发布时隐藏控制台

mod config;
mod logging;
mod message;
mod logic;
mod ui {
    pub mod app;
    pub mod icon;
    pub mod fonts;
}

use ui::app::MyApp;

use crate::ui::icon::load_icon;

fn main() -> eframe::Result {
    // 初始化日志系统（可选）
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([810.0, 500.0])
            .with_title("MC Mod Translator")
            .with_icon(load_icon()),
        ..Default::default()
    };
    
    eframe::run_native(
        "MC Translator",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}