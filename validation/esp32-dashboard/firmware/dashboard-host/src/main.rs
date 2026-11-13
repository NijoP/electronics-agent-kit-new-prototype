//! `dashboard`: live web dashboard for the ESP32-C3 environment board.
//!
//! ```text
//! dashboard --simulate [--seed N] [--faults] [--bind 127.0.0.1:8080] [--frames N] [--period-ms 1000]
//! dashboard --serial <device>   (e.g. \\.\COM5 on Windows, /dev/ttyACM0 on Linux)
//! ```
//!
//! `--simulate` runs the firmware's own sampling code (`dashboard_core::app::Sampler`) against
//! a byte-level SHT40 model and streams the encoded telemetry through the same parser the
//! serial path uses. The page and API label simulated data as SIMULATED.

use dashboard_core::app::SAMPLE_PERIOD_MS;
use dashboard_host::producer::{run_serial, run_simulation};
use dashboard_host::{server, shared, Source};
use std::net::TcpListener;

struct Args {
    serial: Option<String>,
    seed: u64,
    faults: bool,
    bind: String,
    frames: Option<u64>,
    period_ms: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        serial: None,
        seed: 1,
        faults: false,
        bind: "127.0.0.1:8080".into(),
        frames: None,
        period_ms: SAMPLE_PERIOD_MS as u64,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().ok_or(format!("{x} needs a value"));
        match x.as_str() {
            "--simulate" => {}
            "--serial" => a.serial = Some(val()?),
            "--seed" => a.seed = val()?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--faults" => a.faults = true,
            "--bind" => a.bind = val()?,
            "--frames" => a.frames = Some(val()?.parse().map_err(|e| format!("--frames: {e}"))?),
            "--period-ms" => {
                a.period_ms = val()?.parse().map_err(|e| format!("--period-ms: {e}"))?
            }
            "-h" | "--help" => {
                println!("dashboard [--simulate [--seed N] [--faults] [--frames N] [--period-ms MS]] [--serial DEVICE] [--bind ADDR]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(a)
}

fn main() {
    let a = parse_args().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2)
    });
    let source = match &a.serial {
        Some(p) => Source::Serial { path: p.clone() },
        None => Source::Simulated {
            seed: a.seed,
            faults: a.faults,
        },
    };
    let state = shared(source);
    let listener = TcpListener::bind(&a.bind).unwrap_or_else(|e| {
        eprintln!("error: bind {}: {e}", a.bind);
        std::process::exit(2)
    });
    println!(
        "dashboard listening on http://{} ({})",
        listener.local_addr().unwrap(),
        state.lock().unwrap().source.label()
    );
    let st = state.clone();
    let producer = std::thread::spawn(move || match a.serial {
        Some(p) => run_serial(st, p),
        None => run_simulation(st, a.seed, a.faults, a.frames, a.period_ms),
    });
    if a.frames.is_some() {
        // Bounded run: serve while producing, then print the final state and exit.
        let st = state.clone();
        std::thread::spawn(move || server::serve(listener, st));
        producer.join().unwrap();
        println!(
            "{}",
            serde_json::to_string_pretty(&server::state_json(&state)).unwrap()
        );
    } else {
        server::serve(listener, state);
    }
}
