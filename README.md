# FlowPulse - 3D Printer Control System

## ⚠️ Important Notice (Must Read)

**This open-source repository contains only the peripheral adapter interface layer. It does not include any core motion algorithms, control engines, or leveling algorithms.**

This code cannot perform complete printing functionality on its own — it must be paired with the official closed-source core engine to operate.

- **Personal Non-Commercial Use**: You may apply for a test core (subject to application review)
- **Commercial / Mass Production**: An official written commercial license is required

**Violation Warning**:
- Commercial use, mass production, reselling, cracking, or reverse engineering are prohibited
- Bulk distribution of core files is prohibited
- Violators will have their device IDs permanently banned, and legal action may be pursued

---

## Project Overview

A modular 3D printer control system designed for enthusiasts, with cross-platform desktop UI, Rust-based host software, and optimized embedded firmware.

### Target Audience

- 🖨️ **3D Printing Enthusiasts**
- 🔧 **DIY Makers**
- 🛠️ **Customization Developers**
- 🏭 **Printer Manufacturers** (Commercial license required)

---

## Key Features

### 1. Complete Control System

FlowPulse is a complete 3D printing control solution:

- **Host + Firmware**: Rust high-performance host + STM32F407 real-time firmware
- **Local HMI**: Supports LVGL, Vue, Solid.js and other interfaces
- **Remote Control**: Remote/ LAN access via Remote API

### 2. Advanced Motion Control

| Algorithm | Characteristics | Execution Time (test-200.gcode) | Actual Speed |
|-----------|----------------|----------------------------------|--------------|
| Trapezoidal | Simple and reliable, suitable for fast printing | 13.085s | 194.92mm/s |
| S-curve | Smooth and flexible, reduces mechanical vibration | 13.418s | 190.08mm/s |
| Six-point Acceleration | Precise control, ultimate smoothness | 14.256s | 178.91mm/s |

| G-code File  | Speed mm/s | Planned Time | Execution Time | Avg Speed mm/s |
|--------------|------------|--------------|----------------|----------------|
| test-50.gcode | 50 | 51.10s | 51.08s | 49.94 |
| test-100.gcode | 100 | 25.75s | 25.71s | 99.22 |
| test-200.gcode | 200 | 13.26s | 13.08s | 195.03 |
| test-300.gcode | 300 | 10.81s | 9.04s | 282.19 |

### 3. High-Speed Communication

- **Tested 57600 baud** supports **400mm/s @ 20000mm/s² acceleration**
- Efficient serial protocol for real-time data transmission

### 4. Precision Timing

Dual-MCU clock synchronization with **50ns + 1ppm** precision:

**~0.5μs error per 10ms segment**

**DCF = t_measured / t_expected**: Real-time drift compensation

**Multiple sync modes**: Balancing highest precision and lower overhead

### 5. Intelligent Flow Control

A reliable communication protocol designed for real-time motion control:

- **Time-based flow control**: Segment-based timing ensures accurate and safe operation
- **Retransmission + Time prediction**: Automatic retry with latency compensation
- **Motion data by sequence number**: Ensures data integrity and order
- **Independent channels**: Non-motion data transmitted via separate path

```
┌─────────────────────────────────────────────────┐
│           Reliable Communication Protocol        │
├─────────────────────────────────────────────────┤
│                                                  │
│   Motion Data      │  Non-Motion Data           │
│   (Sequence #)     │  (Independent Channel)     │
│         ↓          │         ↓                  │
│   ┌────────────────┴──────────────────┐        │
│   │     Flow Control + Retransmit     │        │
│   │     + Time Prediction Optimization│        │
│   └────────────────────────────────────┘        │
│                      ↓                          │
│              Real-time Execution                │
└─────────────────────────────────────────────────┘
```

### 6. Per-Axis Independent Configuration

Each axis can independently set speed and acceleration parameters:

```json
{
  "axes": {
    "x": { "max_speed_mm_per_s": 300, "max_accel": 20000 },
    "y": { "max_speed_mm_per_s": 300, "max_accel": 20000 },
    "z": { "max_speed_mm_per_s": 30, "max_accel": 5000 },
    "e": { "max_speed_mm_per_s": 100, "max_accel": 10000 }
  }
}
```

### 7. Flexible HMI Options

Via shared memory, supports multiple local interfaces:

