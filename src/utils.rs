use eframe::egui;
use font_loader::system_fonts;
use std::sync::Arc;

pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let sys_font_props = system_fonts::FontPropertyBuilder::new()
        .family("Microsoft YaHei")
        .family("PingFang SC")
        .family("Noto Sans CJK SC")
        .family("SimHei")
        .build();

    let (font_data, font_name) = match system_fonts::get(&sys_font_props) {
        Some((data, _)) => (data, "system_font"),
        #[allow(non_snake_case)]
        None => return, // 未找到则使用默认
    };

    fonts.font_data.insert(
        font_name.to_owned(),
        Arc::new(egui::FontData::from_owned(font_data)),
    );

    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, font_name.to_owned());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, font_name.to_owned());

    ctx.set_fonts(fonts);
}