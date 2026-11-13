//! CRC-8 used by the SHT4x (and reused for telemetry frames).
//!
//! Parameters from Sensirion "Datasheet SHT4x" v7.3 (June 2026), section 4.4 "Checksum
//! Calculation": polynomial 0x31 (x^8 + x^5 + x^4 + 1), initialisation 0xFF, no input/output
//! reflection, final XOR 0x00, example CRC(0xBEEF) = 0x92.

pub const POLY: u8 = 0x31;
pub const INIT: u8 = 0xFF;

pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = INIT;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datasheet_example_vector() {
        assert_eq!(crc8(&[0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn detects_single_bit_flips() {
        let base = [0x66u8, 0x9A];
        let c = crc8(&base);
        for byte in 0..2 {
            for bit in 0..8 {
                let mut d = base;
                d[byte] ^= 1 << bit;
                assert_ne!(crc8(&d), c);
            }
        }
    }
}
