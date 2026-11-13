//! Firmware application logic shared by the ESP32-C3 binary and the host simulator.
//!
//! The firmware main loop calls [`Sampler::sample`] once per period with the real esp-hal
//! I2C bus, ADC reading and button level; the host simulator calls the same function with a
//! byte-level simulated SHT40 bus. Hardware access is limited to the `embedded-hal` traits.

use crate::frame::Frame;
use crate::pins;
use crate::sht4x::{self, Sht4x};
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

/// Sampling period of the firmware main loop.
pub const SAMPLE_PERIOD_MS: u32 = 1_000;

/// Convert the ADC pin voltage (mV) to VBUS (mV) through the R10/R11 divider.
pub fn vbus_mv_from_adc_mv(adc_mv: u32) -> u32 {
    let top = pins::VBUS_DIV_TOP_OHM as u64;
    let bottom = pins::VBUS_DIV_BOTTOM_OHM as u64;
    ((adc_mv as u64 * (top + bottom) + bottom / 2) / bottom) as u32
}

/// Status LED pattern: on while a sensor fault is active, otherwise a heartbeat
/// (on for one sample, off for the next).
pub fn led_on(frame: &Frame) -> bool {
    frame.temperature_mc.is_none() || frame.seq % 2 == 0
}

pub struct Sampler {
    seq: u32,
    faults: u32,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub const fn new() -> Self {
        Sampler { seq: 0, faults: 0 }
    }

    pub fn faults(&self) -> u32 {
        self.faults
    }

    /// Take one sample. `adc_mv` is the calibrated ADC pin voltage, or `None` if the ADC
    /// read failed. A failed sensor transaction yields `NA` fields and increments the
    /// fault counter; it never reuses an old value.
    pub fn sample<I2C: I2c, D: DelayNs>(
        &mut self,
        sensor: &mut Sht4x<I2C>,
        delay: &mut D,
        adc_mv: Option<u32>,
        button_pressed: bool,
        uptime_ms: u32,
    ) -> Frame {
        self.seq = self.seq.wrapping_add(1);
        let m = match sensor.measure(delay) {
            Ok(m) => Some(m),
            Err(sht4x::Error::Bus(_)) | Err(sht4x::Error::Crc) => {
                self.faults = self.faults.saturating_add(1);
                None
            }
        };
        Frame {
            seq: self.seq,
            uptime_ms,
            temperature_mc: m.map(|m| m.temperature_mc),
            humidity_mpct: m.map(|m| m.humidity_mpct),
            vbus_mv: adc_mv.map(vbus_mv_from_adc_mv),
            button_pressed,
            sensor_faults: self.faults,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vbus_divider_scaling() {
        // 5.0 V * 47k / 147k = 1598.6 mV at the ADC pin.
        assert!((vbus_mv_from_adc_mv(1599) as i32 - 5001).abs() <= 2);
        assert_eq!(vbus_mv_from_adc_mv(0), 0);
    }
}
