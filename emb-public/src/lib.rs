pub mod common;
pub mod core_client;
pub mod config_adapter;
pub mod printer_config;
pub mod config_protocol;

pub use common::{EmbError, EmbResult};
pub use core_client::{CoreSocketClient, CoreClientConfig};
pub use printer_config::{
    PrinterJsonConfig, PrinterParams, MotorParams, LimitSwitchParams,
    TemperatureParams, HeaterParams, FanParams, ProbeParams,
    load_config_from_file, parse_json_config,
};
pub use config_protocol::{
    ConfigFrameBuilder, create_config_frames, validate_config,
};