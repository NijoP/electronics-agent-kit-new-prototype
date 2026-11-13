//! SIMULATED hardware for host runs: an SHT40 that speaks the real I2C byte protocol, a
//! delay that advances simulated time, and a VBUS/ADC model.
//!
//! Evidence produced with this module is simulation evidence, not physical validation.

use dashboard_core::crc::crc8;
use dashboard_core::sht4x;
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::{self, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};

/// Deterministic xorshift PRNG (no external crates, reproducible runs).
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    /// Uniform in [-1, 1].
    pub fn signed_unit(&mut self) -> f64 {
        self.next_u32() as f64 / u32::MAX as f64 * 2.0 - 1.0
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u32() as f64 / u32::MAX as f64) < p
    }
}

/// The simulated room: ground truth the dashboard result is compared against.
#[derive(Clone)]
pub struct Environment {
    pub base_temp_c: f64,
    pub base_rh: f64,
    pub vbus_v: f64,
}

impl Environment {
    /// Ground truth at simulated time `t_ms` (slow drift, no noise).
    pub fn temperature_c(&self, t_ms: u64) -> f64 {
        self.base_temp_c + 2.0 * ((t_ms as f64) / 60_000.0 * std::f64::consts::TAU).sin()
    }
    pub fn humidity(&self, t_ms: u64) -> f64 {
        self.base_rh - 5.0 * ((t_ms as f64) / 60_000.0 * std::f64::consts::TAU).sin()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimError(pub ErrorKind);

impl i2c::Error for SimError {
    fn kind(&self) -> ErrorKind {
        self.0
    }
}

/// Fault injection rates (per transaction).
#[derive(Clone, Copy, Default)]
pub struct Faults {
    pub nack: f64,
    pub crc_corruption: f64,
}

pub struct SimSht40<'a> {
    clock: &'a SimClock,
    env: Environment,
    pending: Option<(u64, [u8; 6])>,
    serial: u32,
    pub faults: Faults,
    rng: Rng,
    pub transactions: u64,
}

/// Shared simulated time in microseconds.
#[derive(Default)]
pub struct SimClock {
    us: std::cell::Cell<u64>,
}

impl SimClock {
    pub fn now_us(&self) -> u64 {
        self.us.get()
    }
    pub fn advance_us(&self, d: u64) {
        self.us.set(self.us.get() + d);
    }
}

pub struct SimDelay<'a>(pub &'a SimClock);

impl DelayNs for SimDelay<'_> {
    fn delay_ns(&mut self, ns: u32) {
        self.0.advance_us(ns.div_ceil(1000) as u64);
    }
}

fn words(a: u16, b: u16) -> [u8; 6] {
    let a = a.to_be_bytes();
    let b = b.to_be_bytes();
    [a[0], a[1], crc8(&a), b[0], b[1], crc8(&b)]
}

impl<'a> SimSht40<'a> {
    pub fn new(clock: &'a SimClock, env: Environment, seed: u64) -> Self {
        SimSht40 {
            clock,
            env,
            pending: None,
            serial: 0x1234_5678,
            faults: Faults::default(),
            rng: Rng::new(seed),
            transactions: 0,
        }
    }

    pub fn env(&self) -> &Environment {
        &self.env
    }

    fn command(&mut self, cmd: u8) -> Result<(), SimError> {
        let now = self.clock.now_us();
        match cmd {
            sht4x::CMD_MEASURE_HIGH => {
                let t_ms = now / 1000;
                // Sensor noise of the order of the SHT40 repeatability (0.04 C / 0.08 %RH).
                let t = self.env.temperature_c(t_ms) + 0.04 * self.rng.signed_unit();
                let h = self.env.humidity(t_ms) + 0.08 * self.rng.signed_unit();
                let tt = sht4x::millicelsius_to_ticks((t * 1000.0).round() as i32);
                let ht = sht4x::millipercent_to_ticks((h * 1000.0).round().max(0.0) as u32);
                // Data becomes readable after the typical 6.9 ms conversion time.
                self.pending = Some((now + 6_900, words(tt, ht)));
                Ok(())
            }
            sht4x::CMD_READ_SERIAL => {
                self.pending = Some((now, words((self.serial >> 16) as u16, self.serial as u16)));
                Ok(())
            }
            sht4x::CMD_SOFT_RESET => {
                self.pending = None;
                Ok(())
            }
            _ => Err(SimError(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data))),
        }
    }

    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), SimError> {
        let now = self.clock.now_us();
        match self.pending {
            // Datasheet: the sensor NACKs the read header while it is still measuring.
            Some((ready, _)) if now < ready => {
                Err(SimError(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address)))
            }
            Some((_, data)) => {
                let n = buf.len().min(6);
                buf[..n].copy_from_slice(&data[..n]);
                if self.rng.chance(self.faults.crc_corruption) {
                    buf[1] ^= 0x10;
                }
                self.pending = None;
                Ok(())
            }
            None => Err(SimError(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address))),
        }
    }
}

impl ErrorType for SimSht40<'_> {
    type Error = SimError;
}

impl I2c for SimSht40<'_> {
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), SimError> {
        self.transactions += 1;
        if address != sht4x::ADDRESS || self.rng.chance(self.faults.nack) {
            return Err(SimError(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address)));
        }
        for op in operations {
            match op {
                Operation::Write(bytes) => {
                    if bytes.len() != 1 {
                        return Err(SimError(ErrorKind::Other));
                    }
                    self.command(bytes[0])?;
                }
                Operation::Read(buf) => self.read_into(buf)?,
            }
        }
        Ok(())
    }
}

/// VBUS -> divider -> ADC pin model (12-bit ADC with ~1 mV resolution after calibration).
pub fn adc_pin_mv(env: &Environment, rng: &mut Rng) -> u32 {
    let v = env.vbus_v + 0.02 * rng.signed_unit();
    let pin = v * dashboard_core::pins::VBUS_DIV_BOTTOM_OHM as f64
        / (dashboard_core::pins::VBUS_DIV_TOP_OHM + dashboard_core::pins::VBUS_DIV_BOTTOM_OHM) as f64;
    (pin * 1000.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashboard_core::sht4x::Sht4x;

    #[test]
    fn simulated_sensor_speaks_the_datasheet_protocol() {
        let clock = SimClock::default();
        let env = Environment { base_temp_c: 22.0, base_rh: 45.0, vbus_v: 5.0 };
        let mut s = Sht4x::new(SimSht40::new(&clock, env, 7));
        let mut d = SimDelay(&clock);
        let m = s.measure(&mut d).unwrap();
        assert!((m.temperature_mc - 22_000).abs() < 100, "{m:?}");
        assert!((m.humidity_mpct as i64 - 45_000).abs() < 200, "{m:?}");
        assert_eq!(s.serial_number(&mut d).unwrap(), 0x1234_5678);
    }

    #[test]
    fn reading_before_conversion_is_nacked() {
        let clock = SimClock::default();
        let env = Environment { base_temp_c: 22.0, base_rh: 45.0, vbus_v: 5.0 };
        let mut dev = SimSht40::new(&clock, env, 1);
        dev.write(sht4x::ADDRESS, &[sht4x::CMD_MEASURE_HIGH]).unwrap();
        let mut b = [0u8; 6];
        assert!(dev.read(sht4x::ADDRESS, &mut b).is_err());
        clock.advance_us(8_300);
        assert!(dev.read(sht4x::ADDRESS, &mut b).is_ok());
    }
}
