//! 温度控制测试程序
//!
//! 测试 TemperatureManager 的功能
//!
//! 使用方法：
//! 1. 启动 emb-core-server：
//!    cargo run --release
//! 2. 运行本测试程序：
//!    cargo run --bin test_temperature
//!
//! 预期输出：
//! - 服务端打印配置帧下发日志
//! - 温度管理器初始化成功
//! - 温度预设加载成功
//! - 温度设置和查询成功

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use emb_public::{
    CoreSocketClient, CoreClientConfig, ConfigManager,
    temperature::{TemperatureManager, TemperatureManagerConfig},
    common::SyncEventPublisher,
};

#[tokio::main]
async fn main() {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("========================================");
    log::info!("温度控制测试程序（使用 TemperatureManager）");
    log::info!("========================================");
    log::info!("💡 提示：服务端应使用 --features serial_debug 编译");
    log::info!("");

    // 连接到核心服务器
    let server_addr = "127.0.0.1:9527";
    log::info!("📡 连接到核心服务器: {}", server_addr);

    let config = CoreClientConfig {
        server_addr: server_addr.to_string(),
        ..CoreClientConfig::default()
    };
    let client = Arc::new(CoreSocketClient::new(config));

    match client.connect().await {
        Ok(()) => log::info!("✅ 已连接到核心服务器"),
        Err(e) => {
            log::error!("❌ 连接失败: {}", e);
            return;
        }
    }

    // 加载配置
    let config_dir = "config";
    log::info!("📁 加载配置文件: {}", config_dir);

    if let Err(e) = ConfigManager::instance().load(config_dir) {
        log::error!("❌ 配置加载失败: {}", e);
        return;
    }

    // 获取配置信息并连接串口
    match ConfigManager::instance().get_config() {
        Ok(config) => {
            log::info!("✅ 配置加载成功");
            log::info!("  - 打印机型号: {}", config.printer_model);

            // 连接串口
            if !config.communication.serial.port.is_empty() {
                let serial = &config.communication.serial;
                log::info!("🔌 连接串口: {} @ {}", serial.port, serial.baud_rate);
                match client.serial_connect(&serial.port, serial.baud_rate).await {
                    Ok(()) => log::info!("✅ 串口连接成功"),
                    Err(e) => {
                        log::error!("❌ 串口连接失败: {}", e);
                        log::info!("💡 请确认下位机已连接到 {}", serial.port);
                        return;
                    }
                }
            }
        }
        Err(e) => {
            log::error!("❌ 获取配置失败: {}", e);
            return;
        }
    }

    // 发送配置到服务端和下位机
    match ConfigManager::instance().reload(&client).await {
        Ok(()) => {
            log::info!("✅ 配置发送成功");
        }
        Err(e) => {
            log::error!("❌ 配置发送失败: {}", e);
            return;
        }
    }

    // 创建事件发布器（简化版，仅用于日志）
    let event_publisher = Arc::new(SyncEventPublisher::new());

    // 创建温度管理器
    log::info!("========================================");
    log::info!("🌡️  初始化温度管理器");
    log::info!("========================================");

    let temperature_manager = Arc::new(TemperatureManager::new(
        client.clone(),
        event_publisher.clone(),
        TemperatureManagerConfig::default(),
        None,
    ));

    // 初始化温度管理器
    match temperature_manager.initialize().await {
        Ok(()) => log::info!("✅ 温度管理器初始化成功"),
        Err(e) => {
            log::error!("❌ 温度管理器初始化失败: {}", e);
            return;
        }
    }

    // 订阅温度更新
    match temperature_manager.subscribe_temperature_updates().await {
        Ok(()) => log::info!("✅ 已订阅温度更新"),
        Err(e) => {
            log::error!("❌ 订阅温度更新失败: {}", e);
            return;
        }
    }

    // 启动安全检查（在后台运行）
    let temp_mgr_clone = temperature_manager.clone();
    tokio::spawn(async move {
        temp_mgr_clone.start_safety_check_loop().await;
    });
    log::info!("✅ 安全检查已启动（后台运行）");

    // 查看温度预设
    log::info!("========================================");
    log::info!("📋 查看温度预设");
    log::info!("========================================");

    let presets = temperature_manager.get_presets().await;
    log::info!("已加载 {} 个温度预设:", presets.len());

    for preset in &presets {
        log::info!("  - {}: 热端={}°C, 热床={}°C",
            preset.name, preset.hotend_temp, preset.bed_temp);
    }

    // 查询初始温度状态
    log::info!("========================================");
    log::info!("📊 查询初始温度状态");
    log::info!("========================================");

    let heaters = temperature_manager.get_all_heaters().await;
    for (name, state) in &heaters {
        log::info!("  {}: {}/{}°C, 加热={}",
            name, state.current_temp, state.target_temp, state.is_heating);
    }

    // 测试设置温度
    log::info!("========================================");
    log::info!("🔥 测试设置温度");
    log::info!("========================================");

    // 设置热端温度为 50°C
    log::info!("📤 设置热端温度: 50°C");
    match temperature_manager.set_target("hotend", 50.0).await {
        Ok(()) => log::info!("✅ 热端温度设置成功"),
        Err(e) => log::warn!("❌ 热端温度设置失败: {}", e),
    }

    // 设置热床温度为 40°C
    log::info!("📤 设置热床温度: 40°C");
    match temperature_manager.set_target("bed", 40.0).await {
        Ok(()) => log::info!("✅ 热床温度设置成功"),
        Err(e) => log::warn!("❌ 热床温度设置失败: {}", e),
    }

    // 查询温度状态
    sleep(Duration::from_secs(1)).await;
    let heaters = temperature_manager.get_all_heaters().await;
    for (name, state) in &heaters {
        log::info!("  {}: {}/{}°C, 加热={}",
            name, state.current_temp, state.target_temp, state.is_heating);
    }

    // 测试应用预设
    log::info!("========================================");
    log::info!("🎯 测试应用温度预设");
    log::info!("========================================");

    if !presets.is_empty() {
        let preset_name = &presets[0].name;
        log::info!("📤 应用预设: {}", preset_name);

        match temperature_manager.apply_preset(preset_name).await {
            Ok(()) => {
                log::info!("✅ 预设 '{}' 应用成功", preset_name);

                // 查询温度状态
                sleep(Duration::from_secs(1)).await;
                let heaters = temperature_manager.get_all_heaters().await;
                for (name, state) in &heaters {
                    log::info!("  {}: {}/{}°C, 加热={}",
                        name, state.current_temp, state.target_temp, state.is_heating);
                }
            }
            Err(e) => log::warn!("❌ 预设应用失败: {}", e),
        }
    }

    // 监控温度变化（30秒）
    log::info!("========================================");
    log::info!("📊 监控温度变化（30秒）");
    log::info!("========================================");

    for i in 1..=30 {
        sleep(Duration::from_secs(1)).await;

        // 查询温度状态
        let heaters = temperature_manager.get_all_heaters().await;

        // 打印温度信息
        let hotend = heaters.get("hotend").map(|h| format!("{}/{}°C", h.current_temp, h.target_temp)).unwrap_or_else(|| "N/A".to_string());
        let bed = heaters.get("bed").map(|h| format!("{}/{}°C", h.current_temp, h.target_temp)).unwrap_or_else(|| "N/A".to_string());

        log::info!("⏱️  {}/30 秒 - 热端: {}, 热床: {}", i, hotend, bed);
    }

    // 测试安全检查
    log::info!("========================================");
    log::info!("⚠️  测试安全检查");
    log::info!("========================================");

    let safety_results = temperature_manager.check_safety().await;
    if safety_results.is_empty() {
        log::info!("✅ 安全检查通过，无异常");
    } else {
        log::warn!("⚠️  发现 {} 个安全问题:", safety_results.len());
        for result in safety_results {
            log::warn!("  - {}: {} (级别: {:?})",
                result.heater, result.message, result.level);
        }
    }

    // 关闭所有加热器
    log::info!("========================================");
    log::info!("🛑 关闭所有加热器");
    log::info!("========================================");

    match temperature_manager.turn_off_all().await {
        Ok(()) => log::info!("✅ 所有加热器已关闭"),
        Err(e) => log::warn!("❌ 关闭加热器失败: {}", e),
    }

    // 查询最终温度状态
    sleep(Duration::from_secs(1)).await;
    let heaters = temperature_manager.get_all_heaters().await;
    for (name, state) in &heaters {
        log::info!("  {}: {}/{}°C, 加热={}",
            name, state.current_temp, state.target_temp, state.is_heating);
    }

    // 断开连接
    log::info!("========================================");
    log::info!("🏁 测试完成");
    log::info!("========================================");

    client.disconnect().await;
    log::info!("✅ 已断开服务器连接");
}
