//! Command parsing and safety utilities shared across Praxis crates.

pub mod shell_detect;

pub mod bash;
pub mod command_safety;
pub mod delay_probe;
pub mod parse_command;
pub mod powershell;

pub use command_safety::is_dangerous_command;
pub use command_safety::is_safe_command;
pub use delay_probe::delay_probe_fingerprint;
