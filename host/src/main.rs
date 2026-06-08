//! Host Application V2
//!
//! Socket-based host that connects to emb-core-server via TCP.
//! Integrates state management, message queue, and multi-channel access.
//! Loads config files, prints G-code files.
//! All motion planning + segment dispatch is handled by emb-core-server.

mod printer_host_v2;
mod app;

use printer_host_v2::{PrinterHostV2, HostV2Config};
use app::AppState;
use emb_public::ConfigManager;
use emb_public::ConfigFrameBuilder;
use emb_public::gcode::{GCodeParser, ParsedCommand, CommandKind, MotionCommand};
use emb_api::MCommand;
use std::env;

/// Execute a parsed G-code command.
/// Planning + mm→steps + serial dispatch all happen in emb-core-server.
async fn execute_parsed_command(
    host: &PrinterHostV2,
    parsed: &ParsedCommand,
    total_segments: &mut usize,
) -> Result<(), String> {
    match &parsed.kind {
        CommandKind::Motion(motion) => {
            execute_motion_command(host, motion, parsed.line_number, total_segments).await
        }
        CommandKind::Machine(m_command) => {
            execute_m_command(host, m_command, parsed.line_number).await
        }
        CommandKind::Empty => Ok(()),
        CommandKind::Unsupported { raw } => {
            log::debug!("  [L{:03}] Skipping unsupported: {}", parsed.line_number, raw);
            Ok(())
        }
    }
}

/// Execute a motion command (G0/G1/G2/G3/G28/G92/etc.)
async fn execute_motion_command(
    host: &PrinterHostV2,
    motion: &MotionCommand,
    line_num: u32,
    total_segments: &mut usize,
) -> Result<(), String> {
    match motion {
        MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
            let cmd = if *is_rapid { "G0" } else { "G1" };
            let dispatched = host.client().motion_dispatch(cmd, *x, *y, *z, *e, *f).await?;
            *total_segments += dispatched;
            if dispatched == 0 {
                log::debug!("  [L{:03}] {} => 0 segments (skipped, distance too small)", line_num, cmd);
            }
            Ok(())
        }
        MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
            let cmd = if *is_cw { "G2" } else { "G3" };
            let dispatched = host.client().motion_dispatch_arc(
                cmd, *x, *y, *z, *e, *f,
                Some(emb_api::ArcParamsApi {
                    i: *i,
                    j: *j,
                    direction: if *is_cw { 0 } else { 1 },
                }),
            ).await?;
            *total_segments += dispatched;
            Ok(())
        }
        MotionCommand::Home { x, y, z } => {
            // G28 - 如果指定了轴，只回零指定轴；否则回零所有轴
            if *x || *y || *z {
                // 暂不支持单轴回零，回零所有轴
                log::warn!("  [L{:03}] G28 partial home not supported, homing all axes", line_num);
            }
            let dispatched = host.client().motion_dispatch("G28", None, None, None, None, None).await?;
            *total_segments += dispatched;
            Ok(())
        }
        MotionCommand::SetPosition { x, y, z, e } => {
            host.set_position(*x, *y, *z, *e).await?;
            Ok(())
        }
        MotionCommand::AbsolutePositioning => {
            log::debug!("  [L{:03}] G90 Absolute positioning (handled by slicer)", line_num);
            Ok(())
        }
        MotionCommand::RelativePositioning => {
            log::debug!("  [L{:03}] G91 Relative positioning (handled by slicer)", line_num);
            Ok(())
        }
        MotionCommand::Inches => {
            log::warn!("  [L{:03}] G20 Inches mode not supported, using mm", line_num);
            Ok(())
        }
        MotionCommand::Millimeters => {
            log::debug!("  [L{:03}] G21 Millimeters mode", line_num);
            Ok(())
        }
    }
}

