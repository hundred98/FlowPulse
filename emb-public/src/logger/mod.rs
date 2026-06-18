//! Logger module
//!
//! Provides tracing initialization based on LogConfig from logging.json.
//! Supports console output (compact format, no ANSI colors) and optional
//! file output with rotation.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use anyhow::Context;

use crate::config::log_config::LogConfig;

/// Initialize tracing subscriber based on the given LogConfig.
pub fn init_tracing(config: &LogConfig) -> anyhow::Result<()> {
    let mut layers = Vec::new();

    // Console layer
    if config.console.enable {
        let filter = build_env_filter(&config.console.level, &config.modules)?;
        let console_layer = tracing_subscriber::fmt::layer()
            .compact()
            .with_ansi(false)
            .with_filter(filter);
        layers.push(console_layer.boxed());
    }

    // File layer (optional)
    if config.file.enable {
        let filter = build_env_filter(&config.file.level, &config.modules)?;
        let log_path = std::path::Path::new(&config.file.path);
        let log_dir = log_path.parent().unwrap_or(std::path::Path::new("logs"));
        let log_prefix = log_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("flowpulse");

        // Resolve log dir relative to current working directory and create
        let abs_log_dir = std::env::current_dir()
            .unwrap_or_default()
            .join(log_dir);
        std::fs::create_dir_all(&abs_log_dir)
            .with_context(|| format!("Failed to create log dir {:?}", abs_log_dir))?;

        eprintln!("[logger] File logging enabled: {:?}/{}", abs_log_dir, log_prefix);

        let file_appender = tracing_appender::rolling::daily(&abs_log_dir, log_prefix);
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(file_appender)
            .with_filter(filter);
        layers.push(file_layer.boxed());
    }

    // Build subscriber from layers
    let subscriber = tracing_subscriber::registry().with(layers);
    subscriber.init();

    Ok(())
}

/// Build EnvFilter from a base level and module-level overrides.
fn build_env_filter(
    base_level: &str,
    modules: &std::collections::HashMap<String, String>,
) -> anyhow::Result<EnvFilter> {
    let default_level = if base_level.is_empty() { "info" } else { base_level };

    let mut filter = EnvFilter::builder()
        .with_default_directive(default_level.parse()?)
        .parse("")?;

    for (module, level) in modules {
        let directive = format!("{}={}", module, level);
        filter = filter.add_directive(directive.parse()?);
    }

    Ok(filter)
}