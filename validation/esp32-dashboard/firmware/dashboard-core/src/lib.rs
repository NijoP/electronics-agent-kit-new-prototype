//! Hardware-independent logic of the ESP32-C3 environment dashboard.
//!
//! `no_std`, no allocation. Everything here is unit-tested on the host with `cargo test`;
//! the ESP32-C3 firmware and the host simulator/dashboard both link this crate, so the code
//! path that is tested is the code path that runs on the device.

#![cfg_attr(not(test), no_std)]

pub mod app;
pub mod crc;
pub mod display;
pub mod frame;
pub mod model;
pub mod pins;
pub mod sht4x;
