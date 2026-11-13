//! Host side of the ESP32-C3 dashboard: telemetry ingestion (serial or simulated
//! firmware), the dashboard state and a small HTTP server.

pub mod producer;
pub mod server;
pub mod sim;

use dashboard_core::frame::{Frame, MAX_FRAME_LEN};
use dashboard_core::model::Dashboard;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Where the telemetry comes from (shown on the dashboard so simulated data is never
/// mistaken for hardware data).
#[derive(Clone, Debug)]
pub enum Source {
    Simulated { seed: u64, faults: bool },
    Serial { path: String },
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Simulated { seed, faults } => format!(
                "SIMULATED firmware data path (dashboard-core sampler + byte-level SHT40 model, seed {seed}{})",
                if *faults { ", fault injection on" } else { "" }
            ),
            Source::Serial { path } => format!("SERIAL device {path} (ESP32-C3 USB Serial/JTAG)"),
        }
    }
}

pub struct State {
    pub dashboard: Dashboard,
    pub source: Source,
    pub started: Instant,
    pub last_error: Option<String>,
}

pub type Shared = Arc<Mutex<State>>;

pub fn shared(source: Source) -> Shared {
    Arc::new(Mutex::new(State {
        dashboard: Dashboard::default(),
        source,
        started: Instant::now(),
        last_error: None,
    }))
}

/// Milliseconds since the dashboard started (monotonic).
pub fn host_ms(s: &State) -> u64 {
    s.started.elapsed().as_millis() as u64
}

pub fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Splits a byte stream into lines; overlong lines are discarded and counted as rejected.
#[derive(Default)]
pub struct LineAssembler {
    buf: Vec<u8>,
    overflow: bool,
}

impl LineAssembler {
    /// Feed bytes; calls `on_line` for each complete line (`Err` for an overlong one).
    pub fn feed(&mut self, bytes: &[u8], mut on_line: impl FnMut(Result<&str, ()>)) {
        for &b in bytes {
            if b == b'\n' {
                if self.overflow {
                    on_line(Err(()));
                } else if !self.buf.is_empty() {
                    match std::str::from_utf8(&self.buf) {
                        Ok(s) => on_line(Ok(s)),
                        Err(_) => on_line(Err(())),
                    }
                }
                self.buf.clear();
                self.overflow = false;
            } else if self.buf.len() >= MAX_FRAME_LEN {
                self.overflow = true;
            } else {
                self.buf.push(b);
            }
        }
    }
}

/// Feed raw telemetry bytes into the dashboard.
pub fn ingest_bytes(state: &Shared, asm: &mut LineAssembler, bytes: &[u8]) {
    let mut s = state.lock().expect("state lock");
    let now = host_ms(&s);
    asm.feed(bytes, |line| match line.map(Frame::parse) {
        Ok(Ok(f)) => s.dashboard.ingest(f, now),
        Ok(Err(e)) => {
            s.dashboard.reject();
            s.last_error = Some(format!("{e:?}"));
        }
        Err(()) => {
            s.dashboard.reject();
            s.last_error = Some("line too long or not UTF-8".into());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembler_splits_lines_and_rejects_overlong_ones() {
        let mut a = LineAssembler::default();
        let mut got = vec![];
        a.feed(b"ab\r\ncd", |l| got.push(l.map(String::from)));
        a.feed(b"\n", |l| got.push(l.map(String::from)));
        a.feed(&[b'x'; 200], |l| got.push(l.map(String::from)));
        a.feed(b"\nok\n", |l| got.push(l.map(String::from)));
        assert_eq!(
            got,
            vec![Ok("ab\r".into()), Ok("cd".into()), Err(()), Ok("ok".into())]
        );
    }
}
