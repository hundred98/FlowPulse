//! G-code parser
//!
//! This module provides G-code parsing functionality.

use super::types::*;
use emb_api::MCommand;
use std::collections::HashMap;

/// G-code解析器
pub struct GCodeParser;

impl GCodeParser {
    /// 解析单行 G-code
    pub fn parse_line(line: &str, line_number: u32) -> Option<ParsedCommand> {
        let trimmed = line.trim();

        // 跳过空行和注释
        if trimmed.is_empty() || trimmed.starts_with(';') {
            return Some(ParsedCommand {
                line_number,
                raw: line.to_string(),
                kind: CommandKind::Empty,
            });
        }

        // 移除行内注释
        let code = if let Some(pos) = trimmed.find(';') {
            &trimmed[..pos]
        } else {
            trimmed
        };

        let code = code.trim();
        if code.is_empty() {
            return Some(ParsedCommand {
                line_number,
                raw: line.to_string(),
                kind: CommandKind::Empty,
            });
        }

        // 解析命令
        let kind = Self::parse_command(code);

        Some(ParsedCommand {
            line_number,
            raw: line.to_string(),
            kind,
        })
    }

    /// 解析命令
    fn parse_command(code: &str) -> CommandKind {
        let parts: Vec<&str> = code.split_whitespace().collect();
        if parts.is_empty() {
            return CommandKind::Empty;
        }

        let cmd = parts[0].to_uppercase();
        let params = Self::parse_params(&parts[1..]);

        // 判断命令类型
        if cmd.starts_with('G') {
            Self::parse_g_command(&cmd, &params)
        } else if cmd.starts_with('M') {
            Self::parse_m_command(&cmd, &params)
        } else {
            CommandKind::Unsupported {
                raw: code.to_string(),
            }
        }
    }

    /// 解析参数
    fn parse_params(parts: &[&str]) -> HashMap<char, f32> {
        let mut params = HashMap::new();

        for part in parts {
            if part.is_empty() {
                continue;
            }

            let part = *part;
            let letter = part.chars().next().unwrap_or(' ');

            if letter.is_alphabetic() {
                let value_str = &part[1..];
                if let Ok(value) = value_str.parse::<f32>() {
                    params.insert(letter.to_ascii_uppercase(), value);
                }
            }
        }

        params
    }

    /// 解析G指令
    fn parse_g_command(cmd: &str, params: &HashMap<char, f32>) -> CommandKind {
        let num_str = &cmd[1..];
        let num = match num_str.parse::<u32>() {
            Ok(n) => n,
            Err(_) => return CommandKind::Unsupported { raw: cmd.to_string() },
        };

        let motion = match num {
            // G0/G1 - 线性移动
            0 | 1 => MotionCommand::LinearMove {
                x: params.get(&'X').copied(),
                y: params.get(&'Y').copied(),
                z: params.get(&'Z').copied(),
                e: params.get(&'E').copied(),
                f: params.get(&'F').copied(),
                is_rapid: num == 0,
            },

            // G2/G3 - 圆弧移动
            2 | 3 => {
                let i = params.get(&'I').copied().unwrap_or(0.0);
                let j = params.get(&'J').copied().unwrap_or(0.0);

                MotionCommand::ArcMove {
                    x: params.get(&'X').copied(),
                    y: params.get(&'Y').copied(),
                    z: params.get(&'Z').copied(),
                    e: params.get(&'E').copied(),
                    f: params.get(&'F').copied(),
                    i,
                    j,
                    is_cw: num == 2,
                }
            }

            // G28 - 回零
            28 => MotionCommand::Home {
                x: params.contains_key(&'X'),
                y: params.contains_key(&'Y'),
                z: params.contains_key(&'Z'),
            },

            // G90 - 绝对定位
            90 => MotionCommand::AbsolutePositioning,

            // G91 - 相对定位
            91 => MotionCommand::RelativePositioning,

            // G92 - 设置位置
            92 => MotionCommand::SetPosition {
                x: params.get(&'X').copied(),
                y: params.get(&'Y').copied(),
                z: params.get(&'Z').copied(),
                e: params.get(&'E').copied(),
            },

            // G20 - 英寸单位
            20 => MotionCommand::Inches,

            // G21 - 毫米单位
            21 => MotionCommand::Millimeters,

            _ => return CommandKind::Unsupported { raw: cmd.to_string() },
        };

        CommandKind::Motion(motion)
    }

