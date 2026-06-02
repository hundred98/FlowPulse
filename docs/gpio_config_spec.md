# GPIO 配置规范文档

## 1. 整体结构

```json
{
  "version": "1.0",
  "motor": [...],
  "gpio": {
    "output": [...],
    "input": [...]
  }
}
```

---

## 2. 输出 GPIO (`gpio.output`)

### 配置示例

```json
{
  "name": "box_fan",
  "pin": "PA8",
  "type": "pwm",
  "active_high": true,
  "pwm_freq_hz": 100,
  "default_value": 0.0,
  "shutdown_value": 0.0,
  "max_value": 1.0
}
```

### 字段说明

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `name` | string | ✓ | - | 引脚名称，用于 `SET_PIN` 指令 |
| `pin` | string | ✓ | - | GPIO 引脚，支持 `!` 前缀反转（如 `PA8`, `!PB0`） |
| `type` | string | ✓ | - | 输出类型：`pwm` 或 `digital` |
| `active_high` | bool | | `true` | 高电平有效 |
| `pwm_freq_hz` | u16 | | `100` | PWM 频率（仅 `type=pwm` 有效） |
| `default_value` | float | | `0.0` | 启动时的默认值（范围 0.0-1.0） |
| `shutdown_value` | float | | `0.0` | 关机/急停时的值（范围 0.0-1.0） |
| `max_value` | float | | `1.0` | 最大值限制（范围 0.0-1.0） |

### 输出类型说明

| type | VALUE 含义 |
|------|------------|
| `pwm` | 0.0-1.0 对应占空比 0%-100% |
| `digital` | 0.0 = 关闭，> 0 = 开启 |

### G-code 指令

```
SET_PIN PIN=box_fan VALUE=0.5    ; PWM 输出 50% 占空比
SET_PIN PIN=chamber_led VALUE=1  ; 数字输出开启
SET_PIN PIN=chamber_led VALUE=0  ; 数字输出关闭
```

---

## 3. 输入 GPIO (`gpio.input`)

### 3.1 数字输入

#### 配置示例

```json
{
  "name": "filament_sensor",
  "pin": "!PC5",
  "type": "digital",
  "pull": "up",
  "active_high": false,
  "debounce_ms": 50,
  "event": {
    "action": "filament_runout"
  },
  "report": {
    "mode": "on_change",
    "trigger": "rising"
  }
}
```

#### 字段说明

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `name` | string | ✓ | - | 引脚名称，用于 `QUERY_PIN` 指令 |
| `pin` | string | ✓ | - | GPIO 引脚，支持 `!` 前缀反转 |
| `type` | string | ✓ | - | 固定为 `digital` |
| `pull` | string | ✓ | - | 上下拉：`up` / `down` / `none` |
| `active_high` | bool | | `true` | 高电平有效，定义物理电平到逻辑值的映射 |
| `debounce_ms` | u16 | | `0` | 消抖时间（毫秒） |
| `event` | object | | - | 事件配置，不配置则无事件回调 |
| `report` | object | | - | 上报配置，不配置则不上报 |

#### event 字段（可选）

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `action` | string | ✓ | - | 动作类型：`filament_runout`, `power_loss` 等 |

**触发逻辑**：当逻辑值变为 1（激活）时触发 `action`。

#### report 字段（可选）

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `mode` | string | ✓ | - | 上报模式：`on_change` 或 `interval` |
| `trigger` | string | | `both` | 触发条件（mode=on_change）：`rising` / `falling` / `both` |
| `interval_ms` | u32 | | - | 定时间隔（mode=interval，必填） |

#### 上报格式

```
<filament_sensor:1>
<door_sensor:0>
```

---

### 3.2 模拟输入

#### 配置示例

```json
{
  "name": "power_monitor",
  "pin": "PC2",
  "type": "analog",
  "pull": "none",
  "adc_resolution": 12,
  "calibration": {
    "offset": 0.05,
    "scale": 1.1,
    "min_value": 0.0,
    "max_value": 1.0
  },
  "event": {
    "action": "power_loss",
    "threshold_below": 0.1,
    "threshold_above": 0.9
  },
  "report": {
    "mode": "interval",
    "interval_ms": 1000
  }
}
```

#### 字段说明

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `name` | string | ✓ | - | 引脚名称 |
| `pin` | string | ✓ | - | ADC 引脚 |
| `type` | string | ✓ | - | 固定为 `analog` |
| `pull` | string | ✓ | - | 上下拉：`up` / `down` / `none` |
| `adc_resolution` | u8 | | `12` | ADC 分辨率：8 / 10 / 12 |
| `calibration` | object | | - | 校准参数 |
| `event` | object | | - | 事件配置 |
| `report` | object | | - | 上报配置 |