```
┌─────────────┐     ┌─────────────┐
│   LVGL      │     │   Vue.js    │
└─────────────┘     └─────────────┘
┌─────────────┐     ┌─────────────┐
│  Solid.js   │     │   Preact    │
└─────────────┘     └─────────────┘
```

### 8. Smart Arc Algorithm

Adaptive chord subdivision for the perfect balance of precision and efficiency:

- **Adaptive Subdivision**: Large arcs → fewer segments (save bandwidth), small arcs → more segments (ensure precision)
- **Global Speed Planning**: Avoids unnecessary speed drops at segment joints after subdivision
- **Centripetal Acceleration Limiting**: Critical safety feature prevents excessive radial forces

```
Arc Quality = f(Precision, Efficiency, Safety)
FlowPulse: Maximizes all three simultaneously
```

### 9. Comprehensive JSON Configuration

One JSON file controls everything - no more scattered configs:

| Category | Parameters |
|----------|------------|
| **Motors** | Step/Dir pins, microsteps, current, max speed per axis |
| **Motion** | Velocity profile (trapezoidal/S-curve), jerk, junction deviation |
| **Temperature** | PID gains (Kp/Ki/Kd), sensor type, ADC pin |
| **Limits** | Software limits, homing speed/direction, endstop positions |
| **Hardware** | Heater/fan pins, probe config, UART mapping |

**Benefits**: Type-safe, IDE autocomplete, schema validation, easy backup

---

## Current Status

### ✅ Completed

- Full motion control flow: G-code parsing → Host planning → Firmware execution → Pulse output
- LVGL Demo: File selection, print control, temperature/status feedback
- Remote API: LAN/remote access support

### 🚧 In Progress

- Temperature precision control
- Homing function
- Auto bed leveling
- Resonance compensation

---

## Project Architecture

This project adopts a **hybrid open-source + closed-source** architecture, protecting core technology while supporting community collaboration.

```
┌─────────────────────────────────────────────────────┐
│                         host [MIT]                  │
│         User Interface / HTTP API / CLI Tools / Logging      │
│                          │                          │
│              Direct library calls (emb-public as crate)    │
└─────────────────────────▼───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                    emb-public [MIT]                     │
│   GCode syntax validation, config parsing, peripheral IO,    │
│   error reporting (general preprocessing only — no path,    │
│   leveling, or motion algorithms)                            │
│                          │                          │
│              Local Socket (CoreSocketClient)               │
└─────────────────────────▼───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│               Core Engine [Closed Source | Licensed]       │
│       (Leveling + Motion Planning + Trajectory Optimization  │
│        + License Verification)                              │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  emb-public ↔ External HMI communication                   │
│  Shared Memory - Windows CreateFileMapping                   │
└─────────────────────────────────────────────────────────────┘
```

### Project Structure

```
FlowPulse/
├── host/                        # MIT License - Host application
│   └── src/
│       ├── remote_api/          # HTTP/WebSocket API
│       ├── realtime_monitor/    # Real-time monitoring
│       └── ...
│
├── emb-public/                  # MIT License - Reusable core modules
│   └── src/
│       ├── gcode/              # G-code parser
│       ├── temperature/        # Temperature control interface
│       ├── print_control/      # Print job management
│       └── ...
│
├── emb-api/                     # MIT License - Shared API types
│
├── releases/                    # Proprietary - Pre-built binaries
│   └── Linux/emb-core-server
│
├── config/                      # Configuration files
├── gcodes/                      # Test G-code files
├── docs/                        # Documentation
└── Cargo.toml                   # Rust workspace
```

---

## Open Source Strategy

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   🎯 Goal: Open Source Community + Core Tech Protection    │
│                                                             │
│   ✅ Hobbyists: Free use of full features                   │
│   ✅ Developers: Can contribute to public modules           │
│   ❌ Commercial companies: Commercial license required      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Open Source / Closed Source Boundary

#### ✅ Open Source Components (MIT License)

**host layer** (MIT):
- HTTP REST API, WebSocket real-time communication
- CLI command-line tools, user interface interaction
- Logging display, status monitoring, error alerts
- Configuration management, user preferences

**emb-public layer** (MIT):
- GCode syntax validation, lexical parsing (no path preprocessing)
- UART serial basic connection, send/receive encapsulation
- Project configuration file parsing (TOML/YAML)
- Peripheral IO encapsulation, device integration
- Status reporting, error log encapsulation

