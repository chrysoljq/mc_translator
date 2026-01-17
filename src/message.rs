use crossbeam_channel::Sender;
use std::sync::OnceLock;
use crate::logging::{LogEntry, LogLevel};

pub static GLOBAL_SENDER: OnceLock<Sender<AppMsg>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum AppMsg {
    Log(LogEntry),
    ModelsFetched(Vec<String>),
}

pub fn send_log(level: LogLevel, msg: String) {
    if let Some(sender) = GLOBAL_SENDER.get() {
        let _ = sender.send(AppMsg::Log(LogEntry::new(level, msg)));
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::message::send_log($crate::logging::LogLevel::Info, format!($($arg)*))
    }
}

#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        $crate::message::send_log($crate::logging::LogLevel::Error, format!($($arg)*))
    }
}

#[macro_export]
macro_rules! log_success {
    ($($arg:tt)*) => {
        $crate::message::send_log($crate::logging::LogLevel::Success, format!($($arg)*))
    }
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::message::send_log($crate::logging::LogLevel::Warn, format!($($arg)*))
    }
}