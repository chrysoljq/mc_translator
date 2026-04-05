use super::fonts::setup_custom_fonts;
use crate::config::AppConfig;
use crate::logging::{LogEntry, LogLevel};
use crate::logic::openai::OpenAIClient;
use crate::logic::processor;
use crate::message::{AppMsg, GLOBAL_SENDER};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::thread;
use tokio_util::sync::CancellationToken;

pub struct MyApp {
    config: AppConfig,
    is_processing: bool,
    available_models: Vec<String>,
    logs: Vec<LogEntry>, // <-- 改用 Vec 存储结构化日志
    msg_receiver: Receiver<AppMsg>,
    msg_sender: Sender<AppMsg>,
    cancellation_token: Option<CancellationToken>,
    show_prompt_editor: bool,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(1.1);
        let (sender, receiver) = crossbeam_channel::unbounded();

        let _ = GLOBAL_SENDER.set(sender.clone());

        Self {
            config: AppConfig::load(), // 加载保存的配置
            logs: Vec::new(),
            is_processing: false,
            available_models: vec!["gpt-3.5-turbo".to_string(), "gpt-4o".to_string()],
            msg_receiver: receiver,
            msg_sender: sender,
            cancellation_token: None,
            show_prompt_editor: false,
        }
    }

    fn check_connection_and_fetch_models(&self) {
        let config = self.config.clone();
        let sender = self.msg_sender.clone();

        let _ = sender.send(AppMsg::Log(LogEntry::new(
            LogLevel::Info,
            "正在连接 API 获取模型列表...",
        )));

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let client = OpenAIClient::new(config);
                let token = CancellationToken::new();
                match client.fetch_models(&token).await {
                    Ok(models) => {
                        let _ = sender.send(AppMsg::Log(LogEntry::new(
                            LogLevel::Success,
                            format!("✅ 连接成功！获取到 {} 个模型", models.len()),
                        )));
                        let _ = sender.send(AppMsg::ModelsFetched(models));
                    }
                    Err(e) => {
                        let _ = sender.send(AppMsg::Log(LogEntry::new(
                            LogLevel::Error,
                            format!("❌ 连接失败: {}", e),
                        )));
                    }
                }
            });
        });
    }

    fn start_processing(&mut self, is_update: bool) {
        if self.is_processing {
            return;
        }

        self.is_processing = true;
        // 保存当前配置
        self.config.save();

        let config = self.config.clone();

        // 创建新的 CancellationToken
        let token = CancellationToken::new();
        self.cancellation_token = Some(token.clone());

        let sender = self.msg_sender.clone();
        let completion_msg = if is_update {
            "所有更新任务已完成"
        } else {
            "所有翻译任务已完成"
        };

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                processor::run_processing_task(config, is_update, token).await;
                let _ = sender.send(AppMsg::Log(LogEntry::new(LogLevel::Info, completion_msg)));
            });
        });
    }

    fn cancel_processing(&mut self) {
        if let Some(token) = &self.cancellation_token {
            token.cancel();
            self.logs
                .push(LogEntry::new(LogLevel::Warn, "任务已被用户取消"));
        }
        self.is_processing = false;
        self.cancellation_token = None;
    }

    fn render_prompt_editor(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_prompt_editor;
        let mut should_close = false;

        egui::Window::new("📝 自定义系统提示词 (System Prompt)")
            .open(&mut is_open) // 这里借用的是局部的 is_open
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .vscroll(true)
            .auto_sized()
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.label("在此设置发送给 AI 的系统级指令，可用于控制翻译风格、保留特定术语等。");
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(170.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.config.prompt)
                                .hint_text("请输入 System Prompt...")
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .font(egui::TextStyle::Monospace),
                        );
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("保存并关闭").clicked() {
                            self.config.save();
                            should_close = true;
                        }
                        ui.add_space(5.0);
                        if ui.button("恢复默认").clicked() {
                            self.config.prompt = AppConfig::default().prompt;
                        }
                    });
                });
            });

        if should_close {
            is_open = false;
        }

        self.show_prompt_editor = is_open;
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_prompt_editor(ctx);
        // 处理日志
        while let Ok(msg) = self.msg_receiver.try_recv() {
            match msg {
                AppMsg::Log(entry) => {
                    if self.logs.len() > 1000 {
                        self.logs.remove(0);
                    }
                    if entry.message.contains("已完成") || entry.message.contains("任务终止")
                    {
                        self.is_processing = false;
                        self.cancellation_token = None;
                    }
                    self.logs.push(entry);
                }
                AppMsg::ModelsFetched(models) => {
                    self.available_models = models;
                    // 如果当前配置的模型不在列表里，默认选中第一个
                    if !self.available_models.contains(&self.config.model)
                        && !self.available_models.is_empty()
                    {
                        self.config.model = self.available_models[0].clone();
                    }
                }
            }
        }

        // 底部个人信息
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("v0.2.15").weak().size(10.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.hyperlink_to(
                        egui::RichText::new("GitHub 主页").size(11.0),
                        "https://github.com/chrysoljq/mc_translator",
                    );

                    ui.label(egui::RichText::new("|").weak().size(11.0));

                    ui.hyperlink_to(
                        egui::RichText::new("关于作者").size(11.0),
                        "https://github.com/chrysoljq",
                    );
                });
            });
            ui.add_space(2.0); // 底部留白
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Minecraft Mod 汉化助手（支持任务、模组、资源包）");
            ui.separator();

            egui::Grid::new("settings_grid")
                .num_columns(3)
                .spacing([10.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("BASE URL:");
                    

                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.config.base_url);
                        const LANGUAGES: &[(&str, &str)] = &[
                            ("en_us", "English"),
                            ("zh_cn", "Simplified Chinese"),
                            ("zh_tw", "Traditional Chinese"),
                            ("ja_jp", "Japanese"),
                            ("ko_kr", "Korean"),
                            ("ru_ru", "Russian"),
                            ("fr_fr", "French"),
                            ("es_es", "Spanish"),
                            ("de_de", "German"),
                            ("it_it", "Italian"),
                            ("pt_br", "Brazil"),
                        ];

                        egui::ComboBox::from_id_salt("source_lang")
                            .selected_text(&self.config.source_lang)
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                for (code, name) in LANGUAGES {
                                    ui.selectable_value(
                                        &mut self.config.source_lang,
                                        code.to_string(),
                                        format!("{} - {}", code, name),
                                    );
                                }
                            });

                        if ui.button("→").on_hover_text("交换语言").clicked() {
                            std::mem::swap(&mut self.config.source_lang, &mut self.config.target_lang);
                        }

                        egui::ComboBox::from_id_salt("target_lang")
                            .selected_text(&self.config.target_lang)
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                for (code, name) in LANGUAGES {
                                    ui.selectable_value(
                                        &mut self.config.target_lang,
                                        code.to_string(),
                                        format!("{} - {}", code, name),
                                    );
                                }
                            });
                    });
                    ui.end_row();

                    ui.label("API KEY:");
                    ui.add(egui::TextEdit::singleline(&mut self.config.api_key).password(true));
                    ui.end_row();

                    ui.label("选择模型:");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("model_select")
                            .selected_text(&self.config.model)
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for model in &self.available_models {
                                    ui.selectable_value(
                                        &mut self.config.model,
                                        model.clone(),
                                        model,
                                    );
                                }
                            });

                        if ui.button("🔄 检查 & 刷新").clicked() {
                            if self.config.api_key.is_empty() {
                                self.logs
                                    .push(LogEntry::new(LogLevel::Error, "请先填写 API Key"));
                            } else {
                                self.check_connection_and_fetch_models();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("输入路径:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.config.input_path);
                        if ui.button("📂 打开文件夹").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(&mut self.config.input_path)
                                .pick_folder()
                            {
                                self.config.input_path = path.display().to_string();
                            }
                        }
                        // 没必要了
                        if ui.button("📄 打开文件").clicked() {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("Minecraft Mod", &["jar", "json", "lang"])
                                .set_directory(&mut self.config.input_path)
                                .pick_file()
                            {
                                self.config.input_path = file.display().to_string();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("输出目录:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.config.output_path);
                        if ui.button("📂 选择文件夹").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(&mut self.config.output_path)
                                .pick_folder()
                            {
                                self.config.output_path = path.display().to_string();
                            }
                        }
                    });
                    ui.end_row();
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button("📝 编辑提示词")
                    .on_hover_text("自定义发送给 AI 的系统提示词")
                    .clicked()
                {
                    self.show_prompt_editor = true;
                }
                ui.separator();
                ui.label("批次大小:");
                ui.add(egui::DragValue::new(&mut self.config.batch_size).range(1..=1000))
                    .on_hover_text("影响上下文的处理");
                ui.add_space(10.0);
                ui.checkbox(&mut self.config.skip_existing, "跳过已翻译的文件");
                ui.separator();
                ui.checkbox(&mut self.config.skip_quest, "跳过 snbt")
                    .on_hover_text("勾选后将不再检查config/ftbquests，只检查kubejs下的本地化文件");
            });
            ui.end_row();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if self.is_processing {
                    ui.add_enabled(false, egui::Button::new("⏳ 处理中..."));
                    ui.spinner();
                    if ui.button("❌ 取消任务").clicked() {
                        self.cancel_processing();
                    }
                } else {
                    if ui.button("🚀 开始翻译").clicked() {
                        if self.config.api_key.is_empty() {
                            self.logs
                                .push(LogEntry::new(LogLevel::Error, "请先填写 API Key"));
                        } else {
                            self.logs.push(LogEntry::new(LogLevel::Info, "任务启动..."));
                            self.start_processing(false);
                        }
                    }
                    if ui.button("🔄 更新翻译").clicked() {
                        if self.config.api_key.is_empty() {
                            self.logs
                                .push(LogEntry::new(LogLevel::Error, "请先填写 API Key"));
                        } else {
                            self.logs
                                .push(LogEntry::new(LogLevel::Info, "更新任务启动..."));
                            self.start_processing(true);
                        }
                    }
                }
            });

            ui.separator();

            ui.push_id("log_area", |ui| {
                ui.style_mut().spacing.item_spacing.y = 0.0;
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (i, entry) in self.logs.iter().enumerate() {
                            let visuals = ui.visuals();
                            let (text_color, prefix) = match entry.level {
                                LogLevel::Info => (visuals.text_color(), "INFO"),
                                LogLevel::Success => (egui::Color32::from_rgb(0, 200, 0), "DONE"),
                                LogLevel::Warn => (visuals.warn_fg_color, "WARN"),
                                LogLevel::Error => (visuals.error_fg_color, "ERR "),
                            };

                            let bg_color = if i % 2 == 1 {
                                visuals.faint_bg_color
                            } else {
                                egui::Color32::TRANSPARENT
                            };

                            let full_text =
                                format!("{} [{}] {}", entry.time, prefix, entry.message);

                            let mut job = egui::text::LayoutJob::single_section(
                                full_text,
                                egui::TextFormat {
                                    font_id: egui::FontId::monospace(13.0),
                                    color: text_color,
                                    ..Default::default()
                                },
                            );
                            job.wrap.break_anywhere = true;

                            egui::Frame::new()
                                .fill(bg_color)
                                .inner_margin(2.0)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.label(job);
                                });
                        }
                    });
            });
        });

        if self.is_processing {
            ctx.request_repaint();
        }
    }
}