| Component | License | Source | Description |
|-----------|---------|--------|-------------|
| `emb-public` | MIT | Public | Reusable core modules |
| `host` | MIT | Public | Host application |
| Documents | MIT | Public | Documentation |

#### 🔒 Closed Source Components (License Required)

The core engine is provided by official closed-source modules, including leveling algorithms, motion planning, trajectory optimization, and other core functionality.

| Component | License | Binary | Description |
|-----------|---------|--------|-------------|
| `emb-core` | Proprietary | Pre-built | Motion planning, step generation |
| `device-firmware` | Proprietary | Pre-built | STM32F407 firmware |

### Hard Red Lines (Do Not Submit)

**The following content is strictly prohibited from being submitted to the open-source layer**:
- ❌ Leveling algorithms, mesh compensation calculations
- ❌ Motion planning, trajectory generation, S-curve acceleration/deceleration
- ❌ Lookahead algorithms, input shaping, pressure advance
- ❌ G-code path preprocessing, motion command optimization
- ❌ Encryption, license verification logic
- ❌ Hardware ID binding, permission verification

**Risk Notice**:
The above content falls within the scope of the core engine. Once open-sourced, it will undermine the project's business model.

---

## Comparison with Klipper

| Feature | Klipper | FlowPulse | Advantage |
|---------|---------|-----------|-----------|
| Host Language | Python | Rust | FlowPulse: Memory safety, better performance |
| UI | Web-based | Native LVGL | FlowPulse: Works without network, lower latency |
| Configuration | Text config | JSON + UI | FlowPulse: Type-safe, programmatic |
| Build System | Makefile | Cargo (Rust) | FlowPulse: Dependency management |
| Modularity | Basic | Advanced | FlowPulse: Clear license separation |
| Extensibility | Macros | Full language | FlowPulse: Compile-time checks |
| Firmware | C (AVR, ARM) | C (STM32F4) | Similar |

### FlowPulse Advantages

1. **Rust Performance + Safety**
   - Memory safety without garbage collection
   - Zero-cost abstractions
   - Thread safety

2. **Native Desktop UI**
   - Works without network connection
   - Lower latency communication
   - Runs on low-power devices

3. **Modular License Strategy**
   - `emb-public` (MIT): Hobbyists and third-party developers can use freely
   - Core technology protected: Commercial use requires license

4. **Type-Safe Configuration**
   - JSON Schema validation
   - IDE support
   - Runtime validation

---

## Quick Start

### 1. Download Pre-built Binaries

```bash
# Download latest version from Releases
https://github.com/hundred98/FlowPulse/releases
```
Run:
```bash
.\emb-core-server.exe
```

### 2. Flash Firmware

```bash
# Using STM32CubeProgrammer
# 1. Connect ST-Link programmer
# 2. Load firmware.bin (start address: 0x08000000)
# 3. Click "Start Programming"
```

### 3. Build Host Software

```bash
# Install Rust (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone
git clone https://github.com/hundred98/FlowPulse.git
cd FlowPulse

# Build
cargo build --release

# Run
.\target\release\host.exe "gcodes/test-200.gcode"
```

### 4. Launch Desktop UI

```bash
# Windows (MSYS2)
cd LVGL/lvgl-pc-sim
mkdir -p build && cd build
cmake .. -G "MinGW Makefiles"
mingw32-make -j4
./lvgl-pc-sim.exe

# Linux
cd LVGL/lvgl-pc-sim
mkdir -p build && cd build
cmake ..
make -j4
./lvgl-pc-sim
```

---

## Testing

### Hardware Setup

1. Connect USB-to-TTL module from PC to STM32F407 UART3
2. Modify the serial port in configuration file:

```json
{
  "communication": {
    "serial": {
      "port": "COM7",
      "baud_rate": 57600,
      "data_bits": 8,
      "parity": "None",
      "stop_bits": 1,
      "timeout_ms": 1000,
      "flow_control": false
    }
  }
}
```

### Test 1: Direct G-code Print

```bash
.\target\release\host.exe "gcodes/test-200.gcode"
```

---

## Community Contribution Rules

### Allowed Contributions

PRs are only accepted for open-source layer code:
- Driver adaptation, hardware compatibility improvements
- UI optimization, bug fixes
- Documentation improvements, configuration optimization
- Testing tools, example code

### CLA Agreement

All community contributors must sign a simple CLA agreement:
- Contributors retain copyright to their contributed code, but must sign the CLA to grant the project a usage license
- Contributions may be freely used in both open-source and commercial versions

