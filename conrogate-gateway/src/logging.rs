//! 日志初始化：支持 JSON 格式 + env-filter + 文件输出。

use conrogate_contract::config::LogConfig;
use std::path::Path;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

/// 初始化 tracing 订阅者
pub fn init(log_config: &LogConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_config.level));

    // 控制台 layer
    let console_layer: Box<dyn Layer<_> + Send + Sync> = if log_config.console {
        if log_config.format.eq_ignore_ascii_case("json") {
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .boxed()
        } else {
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .boxed()
        }
    } else {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::empty)
            .boxed()
    };

    // 文件 layer
    if log_config.file_enabled && !log_config.file_path.is_empty() {
        if let Some(parent) = Path::new(&log_config.file_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let file_name = Path::new(&log_config.file_path)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("conrogate.log"));

        let file_appender = tracing_appender::rolling::daily(
            Path::new(&log_config.file_path)
                .parent()
                .unwrap_or(Path::new(".")),
            file_name,
        );

        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(file_appender)
            .boxed();

        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .init();
    }
}
