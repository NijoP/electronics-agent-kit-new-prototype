//! Tool registry: typed command tools (mutations, generated from `eak_runtime::Command`)
//! and read-only tools (inspection, reports, calculators).

pub mod commands;
pub mod readonly;
pub mod schema;

pub use commands::{command_tools, decode as decode_command, CommandTool, Domain};
pub use readonly::{read_tools, ReadTool};
