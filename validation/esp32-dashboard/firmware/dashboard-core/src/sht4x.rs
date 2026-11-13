//! Sensirion SHT4x (SHT40-AD1B) temperature / humidity driver over `embedded-hal` 1.0 I2C.
//!
//! Protocol facts (Sensirion "Datasheet SHT4x" v7.3, June 2026):
//! * I2C address 0x44 for SHT40-AD1B (Device Overview, Quick Start Guide).
//! * 0xFD = measure T & RH with high precision; response 6 bytes
//!   `[T msb, T lsb, CRC, RH msb, RH lsb, CRC]` (Table 8).
//! * High-precision measurement duration: typ 6.9 ms, max 8.3 ms (section 3.1, tMEAS,h).
//! * 0x94 = soft reset, 0x89 = read serial number (two 16-bit words, each + CRC) (Table 8).
//! * T = -45 + 175 * S_T / 65535 [°C]; RH = -6 + 125 * S_RH / 65535 [%RH], cropped to
//!   0..100 %RH (section 4.6 / Quick Start pseudo code).
//! * Supply 1.08 .. 3.6 V (section 2.3 / 3); the board supplies it from +3V3.

use crate::crc::crc8;
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

pub const ADDRESS: u8 = 0x44;
pub const CMD_MEASURE_HIGH: u8 = 0xFD;
pub const CMD_SOFT_RESET: u8 = 0x94;
pub const CMD_READ_SERIAL: u8 = 0x89;
/// Wait after the measure command: datasheet max 8.3 ms, rounded up with margin.
pub const MEASURE_WAIT_US: u32 = 10_000;
/// Soft reset time: datasheet max 1 ms.
pub const RESET_WAIT_US: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// The bus transaction failed (NACK, arbitration, timeout ...).
    Bus(E),
    /// A received word failed its CRC check: data is discarded, never reported.
    Crc,
}

/// One measurement in fixed-point units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Temperature in milli-degrees Celsius.
    pub temperature_mc: i32,
    /// Relative humidity in milli-percent (0 ..= 100_000).
    pub humidity_mpct: u32,
}

/// Convert raw temperature ticks to milli-°C (exact integer form of the datasheet formula).
pub fn ticks_to_millicelsius(ticks: u16) -> i32 {
    -45_000 + ((175_000i64 * ticks as i64) / 65_535) as i32
}

/// Convert raw humidity ticks to milli-%RH, cropped to the physical range.
pub fn ticks_to_millipercent(ticks: u16) -> u32 {
    let v = -6_000i64 + (125_000i64 * ticks as i64) / 65_535;
    v.clamp(0, 100_000) as u32
}

/// Inverse of [`ticks_to_millicelsius`] (used by the simulator and tests).
pub fn millicelsius_to_ticks(mc: i32) -> u16 {
    let t = ((mc as i64 + 45_000) * 65_535 + 87_499) / 175_000;
    t.clamp(0, 65_535) as u16
}

/// Inverse of [`ticks_to_millipercent`] (used by the simulator and tests).
pub fn millipercent_to_ticks(mp: u32) -> u16 {
    let t = ((mp as i64 + 6_000) * 65_535 + 62_499) / 125_000;
    t.clamp(0, 65_535) as u16
}

/// Verify and extract the two CRC-protected words of a 6-byte response.
pub fn parse_words(buf: &[u8; 6]) -> Option<(u16, u16)> {
    if crc8(&buf[0..2]) != buf[2] || crc8(&buf[3..5]) != buf[5] {
        return None;
    }
    Some((
        u16::from_be_bytes([buf[0], buf[1]]),
        u16::from_be_bytes([buf[3], buf[4]]),
    ))
}

pub struct Sht4x<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Sht4x<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Sht4x {
            i2c,
            address: ADDRESS,
        }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }

    /// The shared bus (the SSD1306 display sits on the same I2C lines).
    pub fn bus(&mut self) -> &mut I2C {
        &mut self.i2c
    }

    pub fn soft_reset(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[CMD_SOFT_RESET])
            .map_err(Error::Bus)?;
        delay.delay_us(RESET_WAIT_US);
        Ok(())
    }

    pub fn serial_number(&mut self, delay: &mut impl DelayNs) -> Result<u32, Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[CMD_READ_SERIAL])
            .map_err(Error::Bus)?;
        delay.delay_us(RESET_WAIT_US);
        let mut buf = [0u8; 6];
        self.i2c.read(self.address, &mut buf).map_err(Error::Bus)?;
        let (hi, lo) = parse_words(&buf).ok_or(Error::Crc)?;
        Ok(((hi as u32) << 16) | lo as u32)
    }

    pub fn measure(&mut self, delay: &mut impl DelayNs) -> Result<Measurement, Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[CMD_MEASURE_HIGH])
            .map_err(Error::Bus)?;
        delay.delay_us(MEASURE_WAIT_US);
        let mut buf = [0u8; 6];
        self.i2c.read(self.address, &mut buf).map_err(Error::Bus)?;
        let (t, rh) = parse_words(&buf).ok_or(Error::Crc)?;
        Ok(Measurement {
            temperature_mc: ticks_to_millicelsius(t),
            humidity_mpct: ticks_to_millipercent(rh),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_endpoints_match_datasheet_formula() {
        assert_eq!(ticks_to_millicelsius(0), -45_000);
        assert_eq!(ticks_to_millicelsius(65_535), 130_000);
        assert_eq!(ticks_to_millipercent(0), 0); // -6 %RH cropped to 0
        assert_eq!(ticks_to_millipercent(65_535), 100_000); // 119 %RH cropped to 100
                                                            // 25 °C: (25 + 45) / 175 * 65535 = 26214
        assert!((ticks_to_millicelsius(26_214) - 25_000).abs() <= 3);
    }

    #[test]
    fn inverse_conversions_round_trip_within_one_tick() {
        for mc in [-40_000, -10_123, 0, 21_500, 25_000, 85_000, 125_000] {
            let back = ticks_to_millicelsius(millicelsius_to_ticks(mc));
            assert!((back - mc).abs() <= 3, "{mc} -> {back}");
        }
        for mp in [0u32, 1_000, 45_250, 99_000] {
            let back = ticks_to_millipercent(millipercent_to_ticks(mp));
            assert!((back as i64 - mp as i64).abs() <= 2, "{mp} -> {back}");
        }
    }

    #[test]
    fn corrupted_word_is_rejected() {
        let mut buf = [0x66, 0x66, 0, 0x80, 0x00, 0];
        buf[2] = crc8(&buf[0..2]);
        buf[5] = crc8(&buf[3..5]);
        assert!(parse_words(&buf).is_some());
        buf[4] ^= 0x01;
        assert!(parse_words(&buf).is_none());
    }
}
