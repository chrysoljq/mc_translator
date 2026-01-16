use crate::logging::LogEntry;

pub enum AppMsg {
    Log(LogEntry),           // 普通日志
    ModelsFetched(Vec<String>), // 成功抓取到模型列表
}