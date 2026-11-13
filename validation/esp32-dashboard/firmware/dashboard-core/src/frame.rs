//! Telemetry frame: the wire protocol between the firmware and the dashboard.
//!
//! One ASCII line per sample, sent over the ESP32-C3 USB Serial/JTAG CDC port:
//!
//! ```text
//! $EAKD,1,<seq>,<uptime_ms>,<temp_mC|NA>,<rh_m%|NA>,<vbus_mV|NA>,<button 0|1>,<sensor_faults>*<CRC8 hex>\r\n
//! ```
//!
//! The CRC-8 (same parameters as the SHT4x) covers every byte between `$` and `*`.
//! Fields that could not be measured are sent as `NA`; they are never replaced by a default
//! value, so the dashboard can show "no data" instead of a fabricated reading.

use crate::crc::crc8;
use core::fmt::{self, Write};

pub const PROTOCOL_VERSION: u8 = 1;
pub const TAG: &str = "EAKD";
/// Longest possible frame including CR LF.
pub const MAX_FRAME_LEN: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Frame {
    pub seq: u32,
    pub uptime_ms: u32,
    pub temperature_mc: Option<i32>,
    pub humidity_mpct: Option<u32>,
    pub vbus_mv: Option<u32>,
    pub button_pressed: bool,
    /// Cumulative count of failed sensor transactions (bus errors + CRC errors).
    pub sensor_faults: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    MissingStart,
    MissingChecksum,
    BadChecksumField,
    ChecksumMismatch { expected: u8, actual: u8 },
    WrongTag,
    UnsupportedVersion,
    FieldCount,
    BadField(usize),
    OutOfRange(usize),
}

struct Buf<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl Write for Buf<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let b = s.as_bytes();
        if self.len + b.len() > self.out.len() {
            return Err(fmt::Error);
        }
        self.out[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}

fn opt<T: fmt::Display>(w: &mut Buf<'_>, v: Option<T>) -> fmt::Result {
    match v {
        Some(x) => write!(w, ",{x}"),
        None => w.write_str(",NA"),
    }
}

