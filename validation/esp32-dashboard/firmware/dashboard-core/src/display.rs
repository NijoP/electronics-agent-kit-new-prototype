//! Local dashboard on a 0.96 in 128 x 64 SSD1306 I2C OLED module (header J3).
//!
//! The initialisation sequence is the common SSD1306 128x64 internal-charge-pump sequence
//! (command values per the Solomon Systech SSD1306 datasheet command table; NOT re-verified
//! against the datasheet in this project — see READINESS.md). The display is optional: if it
//! does not acknowledge, the firmware keeps running and only the USB dashboard is used.

use crate::frame::Frame;
use core::fmt::Write;
use embedded_hal::i2c::I2c;

/// 7-bit address of common SSD1306 modules (SA0 = 0).
pub const ADDRESS: u8 = 0x3C;
pub const WIDTH: usize = 128;
pub const PAGES: usize = 8;

/// Control byte prefixes: Co = 0, D/C# = 0 (command stream) or 1 (data stream).
const CMD: u8 = 0x00;
const DATA: u8 = 0x40;

pub const INIT: [u8; 25] = [
    0xAE, // display off
    0xD5, 0x80, // clock divide / oscillator
    0xA8, 0x3F, // multiplex ratio 64
    0xD3, 0x00, // display offset 0
    0x40, // start line 0
    0x8D, 0x14, // charge pump on
    0x20, 0x00, // horizontal addressing mode
    0xA1, // segment remap (column 127 = SEG0)
    0xC8, // COM scan direction remapped
    0xDA, 0x12, // COM pins: alternative configuration
    0x81, 0xCF, // contrast
    0xD9, 0xF1, // pre-charge period
    0xDB, 0x40, // VCOMH deselect level
    0xA4, // resume to RAM content
    0xA6, // normal (not inverted)
    0xAF, // display on
];

/// Classic 5x7 glyphs, one byte per column, bit 0 = top row.
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00],
        '0' => [0x3E, 0x51, 0x49, 0x45, 0x3E],
        '1' => [0x00, 0x42, 0x7F, 0x40, 0x00],
        '2' => [0x42, 0x61, 0x51, 0x49, 0x46],
        '3' => [0x21, 0x41, 0x45, 0x4B, 0x31],
        '4' => [0x18, 0x14, 0x12, 0x7F, 0x10],
        '5' => [0x27, 0x45, 0x45, 0x45, 0x39],
        '6' => [0x3C, 0x4A, 0x49, 0x49, 0x30],
        '7' => [0x01, 0x71, 0x09, 0x05, 0x03],
        '8' => [0x36, 0x49, 0x49, 0x49, 0x36],
        '9' => [0x06, 0x49, 0x49, 0x29, 0x1E],
        '.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        '-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        '%' => [0x23, 0x13, 0x08, 0x64, 0x62],
        ':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        '#' => [0x14, 0x7F, 0x14, 0x7F, 0x14],
        'A' => [0x7E, 0x11, 0x11, 0x11, 0x7E],
        'B' => [0x7F, 0x49, 0x49, 0x49, 0x36],
        'C' => [0x3E, 0x41, 0x41, 0x41, 0x22],
        'D' => [0x7F, 0x41, 0x41, 0x22, 0x1C],
        'E' => [0x7F, 0x49, 0x49, 0x49, 0x41],
        'F' => [0x7F, 0x09, 0x09, 0x09, 0x01],
        'G' => [0x3E, 0x41, 0x49, 0x49, 0x7A],
        'H' => [0x7F, 0x08, 0x08, 0x08, 0x7F],
        'I' => [0x00, 0x41, 0x7F, 0x41, 0x00],
        'K' => [0x7F, 0x08, 0x14, 0x22, 0x41],
        'L' => [0x7F, 0x40, 0x40, 0x40, 0x40],
        'M' => [0x7F, 0x02, 0x0C, 0x02, 0x7F],
        'N' => [0x7F, 0x04, 0x08, 0x10, 0x7F],
        'O' => [0x3E, 0x41, 0x41, 0x41, 0x3E],
        'P' => [0x7F, 0x09, 0x09, 0x09, 0x06],
        'R' => [0x7F, 0x09, 0x19, 0x29, 0x46],
        'S' => [0x46, 0x49, 0x49, 0x49, 0x31],
        'T' => [0x01, 0x01, 0x7F, 0x01, 0x01],
        'U' => [0x3F, 0x40, 0x40, 0x40, 0x3F],
        'V' => [0x1F, 0x20, 0x40, 0x20, 0x1F],
        'W' => [0x3F, 0x40, 0x38, 0x40, 0x3F],
        'Y' => [0x07, 0x08, 0x70, 0x08, 0x07],
        _ => [0x02, 0x01, 0x51, 0x09, 0x06], // '?'
    }
}