#### calibration 字段（可选）

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `offset` | float | | `0.0` | 偏移校准值 |
| `scale` | float | | `1.0` | 缩放系数 |
| `min_value` | float | | `0.0` | 最小值限制 |
| `max_value` | float | | `1.0` | 最大值限制 |

**计算公式**：
```
final_value = clamp((raw_value + offset) * scale, min_value, max_value)
```

#### event 字段（可选）

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `action` | string | ✓ | - | 动作类型 |
| `threshold_below` | float | | - | 低于此值触发 |
| `threshold_above` | float | | - | 高于此值触发 |

**注意**：`threshold_below` 和 `threshold_above` 可同时配置。

#### report 字段（可选）

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|:----:|--------|------|
| `mode` | string | ✓ | - | 上报模式：`on_change` 或 `interval` |
| `threshold` | float | | `0.0` | 变化阈值（mode=on_change），变化超过此值才上报 |
| `interval_ms` | u32 | | - | 定时间隔（mode=interval，必填） |

**注意**：`threshold` 和 `interval_ms` 由 `mode` 决定，互斥。

#### 上报格式

```
<power_monitor:0.73>
```

---

## 4. G-code 指令汇总

| 指令 | 说明 | 示例 |
|------|------|------|
| `SET_PIN` | 设置输出引脚值 | `SET_PIN PIN=box_fan VALUE=0.5` |
| `QUERY_PIN` | 查询输入引脚值 | `QUERY_PIN PIN=filament_sensor` |

**QUERY_PIN 返回格式**：
```
Result: filament_sensor=1
Result: power_monitor=0.73
```

---

## 5. 完整配置示例

```json
{
  "version": "1.0",
  "motor": [
    { "axis": "X", "step_pin": "PE3", "dir_pin": "PE2", "enable_pin": "!PE4", "steps_per_mm": 80, "max_speed_mm_per_s": 400, "max_accel": 20000, "position_min": 0, "position_max": 300, "driver": { "uart_pin": "", "microsteps": 16, "current_ma": 800, "hold_current_ma": 500, "stealthchop_threshold": 999999 } },
    { "axis": "Y", "step_pin": "PE0", "dir_pin": "PB9", "enable_pin": "!PE1", "steps_per_mm": 80, "max_speed_mm_per_s": 400, "max_accel": 20000, "position_min": 0, "position_max": 300, "driver": { "uart_pin": "", "microsteps": 16, "current_ma": 800, "hold_current_ma": 500, "stealthchop_threshold": 999999 } },
    { "axis": "Z", "step_pin": "PB5", "dir_pin": "PB4", "enable_pin": "!PB8", "steps_per_mm": 400, "max_speed_mm_per_s": 20, "max_accel": 500, "position_min": 0, "position_max": 200, "driver": { "uart_pin": "", "microsteps": 16, "current_ma": 800, "hold_current_ma": 500, "stealthchop_threshold": 999999 } },
    { "axis": "E0", "step_pin": "PD6", "dir_pin": "!PD3", "enable_pin": "!PB3", "steps_per_mm": 80, "max_speed_mm_per_s": 50, "max_accel": 5000, "position_min": 0, "position_max": 0, "driver": { "uart_pin": "", "microsteps": 16, "current_ma": 600, "hold_current_ma": 300, "stealthchop_threshold": 999999 }, "extruder": { "nozzle_diameter_mm": 0.4, "filament_diameter_mm": 1.75, "max_flow_rate": 25 } }
  ],
  "gpio": {
    "output": [
      {
        "name": "box_fan",
        "pin": "PA8",
        "type": "pwm",
        "active_high": true,
        "pwm_freq_hz": 100,
        "default_value": 0.0,
        "shutdown_value": 0.0,
        "max_value": 1.0
      },
      {
        "name": "chamber_led",
        "pin": "!PB0",
        "type": "digital",
        "active_high": true,
        "default_value": 0
      }
    ],
    "input": [
      {
        "name": "filament_sensor",
        "pin": "!PC5",
        "type": "digital",
        "pull": "up",
        "active_high": false,
        "debounce_ms": 50,
        "event": {
          "action": "filament_runout"
        },
        "report": {
          "mode": "on_change",
          "trigger": "rising"
        }
      },
      {
        "name": "door_sensor",
        "pin": "PD0",
        "type": "digital",
        "pull": "up",
        "active_high": false,
        "report": {
          "mode": "interval",
          "interval_ms": 5000
        }
      },
      {
        "name": "power_monitor",
        "pin": "PC2",
        "type": "analog",
        "pull": "none",
        "adc_resolution": 12,
        "calibration": {
          "offset": 0.05,
          "scale": 1.1,
          "min_value": 0.0,
          "max_value": 1.0
        },
        "event": {
          "action": "power_loss",
          "threshold_below": 0.1,
          "threshold_above": 0.9
        },
        "report": {
          "mode": "interval",
          "interval_ms": 1000
        }
      }
    ]
  }
}
```
