use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLang {
    Chinese,
    English,
}

impl Default for UiLang {
    fn default() -> Self {
        Self::Chinese
    }
}

pub struct TranslationBundle {
    pub app_title: &'static str,
    pub base_url_label: &'static str,
    pub swap_lang_tooltip: &'static str,
    pub api_key_label: &'static str,
    pub model_select_label: &'static str,
    pub check_refresh_btn: &'static str,
    pub input_path_label: &'static str,
    pub open_folder_btn: &'static str,
    pub open_file_btn: &'static str,
    pub output_path_label: &'static str,
    pub select_folder_btn: &'static str,
    
    // Prompt Editor
    pub edit_prompt_btn: &'static str,
    pub edit_prompt_tooltip: &'static str,
    pub prompt_editor_title: &'static str,
    pub prompt_editor_desc: &'static str,
    pub prompt_placeholder: &'static str,
    pub save_and_close: &'static str,
    pub restore_default: &'static str,

    // Settings
    pub batch_size_label: &'static str,
    pub batch_size_tooltip: &'static str,
    pub skip_existing_checkbox: &'static str,
    pub skip_snbt_checkbox: &'static str,
    pub skip_snbt_tooltip: &'static str,

    // Actions
    pub processing_btn: &'static str,
    pub cancel_task_btn: &'static str,
    pub start_translate_btn: &'static str,
    pub update_translate_btn: &'static str,

    // Footer
    pub github_link: &'static str,
    pub about_author: &'static str,

    // Messages (Logs)
    pub task_started_msg: &'static str,
    pub update_task_started_msg: &'static str,
    pub connecting_msg: &'static str,
    pub connected_msg: &'static str,       // Format: {} models
    pub connection_failed_msg: &'static str, // Format: {}
    pub all_tasks_completed_msg: &'static str,
    pub all_update_tasks_completed_msg: &'static str,
    pub task_canceled_msg: &'static str,
    pub api_key_missing_msg: &'static str,
}

pub const CN: TranslationBundle = TranslationBundle {
    app_title: "Minecraft Mod 汉化助手（支持任务、模组、资源包）",
    base_url_label: "BASE URL:",
    swap_lang_tooltip: "交换语言",
    api_key_label: "API KEY:",
    model_select_label: "选择模型:",
    check_refresh_btn: "🔄 检查 & 刷新",
    input_path_label: "输入路径:",
    open_folder_btn: "📂 打开文件夹",
    open_file_btn: "📄 打开文件",
    output_path_label: "输出目录:",
    select_folder_btn: "📂 选择文件夹",

    edit_prompt_btn: "📝 编辑提示词",
    edit_prompt_tooltip: "自定义发送给 AI 的系统提示词",
    prompt_editor_title: "📝 自定义系统提示词 (System Prompt)",
    prompt_editor_desc: "在此设置发送给 AI 的系统级指令，可用于控制翻译风格、保留特定术语等。",
    prompt_placeholder: "请输入 System Prompt...",
    save_and_close: "保存并关闭",
    restore_default: "恢复默认",

    batch_size_label: "批次大小:",
    batch_size_tooltip: "影响上下文的处理",
    skip_existing_checkbox: "跳过已翻译的文件",
    skip_snbt_checkbox: "跳过 snbt",
    skip_snbt_tooltip: "勾选后将不再检查config/ftbquests，只检查kubejs下的本地化文件",

    processing_btn: "⏳ 处理中...",
    cancel_task_btn: "❌ 取消任务",
    start_translate_btn: "🚀 开始翻译",
    update_translate_btn: "🔄 更新翻译",

    github_link: "GitHub 主页",
    about_author: "关于作者",

    task_started_msg: "任务启动...",
    update_task_started_msg: "更新任务启动...",
    connecting_msg: "正在连接 API 获取模型列表...",
    connected_msg: "✅ 连接成功！获取到 {} 个模型",
    connection_failed_msg: "❌ 连接失败: {}",
    all_tasks_completed_msg: "所有翻译任务已完成",
    all_update_tasks_completed_msg: "所有更新任务已完成",
    task_canceled_msg: "任务已被用户取消",
    api_key_missing_msg: "请先填写 API Key",
};

pub const EN: TranslationBundle = TranslationBundle {
    app_title: "Minecraft Mod Translator (Mods, Quests, Resources)",
    base_url_label: "BASE URL:",
    swap_lang_tooltip: "Swap Languages",
    api_key_label: "API KEY:",
    model_select_label: "Model:",
    check_refresh_btn: "🔄 Check & Refresh",
    input_path_label: "Input Path:",
    open_folder_btn: "📂 Open Folder",
    open_file_btn: "📄 Open File",
    output_path_label: "Output Path:",
    select_folder_btn: "📂 Select Folder",

    edit_prompt_btn: "📝 Edit Prompt",
    edit_prompt_tooltip: "Customize System Prompt sent to AI",
    prompt_editor_title: "📝 Custom System Prompt",
    prompt_editor_desc: "Set system instructions for AI to control translation style, preserve terms, etc.",
    prompt_placeholder: "Enter System Prompt...",
    save_and_close: "Save & Close",
    restore_default: "Restore Default",

    batch_size_label: "Batch Size:",
    batch_size_tooltip: "Affects context processing",
    skip_existing_checkbox: "Skip Translated Files",
    skip_snbt_checkbox: "Skip SNBT",
    skip_snbt_tooltip: "If checked, skips config/ftbquests and only checks localization files in kubejs",

    processing_btn: "⏳ Processing...",
    cancel_task_btn: "❌ Cancel Task",
    start_translate_btn: "🚀 Start Translate",
    update_translate_btn: "🔄 Update Translate",

    github_link: "GitHub Page",
    about_author: "About Author",

    task_started_msg: "Task Started...",
    update_task_started_msg: "Update Task Started...",
    connecting_msg: "Connecting to API...",
    connected_msg: "✅ Connected! Fetched {} models",
    connection_failed_msg: "❌ Connection Failed: {}",
    all_tasks_completed_msg: "All translation tasks completed",
    all_update_tasks_completed_msg: "All update tasks completed",
    task_canceled_msg: "Task canceled by user",
    api_key_missing_msg: "Please enter API Key first",
};

impl UiLang {
    pub fn bundle(&self) -> &'static TranslationBundle {
        match self {
            UiLang::Chinese => &CN,
            UiLang::English => &EN,
        }
    }
}