/// Execute an M command (machine control)
async fn execute_m_command(
    host: &PrinterHostV2,
    m_command: &MCommand,
    line_num: u32,
) -> Result<(), String> {
    match m_command {
        // 温度控制
        MCommand::SetHotendTemp { tool, temp } => {
            log::info!("  [L{:03}] M104: Set hotend {} temp to {:.1}°C", line_num, tool, temp);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::WaitHotendTemp { tool, temp } => {
            log::info!("  [L{:03}] M109: Wait for hotend {} temp to reach {:.1}°C", line_num, tool, temp);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::SetBedTemp { temp } => {
            log::info!("  [L{:03}] M140: Set bed temp to {:.1}°C", line_num, temp);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::WaitBedTemp { temp } => {
            log::info!("  [L{:03}] M190: Wait for bed temp to reach {:.1}°C", line_num, temp);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }

        // 风扇控制
        MCommand::SetFanSpeed { index, speed } => {
            log::info!("  [L{:03}] M106: Set fan {} speed to {}", line_num, index, speed);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::FanOff { index } => {
            log::info!("  [L{:03}] M107: Turn off fan {}", line_num, index);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }

        // 挤出机模式
        MCommand::ExtruderAbsoluteMode => {
            log::info!("  [L{:03}] M82: Extruder absolute mode", line_num);
            // 挤出机模式由客户端管理，服务端只记录日志
            Ok(())
        }
        MCommand::ExtruderRelativeMode => {
            log::info!("  [L{:03}] M83: Extruder relative mode", line_num);
            // 挤出机模式由客户端管理，服务端只记录日志
            Ok(())
        }

        // 运动参数
        MCommand::SetAcceleration { x, y, z, e } => {
            log::info!("  [L{:03}] M201: Set acceleration X={:?} Y={:?} Z={:?} E={:?}", line_num, x, y, z, e);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::SetMaxVelocity { x, y, z, e } => {
            log::info!("  [L{:03}] M203: Set max velocity X={:?} Y={:?} Z={:?} E={:?}", line_num, x, y, z, e);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::SetAccelParams { travel, print, retract } => {
            log::info!("  [L{:03}] M204: Set accel params T={:?} P={:?} R={:?}", line_num, travel, print, retract);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
        MCommand::SetStepsPerMm { x, y, z, e } => {
            log::info!("  [L{:03}] M92: Set steps/mm X={:?} Y={:?} Z={:?} E={:?}", line_num, x, y, z, e);
            host.client().motion_execute_m_command(m_command.clone()).await?;
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logger with default info level
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    
    // Parse flags first
    let query_stats = args.iter().any(|a| a == "--stats" || a == "-s");
    let enable_services = args.iter().any(|a| a == "--services");
    
    // Parse positional arguments (skip flags)
    let positional_args: Vec<&str> = args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();
    
    let gcode_path = positional_args.get(1).unwrap_or(&"gcodes/test-200.gcode");
    let server_addr = positional_args.get(2).unwrap_or(&"127.0.0.1:9527");
    let config_dir = positional_args.get(3).unwrap_or(&"config");

    log::info!("3D Printer Host V2 starting...");
    log::info!("G-code: {}", gcode_path);
    log::info!("Server: {}", server_addr);
    log::info!("Config: {}", config_dir);
    log::info!("Query stats: {}", query_stats);
    log::info!("Enable services: {}", enable_services);

    // Load config files (hardware.json + motion.json + printer.json)
    ConfigManager::instance().load(config_dir)
        .unwrap_or_else(|e| {
            log::error!("Failed to load configs: {}", e);
            std::process::exit(1);
        });

    let printer_config = ConfigManager::instance().get_config()
        .unwrap_or_else(|e| {
            log::error!("Failed to get config: {}", e);
            std::process::exit(1);
        });

    log::info!(
        "Loaded {} motors, printer model: {}",
        printer_config.motor.len(),
        printer_config.printer_model,
    );

    // Merge configs into MotionConfig JSON
    let motion_json = ConfigManager::instance().get_motion_config_json().unwrap_or_else(|e| {
        log::error!("Failed to build motion config: {}", e);
        std::process::exit(1);
    });

    let host_config = HostV2Config {
        server_addr: server_addr.to_string(),
        ..Default::default()
    };

    let host = PrinterHostV2::new(host_config);

    // Step 1: Connect TCP socket to emb-core-server
    match host.connect_socket().await {
        Ok(()) => log::info!("Connected to emb-core-server"),
        Err(e) => {
            log::error!("TCP connection failed: {}", e);
            std::process::exit(1);
        }
    }

    // Create application state with state management and multi-channel services
    let app_state = AppState::new(host.client());
    
    // Initialize application state
    if enable_services {
        app_state.initialize().await?;
        log::info!("Application state initialized");
        
        // Start background services
        app_state.start_services().await?;
        log::info!("Background services started");
        log::info!("WebSocket server: http://127.0.0.1:8080");
        log::info!("UnixSocket server: /tmp/flowpulse.sock");
    }

    // Step 2: Connect serial port to STM32 (through server proxy)
    let (serial_port, serial_baud) = {
        let serial = &printer_config.communication.serial;
        (serial.port.clone(), serial.baud_rate)
    };

    log::info!("Connecting serial {} @ {} baud...", serial_port, serial_baud);
    match host.client().serial_connect(&serial_port, serial_baud).await {
        Ok(()) => log::info!("Serial connected to {}", serial_port),
        Err(e) => {
            log::error!("Serial connect failed: {}", e);
            log::error!("Continuing in plan-only mode (no motor movement)");
        }
    }

    // Step 3: Send merged MotionConfig to server
    match host.client().config_update_motion(&motion_json).await {
        Ok(()) => log::info!("Motion config sent to server"),
        Err(e) => log::warn!("Send motion config failed (using defaults): {}", e),
    }

    // Step 3.1: Send FanConfig to server
    match ConfigManager::instance().get_fan_config() {
        Ok(fan_config) => {
            match host.client().config_update_fan(&fan_config).await {
                Ok(()) => log::info!("Fan config sent to server"),
                Err(e) => log::warn!("Send fan config failed: {}", e),
            }
        }
        Err(e) => log::warn!("Get fan config failed: {}", e),
    }

    // Step 3.5: Send config frames to STM32 device (motor pins, etc.)
    let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
    log::info!("Sending {} config frames to device...", config_frames.len());

    for frame_bytes in &config_frames {
        match host.client().serial_send_raw(frame_bytes).await {
            Ok(()) => log::debug!("Config frame sent: {} bytes", frame_bytes.len()),
            Err(e) => log::warn!("Failed to send config frame: {}", e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    log::info!("All config frames sent, waiting 300ms before ConfigComplete...");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Step 4: Initialize STM32 device
    match host.client().serial_config_complete().await {
        Ok(()) => log::info!("ConfigComplete sent"),
        Err(e) => log::warn!("ConfigComplete failed: {}", e),
    }

    match host.client().serial_init_seq().await {
        Ok(()) => log::info!("Device seq initialized"),
        Err(e) => log::warn!("Init seq failed: {}", e),
    }

    // Get initial position
    match host.get_position().await {
        Ok((x, y, z, e)) => log::info!("Initial position: X={:.3} Y={:.3} Z={:.3} E={:.3}", x, y, z, e),
        Err(e) => log::warn!("Get position failed: {}", e),
    }

    // 进入打印模式 - 启用 StatusReport + 运动执行
    match host.client().serial_enter_print_mode().await {
        Ok(()) => {}, // log::info!("Entered print mode"),
        Err(e) => log::warn!("EnterPrintMode failed: {} (motion may not work)", e),
    }

    // Load and parse G-code file
    let content = std::fs::read_to_string(gcode_path)
        .map_err(|e| anyhow::anyhow!("Failed to read G-code file '{}': {}", gcode_path, e))?;

    let lines: Vec<&str> = content.lines().collect();
    let mut total_segments = 0usize;
    let mut executed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let start_time = std::time::Instant::now();

    log::info!("=== Printing {} ({} lines) ===", gcode_path, lines.len());

    for (idx, line) in lines.iter().enumerate() {
        let line_num = (idx + 1) as u32;

        if let Some(parsed) = GCodeParser::parse_line(line, line_num) {
            match &parsed.kind {
                CommandKind::Empty => {
                    skipped += 1;
                }
                _ => {
                    match execute_parsed_command(&host, &parsed, &mut total_segments).await {
                        Ok(()) => executed += 1,
                        Err(e) => {
                            log::error!("  [L{:03}] ERROR: {}", line_num, e);
                            errors += 1;
                        }
                    }
                }
            }
        } else {
            skipped += 1;
        }
    }

    let elapsed = start_time.elapsed();
    log::info!("=== Print complete ===");
    log::info!("  Lines: {} executed, {} skipped, {} errors", executed, skipped, errors);
    log::info!("  Total segments dispatched: {}", total_segments);
    log::info!("  Time: {:.2}s", elapsed.as_secs_f64());

    // Final position
    match host.get_position().await {
        Ok((x, y, z, e)) => log::info!("Final position: X={:.3} Y={:.3} Z={:.3} E={:.3}", x, y, z, e),
        Err(e) => log::warn!("Get final position failed: {}", e),
    }

    // Wait for STM32 to finish executing all queued motion
    if total_segments > 0 {
        log::info!("Waiting for STM32 buffer to drain...");
        match host.client().motion_wait_drain().await {
            Ok(()) => log::info!("STM32 buffer drained"),
            Err(e) => log::warn!("Drain wait failed: {} (motion may still be executing)", e),
        }
    }

    // 退出打印模式
    match host.client().serial_exit_print_mode().await {
        Ok(()) => {}, // log::info!("Exited print mode"),
        Err(e) => log::warn!("ExitPrintMode failed: {}", e),
    }

    // Query and display statistics if requested
    if query_stats {
        log::info!("=== Querying print statistics ===");
        // Wait for all motion to complete (including device-side buffer)
        // Device may have pending steps in various buffers that need time to complete
        log::info!("Waiting 3 seconds for device to finish processing...");
        ::tokio::time::sleep(::tokio::time::Duration::from_secs(3)).await;
        match host.client().motion_query_stats().await {
            Ok(stats) => {
                log::info!("--- Serial Statistics ---");
                log::info!("  Bytes sent: {}", stats.serial.bytes_sent);
                log::info!("  Bytes received: {}", stats.serial.bytes_received);
                log::info!("  Frames sent: {}", stats.serial.frames_sent);
                log::info!("  Frames received: {}", stats.serial.frames_received);
                log::info!("  CRC errors: {}", stats.serial.crc_errors);
                log::info!("  Invalid frames: {}", stats.serial.frames_invalid);

                log::info!("--- Motion Statistics ---");
                log::info!("  Total G-code lines: {}", stats.motion.total_gcode_lines);
                log::info!("  Total batches: {}", stats.motion.total_batches);
                log::info!("  Total steps: {}", stats.motion.total_steps);
                log::info!("  Distance X: {:.3} mm", stats.motion.distance_x_mm);
                log::info!("  Distance Y: {:.3} mm", stats.motion.distance_y_mm);
                log::info!("  Distance Z: {:.3} mm", stats.motion.distance_z_mm);
                log::info!("  Distance E: {:.3} mm", stats.motion.distance_e_mm);
                log::info!("  Peak speed: {:.2} mm/s", stats.motion.peak_speed_mm_per_s);
                log::info!("  Avg speed: {:.2} mm/s", stats.motion.avg_speed_mm_per_s);
                log::info!("  Flow control wait count: {}", stats.motion.flow_control_wait_count);

                log::info!("--- Time Statistics ---");
                log::info!("  Total print time (host): {:.2} s", stats.time.total_print_time_ms as f64 / 1000.0);
                log::info!("  Flow control wait time: {:.2} s", stats.time.flow_control_wait_ms as f64 / 1000.0);
                if let Some(first) = stats.time.first_move_ts_ms {
                    log::info!("  First move timestamp (host): {} ms", first);
                }
                if let Some(last) = stats.time.last_move_ts_ms {
                    log::info!("  Last move timestamp (host): {} ms", last);
                }
                if let Some(device_duration) = stats.time.device_motion_duration_ms {
                    log::info!("  Device motion duration: {:.2} s", device_duration as f64 / 1000.0);
                }
                if let (Some(first_tick), Some(last_tick)) = (stats.time.device_first_step_tick, stats.time.device_last_step_tick) {
                    log::info!("  Device step ticks: {} -> {} (diff: {} ms)", first_tick, last_tick, (last_tick - first_tick) as f64 / 1000.0);
                }
            }
            Err(e) => log::error!("Failed to query stats: {}", e),
        }
    }

    log::info!("Shutting down...");
    
    // Stop services if enabled
    if enable_services {
        app_state.stop_services().await?;
        log::info!("Services stopped");
    }
    
    host.disconnect().await.ok();

    Ok(())
}
