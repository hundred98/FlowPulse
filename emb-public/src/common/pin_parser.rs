//! Pin parser for GPIO pin configuration
//!
//! Parses pin strings like "PE3", "!PA0" into PinInfo.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinInfo {
    pub port: u8,
    pub pin: u8,
    pub inverted: bool,
}

impl fmt::Display for PinInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let port_char = match self.port {
            0 => 'A', 1 => 'B', 2 => 'C', 3 => 'D', 4 => 'E',
            _ => '?',
        };
        if self.inverted {
            write!(f, "!P{}{}", port_char, self.pin)
        } else {
            write!(f, "P{}{}", port_char, self.pin)
        }
    }
}

pub fn parse_pin(pin_str: &str) -> Option<PinInfo> {
    let mut pin_str = pin_str.trim();
    let inverted = pin_str.starts_with('!');
    if inverted {
        pin_str = &pin_str[1..];
    }

    if pin_str.len() < 3 {
        return None;
    }

    let mut chars = pin_str.chars();
    let first_char = chars.next()?;
    if first_char != 'P' && first_char != 'p' {
        return None;
    }

    let port_char = chars.next()?;
    let port = match port_char {
        'A' | 'a' => 0,
        'B' | 'b' => 1,
        'C' | 'c' => 2,
        'D' | 'd' => 3,
        'E' | 'e' => 4,
        _ => return None,
    };

    let pin_str = chars.as_str();
    let pin: u8 = pin_str.parse().ok()?;

    if pin > 15 {
        return None;
    }

    Some(PinInfo {
        port,
        pin,
        inverted,
    })
}

pub fn pin_to_port_pin(pin_str: &str) -> Option<(u8, u8)> {
    parse_pin(pin_str).map(|info| (info.port, info.pin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pin() {
        let result = parse_pin("PE3").unwrap();
        assert_eq!(result.port, 4);
        assert_eq!(result.pin, 3);
        assert!(!result.inverted);

        let result = parse_pin("!PE4").unwrap();
        assert_eq!(result.port, 4);
        assert_eq!(result.pin, 4);
        assert!(result.inverted);

        let result = parse_pin("PA15").unwrap();
        assert_eq!(result.port, 0);
        assert_eq!(result.pin, 15);
        assert!(!result.inverted);

        let result = parse_pin("!PD2").unwrap();
        assert_eq!(result.port, 3);
        assert_eq!(result.pin, 2);
        assert!(result.inverted);

        let result = parse_pin("NC");
        assert!(result.is_none());

        let result = parse_pin("PC0").unwrap();
        assert_eq!(result.port, 2);
        assert_eq!(result.pin, 0);
    }

    #[test]
    fn test_pin_display() {
        assert_eq!(parse_pin("PE3").unwrap().to_string(), "PE3");
        assert_eq!(parse_pin("!PE4").unwrap().to_string(), "!PE4");
    }

    #[test]
    fn test_pin_to_port_pin() {
        assert_eq!(pin_to_port_pin("PE3"), Some((4, 3)));
        assert_eq!(pin_to_port_pin("!PA0"), Some((0, 0)));
        assert_eq!(pin_to_port_pin("NC"), None);
    }
}