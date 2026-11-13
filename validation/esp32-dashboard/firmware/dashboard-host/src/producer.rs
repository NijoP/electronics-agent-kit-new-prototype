//! Telemetry producers: the simulated firmware and the serial reader.

use crate::sim::{adc_pin_mv, Environment, Faults, Rng, SimClock, SimDelay, SimSht40};
use crate::{ingest_bytes, LineAssembler, Shared};
use dashboard_core::app::{Sampler, SAMPLE_PERIOD_MS};
use dashboard_core::frame::MAX_FRAME_LEN;
use dashboard_core::sht4x::Sht4x;
use std::io::Read;
use std::time::Duration;

/// Ground-truth environment used by the simulator (exposed for tests).
pub fn sim_environment() -> Environment {
    Environment {
        base_temp_c: 23.5,
        base_rh: 48.0,
        vbus_v: 5.02,
    }
}

/// Simulated firmware: identical sampling + encoding path to `esp32c3/src/main.rs`.
pub fn run_simulation(state: Shared, seed: u64, faults: bool, frames: Option<u64>, period_ms: u64) {
    let clock = SimClock::default();
    let env = sim_environment();
    let mut dev = SimSht40::new(&clock, env.clone(), seed);
    if faults {
        dev.faults = Faults {
            nack: 0.05,
            crc_corruption: 0.05,
        };
    }
    let mut sensor = Sht4x::new(dev);
    let mut delay = SimDelay(&clock);
    let mut sampler = Sampler::new();
    let mut rng = Rng::new(seed ^ 0x9E37_79B9);
    let mut asm = LineAssembler::default();
    let mut n = 0u64;
    loop {
        let uptime = (clock.now_us() / 1000) as u32;
        let button = (n / 10) % 6 == 5; // pressed for 10 s every minute
        let frame = sampler.sample(&mut sensor, &mut delay, Some(adc_pin_mv(&env, &mut rng)), button, uptime);
        let mut buf = [0u8; MAX_FRAME_LEN];
        let len = frame.encode(&mut buf).expect("frame fits");
        let mut wire = buf[..len].to_vec();
        if faults && rng.chance(0.03) {
            wire[len / 2] ^= 0x04; // simulated line noise on the USB/serial link
        }
        ingest_bytes(&state, &mut asm, &wire);
        n += 1;
        if frames.is_some_and(|f| n >= f) {
            return;
        }
        // Advance simulated time to the next sample period, then pace in real time.
        let used = clock.now_us() % (SAMPLE_PERIOD_MS as u64 * 1000);
        clock.advance_us(SAMPLE_PERIOD_MS as u64 * 1000 - used);
        std::thread::sleep(Duration::from_millis(period_ms));
    }
}

pub fn run_serial(state: Shared, path: String) {
    let mut asm = LineAssembler::default();
    loop {
        match std::fs::File::open(&path) {
            Ok(mut f) => {
                let mut buf = [0u8; 256];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) => std::thread::sleep(Duration::from_millis(20)),
                        Ok(n) => ingest_bytes(&state, &mut asm, &buf[..n]),
                        Err(e) => {
                            state.lock().unwrap().last_error = Some(format!("serial read: {e}"));
                            break;
                        }
                    }
                }
            }
            Err(e) => state.lock().unwrap().last_error = Some(format!("open {path}: {e}")),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

