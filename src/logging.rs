use chrono::Local;

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Success, // 用于显示 "任务完成" 或 "保存成功"
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub time: String,
    pub level: LogLevel,
    pub message: String,
}

impl LogEntry {
    pub fn new(level: LogLevel, msg: impl Into<String>) -> Self {
        Self {
            time: Local::now().format("%H:%M:%S").to_string(), // 自动生成时间戳
            level,
            message: msg.into(),
        }
    }
}