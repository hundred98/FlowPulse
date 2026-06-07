//! 温度控制测试程序
//!
//! 测试配置下发和温度读取功能
//!
//! 使用方法：
//! 1. 启动 emb-core-server：
//!    cargo run --release
//! 2. 运行本测试程序：
//!    cargo run --bin test_temperature
//!
//! 预期输出：
//! - 服务端打印配置帧下发日志
//! - 服务端打印 ConfigComplete 发送日志
//! - 服务端打印温度数据上报日志

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use std::sync::atomic::{AtomicU32, Ordering};

use emb_public::{CoreSocketClient, CoreClientConfig, ConfigFrameBuilder, config_adapter};

#[tokio::main]
async fn main() {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    log::info!("========================================");
    log::info!("温度控制测试程序");
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
    
    // 加载配置并发送到服务端和下位机
    // 注意：串口连接现在由 configure_device 函数内部处理，配置来自 printer.json
    let config_dir = "config";
    log::info!("📁 加载配置文件: {}", config_dir);
    
    match config_adapter::configure_device(&client, config_dir).await {
        Ok(configs) => {
            log::info!("✅ 配置发送成功");
            log::info!("  - 打印机型号: {}", configs.printer.printer_model);
        }
        Err(e) => {
            log::error!("❌ 配置发送失败: {}", e);
            return;
        }
    }
    
    // 监控温度数据（使用订阅方式）
    log::info!("========================================");
    log::info!("📊 开始监控温度数据（订阅方式，30秒）");
    log::info!("========================================");
    
    // 计数器：记录收到的状态数据数量
    let status_count = Arc::new(AtomicU32::new(0));
    let status_count_clone = status_count.clone();
    
    // 设置状态回调
    client.set_status_report_callback(move |frame_type, payload| {
        // StatusResponse 帧格式：
        // [credits:1][pos_x:4][pos_y:4][pos_z:4][pos_e:4][temp_bed_cur:2][temp_bed_tgt:2][temp_nozzle_cur:2][temp_nozzle_tgt:2][status:1]
        // 温度数据从 payload[17] 开始
        if frame_type == 0x04 && payload.len() >= 25 {  // 0x04 = StatusResponse
            let temp_bed_cur = i16::from_be_bytes([payload[17], payload[18]]);
            let temp_bed_tgt = i16::from_be_bytes([payload[19], payload[20]]);
            let temp_nozzle_cur = i16::from_be_bytes([payload[21], payload[22]]);
            let temp_nozzle_tgt = i16::from_be_bytes([payload[23], payload[24]]);
            
            log::info!("🌡️  温度数据: 热床={}/{}°C, 热端={}/{}°C", 
                temp_bed_cur as f32 / 10.0, 
                temp_bed_tgt as f32 / 10.0,
                temp_nozzle_cur as f32 / 10.0, 
                temp_nozzle_tgt as f32 / 10.0);
            
            // 增加计数器
            status_count_clone.fetch_add(1, Ordering::SeqCst);
        }
    }).await;
    
    // 订阅状态上报
    match client.subscribe_status(true).await {
        Ok(()) => log::info!("✅ 已订阅状态上报"),
        Err(e) => {
            log::error!("❌ 订阅失败: {}", e);
            return;
        }
    }
    
    // 等待30秒
    for i in 1..=30 {
        sleep(Duration::from_secs(1)).await;
        let count = status_count.load(Ordering::SeqCst);
        log::info!("⏱️  监控中... {}/30 秒, 已收到 {} 条状态数据", i, count);
    }
    
    // 取消订阅
    match client.subscribe_status(false).await {
        Ok(()) => log::info!("✅ 已取消订阅"),
        Err(e) => log::warn!("❌ 取消订阅失败: {}", e),
    }
    
    // 清除回调
    client.clear_status_report_callback().await;
    
    // 测试设置温度
    log::info!("========================================");
    log::info!("🔥 测试设置温度");
    log::info!("========================================");
    
    // 设置热端温度为 50°C
    log::info!("📤 设置热端温度: 50°C");
    let set_temp_frame = ConfigFrameBuilder::build_set_temp_frame(1, 50.0);
    match client.serial_send_raw(&set_temp_frame).await {
        Ok(()) => log::info!("✅ SetTemp 帧已发送"),
        Err(e) => log::warn!("❌ SetTemp 帧发送失败: {}", e),
    }
    
    // 监控温度变化（10秒）
    log::info!("📊 监控温度变化（10秒）...");
    for i in 1..=10 {
        sleep(Duration::from_secs(1)).await;
        log::info!("⏱️  监控中... {}/10 秒", i);
    }
    
    // 关闭加热器
    log::info!("📤 关闭热端加热器");
    let set_temp_frame = ConfigFrameBuilder::build_set_temp_frame(1, 0.0);
    match client.serial_send_raw(&set_temp_frame).await {
        Ok(()) => log::info!("✅ SetTemp 帧已发送"),
        Err(e) => log::warn!("❌ SetTemp 帧发送失败: {}", e),
    }
    
    // 断开连接
    log::info!("========================================");
    log::info!("🏁 测试完成");
    log::info!("========================================");
    
    
    client.disconnect().await;
    log::info!("✅ 已断开服务器连接");
}