/// 1-bit frame buffer in SSD1306 page layout: `buf[page * 128 + x]`, bit = y % 8.
pub struct FrameBuffer {
    pub buf: [u8; WIDTH * PAGES],
}

impl Default for FrameBuffer {
    fn default() -> Self {
        FrameBuffer {
            buf: [0; WIDTH * PAGES],
        }
    }
}

impl FrameBuffer {
    pub fn clear(&mut self) {
        self.buf = [0; WIDTH * PAGES];
    }

    pub fn pixel(&self, x: usize, y: usize) -> bool {
        x < WIDTH && y < PAGES * 8 && self.buf[(y / 8) * WIDTH + x] & (1 << (y % 8)) != 0
    }

    /// Draw text on text row `page` (0..8), 6 px per character; clipped at the right edge.
    pub fn text(&mut self, page: usize, s: &str) {
        if page >= PAGES {
            return;
        }
        for (i, c) in s.chars().enumerate() {
            let x0 = i * 6;
            if x0 + 5 > WIDTH {
                break;
            }
            for (dx, col) in glyph(c).iter().enumerate() {
                self.buf[page * WIDTH + x0 + dx] = *col;
            }
        }
    }
}

struct Line {
    b: [u8; 22],
    n: usize,
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &c in s.as_bytes() {
            if self.n < self.b.len() {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
        Ok(())
    }
}

impl Line {
    fn new() -> Line {
        Line {
            b: [b' '; 22],
            n: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.b[..self.n]).unwrap_or("?")
    }
}

/// Lay out the device-side dashboard for one telemetry frame.
pub fn render(fb: &mut FrameBuffer, f: &Frame) {
    fb.clear();
    fb.text(0, "EAK ENV DASHBOARD");
    let mut l = Line::new();
    match f.temperature_mc {
        Some(t) => {
            let sign = if t < 0 { "-" } else { "" };
            let a = t.unsigned_abs();
            let _ = write!(l, "T    {sign}{}.{:02} C", a / 1000, (a % 1000) / 10);
        }
        None => {
            let _ = write!(l, "T    --.-- C");
        }
    }
    fb.text(2, l.as_str());
    let mut l = Line::new();
    match f.humidity_mpct {
        Some(h) => {
            let _ = write!(l, "RH   {}.{} %", h / 1000, (h % 1000) / 100);
        }
        None => {
            let _ = write!(l, "RH   --.- %");
        }
    }
    fb.text(3, l.as_str());
    let mut l = Line::new();
    match f.vbus_mv {
        Some(v) => {
            let _ = write!(l, "VBUS {}.{:02} V", v / 1000, (v % 1000) / 10);
        }
        None => {
            let _ = write!(l, "VBUS -.-- V");
        }
    }
    fb.text(4, l.as_str());
    let mut l = Line::new();
    let status = if f.temperature_mc.is_none() {
        "SENSOR FAULT"
    } else if f.button_pressed {
        "BUTTON"
    } else {
        "OK"
    };
    let _ = write!(l, "#{} {}", f.seq, status);
    fb.text(6, l.as_str());
    let mut l = Line::new();
    let _ = write!(l, "FAULTS {}", f.sensor_faults);
    fb.text(7, l.as_str());
}

pub struct Ssd1306<'a, I2C> {
    i2c: &'a mut I2C,
}

impl<'a, I2C: I2c> Ssd1306<'a, I2C> {
    pub fn new(i2c: &'a mut I2C) -> Self {
        Ssd1306 { i2c }
    }

