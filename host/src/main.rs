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
use emb_public::config_adapter;
use emb_public::config_protocol::ConfigFrameBuilder;
use std::env;

/// Simple G-code line parser: extracts G-code command letter/number and parameters.
/// Only handles G0/G1/G2/G3/G28/G92.
struct GCodeLine {
    cmd: String,
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    e: Option<f32>,
    f: Option<f32>,
    i: Option<f32>,
    j: Option<f32>,
}

fn parse_gcode_line(line: &str) -> Option<GCodeLine> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(';') || line.starts_with('(') {
        return None;
    }

    let cmd_start = line.chars().position(|c| c.is_ascii_alphabetic())?;
    let rest = &line[cmd_start..];

    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let cmd = tokens[0].to_uppercase();
    if !cmd.starts_with('G') {
        return None;
    }

    let mut parsed = GCodeLine {
        cmd,
        x: None, y: None, z: None, e: None, f: None, i: None, j: None,
    };

    for token in &tokens[1..] {
        let token = token.to_uppercase();
        let key = token.chars().next()?;
        let val: Option<f32> = token[1..].parse().ok();
        match key {
            'X' => parsed.x = val,
            'Y' => parsed.y = val,
            'Z' => parsed.z = val,
            'E' => parsed.e = val,
            'F' => parsed.f = val,
            'I' => parsed.i = val,
            'J' => parsed.j = val,
            _ => {}
        }
    }

    Some(parsed)
}

/// Execute a single parsed G-code line.
/// Planning + mm→steps + serial dispatch all happen in emb-core-server.
async fn execute_gcode(
    host: &PrinterHostV2,
    gcode: &GCodeLine,
    line_num: usize,
    total_segments: &mut usize,
) -> Result<(), String> {
    match gcode.cmd.as_str() {
        "G0" | "G00" | "G1" | "G01" => {
            let dispatched = host.client().motion_dispatch(
                &gcode.cmd, gcode.x, gcode.y, gcode.z, gcode.e, gcode.f,
            ).await?;
            *total_segments += dispatched;
            if dispatched > 0 {
                // log::info!("  [L{:03}] {} => {} segments (total: {})",
                //     line_num, gcode.cmd, dispatched, total_segments);
            } else {
                log::debug!("  [L{:03}] {} => 0 segments (skipped, distance too small)", line_num, gcode.cmd);
            }
            Ok(())
        }
        "G2" | "G02" | "G3" | "G03" => {
            let i = gcode.i.unwrap_or(0.0);
            let j = gcode.j.unwrap_or(0.0);
            let dispatched = host.client().motion_dispatch_arc(
                &gcode.cmd, gcode.x, gcode.y, gcode.z, gcode.e, gcode.f,
                Some(emb_api::ArcParamsApi {
                    i, j,
                    direction: if gcode.cmd.ends_with('2') { 0 } else { 1 },
                }),
            ).await?;
            *total_segments += dispatched;
            if dispatched > 0 {
                // log::info!("  [L{:03}] {} => {} segments (total: {})",
                //     line_num, gcode.cmd, dispatched, total_segments);
            }
            Ok(())
        }
        "G28" => {
            let dispatched = host.client().motion_dispatch(
                "G28", None, None, None, None, None,
            ).await?;
            *total_segments += dispatched;
            // log::info!("  [L{:03}] G28 HOME => {} segments", line_num, dispatched);
            Ok(())
        }
        "G92" => {
            host.set_position(gcode.x, gcode.y, gcode.z, gcode.e).await?;
            // log::info!("  [L{:03}] G92 SET POSITION => OK", line_num);
            Ok(())
        }
        other => {
            log::debug!("  [L{:03}] Skipping unsupported: {}", line_num, other);
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
    let configs = config_adapter::load_configs(config_dir)
        .unwrap_or_else(|e| {
            log::error!("Failed to load configs: {}", e);
            std::process::exit(1);
        });

    log::info!(
        "Loaded {} motors, printer model: {}",
        configs.hardware.motor.len(),
        configs.printer.printer_model,
    );

    // Merge configs into MotionConfig JSON
    let motion_json = config_adapter::build_motion_config_json(&configs).unwrap_or_else(|e| {
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
    let serial_cfg = &configs.printer.communication.as_ref()
        .and_then(|c| c.serial.as_ref());
    let (serial_port, serial_baud) = match serial_cfg {
        Some(cfg) => (cfg.port.clone(), cfg.baud_rate),
        None => {
            log::error!("No serial config found in printer.json");
            std::process::exit(1);
        }
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

    // Step 3.5: Send config frames to STM32 device (motor pins, etc.)
    let printer_config = config_adapter::build_printer_config(&configs);

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

    // Enter special mode (print mode) on STM32 — enables StatusReport + motion execution
    match host.client().serial_enter_special_mode().await {
        Ok(()) => {}, // log::info!("Entered special mode (print mode)"),
        Err(e) => log::warn!("EnterSpecialMode failed: {} (motion may not work)", e),
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

    // log::info!("=== Printing {} ({} lines) ===", gcode_path, lines.len());

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;

        if let Some(gcode) = parse_gcode_line(line) {
            match execute_gcode(&host, &gcode, line_num, &mut total_segments).await {
                Ok(()) => executed += 1,
                Err(e) => {
                    log::error!("  [L{:03}] ERROR: {}", line_num, e);
                    errors += 1;
                }
            }
        } else {
            skipped += 1;
        }
    }

    let elapsed = start_time.elapsed();
    // log::info!("=== Print complete ===");
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

    // Exit special mode (print mode) on STM32
    match host.client().serial_exit_special_mode().await {
        Ok(()) => {}, // log::info!("Exited special mode"),
        Err(e) => log::warn!("ExitSpecialMode failed: {}", e),
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
