use eframe::egui;
use std::thread;
use crate::config::AppConfig;
use crate::logic::processor;
use crate::utils::setup_custom_fonts;
use crate::logging::{LogEntry, LogLevel};
use crate::message::AppMsg;
use crossbeam_channel::{Receiver, Sender};
use crate::logic::openai::OpenAIClient;

pub struct MyApp {
    config: AppConfig,
    is_processing: bool,
    available_models: Vec<String>,
    logs: Vec<LogEntry>, // <-- 改用 Vec 存储结构化日志
    msg_receiver: Receiver<AppMsg>,
    msg_sender: Sender<AppMsg>,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(1.1); // 字体缩放
        let (sender, receiver) = crossbeam_channel::unbounded();

        Self {
            config: AppConfig::load(), // 加载保存的配置
            logs: Vec::new(),
            is_processing: false,
            available_models: vec![
                "gpt-3.5-turbo".to_string(),
                "gpt-4o".to_string(),
                ],
            msg_receiver: receiver,
            msg_sender: sender,
        }
    }

    fn check_connection_and_fetch_models(&self) {
        let api_key = self.config.api_key.clone();
        let base_url = self.config.base_url.clone();
        let sender = self.msg_sender.clone();
        
        let _ = sender.send(AppMsg::Log(LogEntry::new(LogLevel::Info, "正在连接 API 获取模型列表...")));

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let client = OpenAIClient::new(api_key, base_url, "default".to_string());
                match client.fetch_models().await {
                    Ok(models) => {
                        let _ = sender.send(AppMsg::Log(LogEntry::new(LogLevel::Success, format!("✅ 连接成功！获取到 {} 个模型", models.len()))));
                        let _ = sender.send(AppMsg::ModelsFetched(models));
                    },
                    Err(e) => {
                         let _ = sender.send(AppMsg::Log(LogEntry::new(LogLevel::Error, format!("❌ 连接失败: {}", e))));
                    }
                }
            });
        });
    }

    fn start_processing(&mut self) {
        if self.is_processing { return; }
        
        self.is_processing = true;
        // 保存当前配置
        self.config.save();
        
        let input = self.config.input_path.clone();
        let output = self.config.output_path.clone();
        let api_key = self.config.api_key.clone();
        let base_url = self.config.base_url.clone();
        let sender = self.msg_sender.clone();
        let model = self.config.model.clone();          // 传递
        let batch_size = self.config.batch_size;        // 传递

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                processor::run_processing_task(input, output, api_key, base_url, model, batch_size, sender).await;
            });
        });
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理日志
        while let Ok(msg) = self.msg_receiver.try_recv() {
            match msg {
                AppMsg::Log(entry) => {
                    if self.logs.len() > 1000 { self.logs.remove(0); }
                    if entry.message.contains("完成") || entry.message.contains("任务终止") {
                        self.is_processing = false;
                    }
                    self.logs.push(entry);
                },
                AppMsg::ModelsFetched(models) => {
                    self.available_models = models;
                    // 如果当前配置的模型不在列表里，默认选中第一个（可选逻辑）
                    if !self.available_models.contains(&self.config.model) && !self.available_models.is_empty() {
                         self.config.model = self.available_models[0].clone();
                    }
                }
            }
        }
        egui::TopBottomPanel::bottom("footer_panel").show(ctx, |ui| {
            ui.add_space(2.0); // 稍微加点顶部留白
            ui.horizontal(|ui| {
                // 左侧文本
                ui.label(egui::RichText::new("v1.0.0").weak().size(10.0));
                
                // 右侧链接 (使用 right_to_left 让链接靠右对齐)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 注意：布局是“从右往左”添加的，所以顺序要反过来写
                    
                    // 链接 2
                    ui.hyperlink_to(
                        egui::RichText::new("去 GitHub 给个 Star ⭐").size(11.0), 
                        "https://github.com/chrysoljq/mc_translator"
                    );
                    
                    ui.label(egui::RichText::new("|").weak().size(11.0));
                    
                    // 链接 1
                    ui.hyperlink_to(
                        egui::RichText::new("关于作者").size(11.0), 
                        "https://github.com/chrysoljq"
                    );
                });
            });
            ui.add_space(2.0); // 底部留白
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Minecraft Mod 汉化助手");
            ui.separator();

            egui::Grid::new("settings_grid").num_columns(2).spacing([10.0, 8.0]).striped(true).show(ui, |ui| {
                ui.label("BASE URL:");
                ui.text_edit_singleline(&mut self.config.base_url);
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
                                ui.selectable_value(&mut self.config.model, model.clone(), model);
                            }
                        });

                    if ui.button("🔄 检查 & 刷新").clicked() {
                        if self.config.api_key.is_empty() {
                            self.logs.push(LogEntry::new(LogLevel::Error, "请先填写 API Key"));
                        } else {
                            self.check_connection_and_fetch_models();
                        }
                    }
                    ui.label("Batch Size:");
                    ui.add(egui::DragValue::new(&mut self.config.batch_size).range(1..=500).speed(1));
                });
                ui.end_row();

                ui.label("输入路径:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.config.input_path);
                    if ui.button("📂 打开文件夹").on_hover_text("选择包含 Jar 的目录").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.config.input_path = path.display().to_string();
                    }
                }

                if ui.button("📄 打开文件").on_hover_text("选择单个汉化文件").clicked() {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("Minecraft Mod", &["jar", "json", "lang"]) // 顺便加个过滤器，只显示 jar
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
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.config.output_path = path.display().to_string();
                        }
                    }
                });
                ui.end_row();
            });

            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if self.is_processing {
                    ui.add_enabled(false, egui::Button::new("⏳ 处理中..."));
                    ui.spinner();
                } else {
                    if ui.button("🚀 开始翻译").clicked() {
                        if self.config.api_key.is_empty() {
                            self.logs.push(LogEntry::new(LogLevel::Error, "请先填写 API Key"));
                        } else {
                            self.logs.push(LogEntry::new(LogLevel::Info, "任务启动..."));
                            self.start_processing();
                        }
                    }
                }
            });

            ui.separator();

            ui.push_id("log_area", |ui| {
                let mut style = (*ctx.style()).clone();
                style.spacing.item_spacing = egui::vec2(1.0, 0.0);
                ui.set_style(style);

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("log_grid")
                            .num_columns(3) 
                            .spacing([5.0, 0.0]) // 列与列之间留出空隙，防止粘连
                            .striped(true)       // 斑马纹背景
                            .show(ui, |ui| {
                                for entry in &self.logs {
                                    // 根据等级定义颜色
                                    let (color, prefix) = match entry.level {
                                        LogLevel::Info => (egui::Color32::from_gray(200), "INFO"),
                                        LogLevel::Success => (egui::Color32::LIGHT_GREEN, "DONE"),
                                        LogLevel::Warn => (egui::Color32::YELLOW, "WARN"),
                                        LogLevel::Error => (egui::Color32::LIGHT_RED, "ERR "),
                                    };

                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                        ui.label(egui::RichText::new(&entry.time)
                                            .color(egui::Color32::GRAY)
                                            .size(13.0)
                                            .monospace());
                                    });

                                    // --- 第二列：标签 [INFO] ---
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        ui.label(egui::RichText::new(format!("[{}]", prefix))
                                            .color(color)
                                            .size(13.0)
                                            .strong() // 加粗
                                            .monospace());
                                    });

                                    // --- 第三列：具体内容 ---
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        let text = egui::RichText::new(&entry.message)
                                            .color(color)
                                            .size(13.0);
                                        
                                        let text = if matches!(entry.level, LogLevel::Error) {
                                            text.monospace()
                                        } else {
                                            text
                                        };

                                        ui.add(egui::Label::new(text).wrap())
                                    });

                                    ui.end_row(); // 结束这一行
                                }
                            });
                    });
            });
        });

        if self.is_processing {
            ctx.request_repaint();
        }
    }
}