    pub fn init(&mut self) -> Result<(), I2C::Error> {
        let mut b = [0u8; INIT.len() + 1];
        b[0] = CMD;
        b[1..].copy_from_slice(&INIT);
        self.i2c.write(ADDRESS, &b)
    }

    /// Send the whole frame buffer (column 0..127, page 0..7) in 16-byte data chunks.
    pub fn flush(&mut self, fb: &FrameBuffer) -> Result<(), I2C::Error> {
        self.i2c
            .write(ADDRESS, &[CMD, 0x21, 0x00, 0x7F, 0x22, 0x00, 0x07])?;
        for chunk in fb.buf.chunks(16) {
            let mut b = [0u8; 17];
            b[0] = DATA;
            b[1..1 + chunk.len()].copy_from_slice(chunk);
            self.i2c.write(ADDRESS, &b[..1 + chunk.len()])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    #[derive(Default)]
    struct Recorder {
        writes: Vec<(u8, Vec<u8>)>,
    }
    impl ErrorType for Recorder {
        type Error = ErrorKind;
    }
    impl I2c for Recorder {
        fn transaction(&mut self, a: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
            for op in ops {
                if let Operation::Write(w) = op {
                    self.writes.push((a, w.to_vec()));
                }
            }
            Ok(())
        }
    }

    fn row_bits(fb: &FrameBuffer, page: usize, x0: usize) -> [u8; 5] {
        let mut out = [0; 5];
        out.copy_from_slice(&fb.buf[page * WIDTH + x0..page * WIDTH + x0 + 5]);
        out
    }

    #[test]
    fn init_and_flush_byte_stream() {
        let mut r = Recorder::default();
        let mut d = Ssd1306::new(&mut r);
        d.init().unwrap();
        let mut fb = FrameBuffer::default();
        fb.text(0, "8");
        d.flush(&fb).unwrap();
        assert_eq!(r.writes[0].0, ADDRESS);
        assert_eq!(r.writes[0].1[0], 0x00);
        assert_eq!(&r.writes[0].1[1..], &INIT);
        assert_eq!(
            r.writes[1].1,
            vec![0x00, 0x21, 0x00, 0x7F, 0x22, 0x00, 0x07]
        );
        // 1024 bytes of data in 64 chunks, each prefixed with the data control byte.
        assert_eq!(r.writes.len(), 2 + 64);
        assert!(r.writes[2..]
            .iter()
            .all(|(_, w)| w[0] == 0x40 && w.len() == 17));
        assert_eq!(&r.writes[2].1[1..6], &[0x36, 0x49, 0x49, 0x49, 0x36]);
    }

    #[test]
    fn renders_values_faults_and_negative_temperatures() {
        let mut fb = FrameBuffer::default();
        let f = Frame {
            seq: 7,
            temperature_mc: Some(-5_250),
            humidity_mpct: Some(48_270),
            vbus_mv: Some(5_020),
            ..Default::default()
        };
        render(&mut fb, &f);
        // "T    -5.25 C": char 5 is '-', char 6 is '5'.
        assert_eq!(row_bits(&fb, 2, 5 * 6), glyph('-'));
        assert_eq!(row_bits(&fb, 2, 6 * 6), glyph('5'));
        // "RH   48.2 %": char 5 is '4'.
        assert_eq!(row_bits(&fb, 3, 5 * 6), glyph('4'));
        // Status row reads "#7 OK".
        assert_eq!(row_bits(&fb, 6, 3 * 6), glyph('O'));
        render(
            &mut fb,
            &Frame {
                seq: 8,
                ..Default::default()
            },
        );
        // Missing temperature is shown as dashes and a fault, never as a number.
        assert_eq!(row_bits(&fb, 2, 5 * 6), glyph('-'));
        assert_eq!(row_bits(&fb, 6, 3 * 6), glyph('S'));
        assert!(fb.pixel(1, 0)); // header drawn
    }

    #[test]
    fn text_is_clipped_at_the_right_edge() {
        let mut fb = FrameBuffer::default();
        fb.text(1, "0123456789012345678901234");
        assert!(fb.buf[WIDTH..2 * WIDTH][126..].iter().all(|&b| b == 0));
    }
}
