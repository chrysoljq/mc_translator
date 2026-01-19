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

    let mono_font_props = system_fonts::FontPropertyBuilder::new()
        .family("Consolas")
        .family("JetBrains Mono")
        .family("Menlo")
        .family("Monaco")
        .family("Courier New")
        .build();

    if let Some((data, _)) = system_fonts::get(&sys_font_props) {
        fonts.font_data.insert(
            "my_ui_font".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
        fonts.families.entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "my_ui_font".to_owned());
    }

    if let Some((data, _)) = system_fonts::get(&mono_font_props) {
        fonts.font_data.insert(
            "my_code_font".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
        fonts.families.entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "my_code_font".to_owned());
    } else {
        if fonts.font_data.contains_key("my_ui_font") {
            fonts.families.entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "my_ui_font".to_owned());
        }
    }

    if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        vec.push("my_ui_font".to_owned());
    }
    ctx.set_fonts(fonts);
}