### Prohibited Contributions

- ❌ Submitting core algorithm or proprietary logic related code
- ❌ Submitting leveling or motion planning related code
- ❌ Submitting encryption or license verification logic

---

## Development

### Contributing

1. **Fork** the repository
2. **Clone** your fork
3. **Create** a feature branch
4. **Make** your changes
5. **Test** thoroughly
6. **Submit** a Pull Request

### Development Areas

| Area | Open Source | Suitable For |
|------|------------|--------------|
| UI/UX Design | ✅ | UI designers, frontend developers |
| G-code Processing | ✅ | Parser developers |
| Temperature Control | ✅ | Control algorithm engineers |
| API Development | ✅ | Web/backend developers |
| Documentation | ✅ | Technical writers |
| Motion Algorithms | 🔒 | Commercial license required |
| Flow Control | 🔒 | Commercial license required |
| Firmware | 🔒 | Commercial license required |

### emb-public Module Contribution

`emb-public` is fully MIT licensed, contributions welcome:

```
emb-public/src/
├── gcode/           # G-code parser
├── temperature/     # Temperature control algorithms
├── flow_control/    # Flow control interface
├── state_machine/   # State machine framework
├── message_queue/   # Message queue
└── common/          # Common utilities
```

### Features

#### LVGL Desktop UI

- **Dashboard**: Real-time printer status monitoring
- **Temperature**: Visual gauges for hotend/bed
- **File Manager**: G-code browsing
- **Settings**: System configuration

#### Host Software (Rust)

- **G-code Processing**: Full parser with optimization
- **State Management**: Robust state machine
- **Remote API**: HTTP + WebSocket interface
- **Configuration**: JSON-based type-safe config

#### Embedded Firmware

- **USART3**: High-speed serial communication
- **GPIO Control**: Endstops, heaters
- **Timer PWM**: Precision control

---

## Licensing Rules

### Personal Non-Commercial Definition

Applies only to: personal desktop DIY, home use, personal learning and research.

**The following activities do NOT qualify as personal non-commercial use**:
- Studio work orders, repair/modification services, paid assembly
- Storefront sales, e-commerce reselling of complete machines
- Small-batch testing, factory mass production, commercial projects
- Applying on behalf of others, bulk core acquisition, redistribution

### Core Application Process

1. The public repository provides only: open-source host code + firmware BIN basic trial package
2. **The full-featured core is not freely available and will not be distributed via direct message**
3. Users must submit the official application form with:
   - Real use case, actual device photos/video
   - Number of owned devices, DIY experience description
   - Community account profile verification
   - Mainboard MCU unique ID
   - Signed Non-Commercial Use Commitment

### Tiered Distribution

- **Pure hobbyist**: Approved — receive a single-device-bound full-featured test core (time-limited)
- **Suspected studio/business**: Rejected
- **Enterprise commercial needs**: Directed to official commercial licensing channels

---

## Security Protection

### Hardware Unique Binding

- Reads the factory-programmed unique ID of the firmware MCU
- Core license is bound one-to-one to a single board ID
- One board, one license — copying or transplanting will cause immediate failure

### Trial Version Feature Restrictions

- Maximum print speed and travel size are limited
- Advanced self-developed compensation and lookahead optimization algorithms are disabled
- License validity period set (30 days)
- Built-in invisible traceability watermark

---

## License Compliance

- **host**: MIT License
- **emb-public**: MIT License
- **Core Engine**: Proprietary, licensed use

The open-source components and closed-source core communicate via Local Socket and are legally independent software.

---

## Tech Stack

- **Host Software**: Rust + Tokio async runtime
- **Firmware**: C (STM32)
- **Communication**: Local Socket + UART

---

## Pre-built Binaries

The `releases/` directory in this repository provides pre-built executables:

| File | License | Description |
|------|---------|-------------|
| emb-core-server | Proprietary | For authorized users only |

- **Source Code**: MIT License (see each crate's LICENSE file)
- **Binaries**: Proprietary, authorization required

---

## Contact

- **Core Application**: Submit the official application form (see project Wiki)
- **Commercial Licensing**: Contact the project founder
- **Technical Support**: Community forums, GitHub Issues

---

## License

FlowPulse uses a dual licensing model:

- **Open Source Components**: MIT License
- **Core Technology**: Proprietary, commercial license required

For commercial licensing, please contact [hundred98@163.com](mailto:hundred98@163.com).