impl Frame {
    /// Encode into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, fmt::Error> {
        let mut w = Buf { out, len: 0 };
        w.write_str("$")?;
        write!(
            w,
            "{TAG},{PROTOCOL_VERSION},{},{}",
            self.seq, self.uptime_ms
        )?;
        opt(&mut w, self.temperature_mc)?;
        opt(&mut w, self.humidity_mpct)?;
        opt(&mut w, self.vbus_mv)?;
        write!(
            w,
            ",{},{}",
            u8::from(self.button_pressed),
            self.sensor_faults
        )?;
        let crc = crc8(&w.out[1..w.len]);
        write!(w, "*{crc:02X}\r\n")?;
        Ok(w.len)
    }

    /// Parse one line (trailing CR/LF optional).
    pub fn parse(line: &str) -> Result<Frame, ParseError> {
        let line = line.trim_end_matches(['\r', '\n']);
        let body = line.strip_prefix('$').ok_or(ParseError::MissingStart)?;
        let (payload, crc_hex) = body.rsplit_once('*').ok_or(ParseError::MissingChecksum)?;
        if crc_hex.len() != 2 {
            return Err(ParseError::BadChecksumField);
        }
        let expected = u8::from_str_radix(crc_hex, 16).map_err(|_| ParseError::BadChecksumField)?;
        let actual = crc8(payload.as_bytes());
        if expected != actual {
            return Err(ParseError::ChecksumMismatch { expected, actual });
        }
        let mut it = payload.split(',');
        let mut fields: [&str; 9] = [""; 9];
        let mut n = 0;
        for f in it.by_ref() {
            if n == fields.len() {
                return Err(ParseError::FieldCount);
            }
            fields[n] = f;
            n += 1;
        }
        if n != 9 {
            return Err(ParseError::FieldCount);
        }
        if fields[0] != TAG {
            return Err(ParseError::WrongTag);
        }
        if fields[1] != "1" {
            return Err(ParseError::UnsupportedVersion);
        }
        fn num<T: core::str::FromStr>(s: &str, i: usize) -> Result<T, ParseError> {
            s.parse().map_err(|_| ParseError::BadField(i))
        }
        fn optnum<T: core::str::FromStr>(s: &str, i: usize) -> Result<Option<T>, ParseError> {
            if s == "NA" {
                Ok(None)
            } else {
                num(s, i).map(Some)
            }
        }
        let f = Frame {
            seq: num(fields[2], 2)?,
            uptime_ms: num(fields[3], 3)?,
            temperature_mc: optnum(fields[4], 4)?,
            humidity_mpct: optnum(fields[5], 5)?,
            vbus_mv: optnum(fields[6], 6)?,
            button_pressed: match fields[7] {
                "0" => false,
                "1" => true,
                _ => return Err(ParseError::BadField(7)),
            },
            sensor_faults: num(fields[8], 8)?,
        };
        // Physical plausibility: SHT4x range -40..125 °C (datasheet Table 6), RH 0..100 %.
        if let Some(t) = f.temperature_mc {
            if !(-45_000..=130_000).contains(&t) {
                return Err(ParseError::OutOfRange(4));
            }
        }
        if let Some(h) = f.humidity_mpct {
            if h > 100_000 {
                return Err(ParseError::OutOfRange(5));
            }
        }
        if let Some(v) = f.vbus_mv {
            if v > 20_000 {
                return Err(ParseError::OutOfRange(6));
            }
        }
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(f: &Frame) -> std::string::String {
        let mut b = [0u8; MAX_FRAME_LEN];
        let n = f.encode(&mut b).unwrap();
        std::string::String::from_utf8(b[..n].to_vec()).unwrap()
    }

    #[test]
    fn round_trip_full_and_missing_fields() {
        let full = Frame {
            seq: 42,
            uptime_ms: 123_456,
            temperature_mc: Some(-12_345),
            humidity_mpct: Some(45_678),
            vbus_mv: Some(5_012),
            button_pressed: true,
            sensor_faults: 3,
        };
        let s = enc(&full);
        assert!(s.starts_with("$EAKD,1,42,123456,-12345,45678,5012,1,3*"));
        assert!(s.ends_with("\r\n"));
        assert_eq!(Frame::parse(&s), Ok(full));
        let missing = Frame {
            temperature_mc: None,
            humidity_mpct: None,
            vbus_mv: None,
            ..full
        };
        assert_eq!(Frame::parse(&enc(&missing)), Ok(missing));
    }

    #[test]
    fn worst_case_frame_fits_buffer() {
        let f = Frame {
            seq: u32::MAX,
            uptime_ms: u32::MAX,
            temperature_mc: Some(-45_000),
            humidity_mpct: Some(100_000),
            vbus_mv: Some(20_000),
            button_pressed: true,
            sensor_faults: u32::MAX,
        };
        assert!(enc(&f).len() <= MAX_FRAME_LEN);
    }

    #[test]
    fn corruption_and_garbage_are_rejected() {
        let s = enc(&Frame {
            seq: 1,
            temperature_mc: Some(21_000),
            ..Default::default()
        });
        let bad = s.replace("21000", "91000");
        assert!(matches!(
            Frame::parse(&bad),
            Err(ParseError::ChecksumMismatch { .. })
        ));
        assert_eq!(Frame::parse("hello"), Err(ParseError::MissingStart));
        assert_eq!(Frame::parse("$EAKD,1,2"), Err(ParseError::MissingChecksum));
        assert_eq!(Frame::parse(""), Err(ParseError::MissingStart));
    }

    #[test]
    fn implausible_values_are_rejected_even_with_valid_crc() {
        let s = enc(&Frame {
            humidity_mpct: Some(150_000),
            ..Default::default()
        });
        assert_eq!(Frame::parse(&s), Err(ParseError::OutOfRange(5)));
    }
}