    /// 解析M指令
    fn parse_m_command(cmd: &str, params: &HashMap<char, f32>) -> CommandKind {
        let num_str = &cmd[1..];
        let num = match num_str.parse::<u32>() {
            Ok(n) => n,
            Err(_) => return CommandKind::Unsupported { raw: cmd.to_string() },
        };

        let m_command = match num {
            // M82 - 挤出机绝对模式
            82 => MCommand::ExtruderAbsoluteMode,

            // M83 - 挤出机相对模式
            83 => MCommand::ExtruderRelativeMode,

            // M92 - 设置步数
            92 => MCommand::SetStepsPerMm {
                x: params.get(&'X').copied(),
                y: params.get(&'Y').copied(),
                z: params.get(&'Z').copied(),
                e: params.get(&'E').copied(),
            },

            // M104 - 设置热端温度
            104 => MCommand::SetHotendTemp {
                tool: params.get(&'T').copied().unwrap_or(0.0) as u8,
                temp: params.get(&'S').copied().unwrap_or(0.0),
            },

            // M106 - 设置风扇速度
            106 => MCommand::SetFanSpeed {
                index: params.get(&'P').copied().unwrap_or(0.0) as u8,
                speed: params.get(&'S').copied().unwrap_or(255.0) as u8,
            },

            // M107 - 关闭风扇
            107 => MCommand::FanOff {
                index: params.get(&'P').copied().unwrap_or(0.0) as u8,
            },

            // M109 - 设置热端温度并等待
            109 => MCommand::WaitHotendTemp {
                tool: params.get(&'T').copied().unwrap_or(0.0) as u8,
                temp: params.get(&'S').copied().unwrap_or(0.0),
            },

            // M140 - 设置热床温度
            140 => MCommand::SetBedTemp {
                temp: params.get(&'S').copied().unwrap_or(0.0),
            },

            // M190 - 设置热床温度并等待
            190 => MCommand::WaitBedTemp {
                temp: params.get(&'S').copied().unwrap_or(0.0),
            },

            // M201 - 设置加速度
            201 => MCommand::SetAcceleration {
                x: params.get(&'X').copied(),
                y: params.get(&'Y').copied(),
                z: params.get(&'Z').copied(),
                e: params.get(&'E').copied(),
            },

            // M203 - 设置最大速度
            203 => MCommand::SetMaxVelocity {
                x: params.get(&'X').copied(),
                y: params.get(&'Y').copied(),
                z: params.get(&'Z').copied(),
                e: params.get(&'E').copied(),
            },

            // M204 - 设置加速度参数
            204 => MCommand::SetAccelParams {
                travel: params.get(&'T').copied(),
                print: params.get(&'P').copied(),
                retract: params.get(&'R').copied(),
            },

            _ => return CommandKind::Unsupported { raw: cmd.to_string() },
        };

        CommandKind::Machine(m_command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_line() {
        let result = GCodeParser::parse_line("", 1).unwrap();
        assert!(matches!(result.kind, CommandKind::Empty));
    }

    #[test]
    fn test_parse_comment() {
        let result = GCodeParser::parse_line("; This is a comment", 1).unwrap();
        assert!(matches!(result.kind, CommandKind::Empty));
    }

    #[test]
    fn test_parse_g0() {
        let result = GCodeParser::parse_line("G0 X100 Y200 F3000", 1).unwrap();
        if let CommandKind::Motion(motion) = result.kind {
            if let MotionCommand::LinearMove { x, y, f, is_rapid, .. } = motion {
                assert_eq!(x, Some(100.0));
                assert_eq!(y, Some(200.0));
                assert_eq!(f, Some(3000.0));
                assert!(is_rapid);
            } else {
                panic!("Expected LinearMove");
            }
        } else {
            panic!("Expected Motion command");
        }
    }

    #[test]
    fn test_parse_g1() {
        let result = GCodeParser::parse_line("G1 X50 Y75 Z0.2 E1.5 F1500", 1).unwrap();
        if let CommandKind::Motion(motion) = result.kind {
            if let MotionCommand::LinearMove { x, y, z, e, f, is_rapid, .. } = motion {
                assert_eq!(x, Some(50.0));
                assert_eq!(y, Some(75.0));
                assert_eq!(z, Some(0.2));
                assert_eq!(e, Some(1.5));
                assert_eq!(f, Some(1500.0));
                assert!(!is_rapid);
            } else {
                panic!("Expected LinearMove");
            }
        } else {
            panic!("Expected Motion command");
        }
    }

    #[test]
    fn test_parse_m104() {
        let result = GCodeParser::parse_line("M104 S200", 1).unwrap();
        if let CommandKind::Machine(m_cmd) = result.kind {
            if let MCommand::SetHotendTemp { tool, temp } = m_cmd {
                assert_eq!(tool, 0);
                assert_eq!(temp, 200.0);
            } else {
                panic!("Expected SetHotendTemp");
            }
        } else {
            panic!("Expected Machine command");
        }
    }

    #[test]
    fn test_parse_m109() {
        let result = GCodeParser::parse_line("M109 S210", 1).unwrap();
        if let CommandKind::Machine(m_cmd) = result.kind {
            if let MCommand::WaitHotendTemp { tool, temp } = m_cmd {
                assert_eq!(tool, 0);
                assert_eq!(temp, 210.0);
            } else {
                panic!("Expected WaitHotendTemp");
            }
        } else {
            panic!("Expected Machine command");
        }
    }

    #[test]
    fn test_parse_m203() {
        let result = GCodeParser::parse_line("M203 X100 Y100 Z5 E50", 1).unwrap();
        if let CommandKind::Machine(m_cmd) = result.kind {
            if let MCommand::SetMaxVelocity { x, y, z, e } = m_cmd {
                assert_eq!(x, Some(100.0));
                assert_eq!(y, Some(100.0));
                assert_eq!(z, Some(5.0));
                assert_eq!(e, Some(50.0));
            } else {
                panic!("Expected SetMaxVelocity");
            }
        } else {
            panic!("Expected Machine command");
        }
    }
}
