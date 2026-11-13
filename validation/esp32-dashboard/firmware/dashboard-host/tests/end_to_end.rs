//! SIMULATED end-to-end test: firmware sampling code -> SHT40 byte protocol -> telemetry
//! frames -> line parser -> dashboard model -> HTTP API, compared against the simulator's
//! ground truth. This is interface-level evidence, not a hardware measurement.

use dashboard_host::producer::{run_simulation, sim_environment};
use dashboard_host::{server, shared, Source};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn get(addr: std::net::SocketAddr, path: &str) -> (String, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    (head.to_string(), body.to_string())
}

fn start(seed: u64, faults: bool, frames: u64) -> std::net::SocketAddr {
    let state = shared(Source::Simulated { seed, faults });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let st = state.clone();
    std::thread::spawn(move || server::serve(listener, st));
    run_simulation(state, seed, faults, Some(frames), 0);
    addr
}

#[test]
fn clean_link_delivers_every_sample_with_values_matching_ground_truth() {
    let addr = start(3, false, 30);
    let (head, body) = get(addr, "/api/state");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["simulated"], true);
    assert!(v["source"].as_str().unwrap().starts_with("SIMULATED"));
    assert_eq!(v["link"]["frames_ok"], 30);
    assert_eq!(v["link"]["frames_rejected"], 0);
    assert_eq!(v["link"]["sequence_gaps"], 0);
    let l = &v["latest"];
    assert_eq!(l["seq"], 30);
    // Ground truth at the device uptime of the last frame (+ the 10 ms measurement wait).
    let env = sim_environment();
    let t_ms = l["device_uptime_ms"].as_u64().unwrap() + 10;
    let t = l["temperature_c"].as_f64().unwrap();
    let h = l["humidity_pct"].as_f64().unwrap();
    assert!((t - env.temperature_c(t_ms)).abs() < 0.06, "{t}");
    assert!((h - env.humidity(t_ms)).abs() < 0.1, "{h}");
    let vb = l["vbus_v"].as_f64().unwrap();
    assert!((vb - 5.02).abs() < 0.03, "{vb}");
    assert_eq!(v["stats"]["temperature_c"]["count"], 30);
    assert_eq!(v["alarms"].as_array().unwrap().len(), 0, "{}", v["alarms"]);

    let (_, hist) = get(addr, "/api/history");
    let h: serde_json::Value = serde_json::from_str(&hist).unwrap();
    assert_eq!(h["rows"].as_array().unwrap().len(), 30);
}

#[test]
fn faults_are_counted_and_never_turned_into_fake_values() {
    let addr = start(11, true, 200);
    let (_, body) = get(addr, "/api/state");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let ok = v["link"]["frames_ok"].as_u64().unwrap();
    let rejected = v["link"]["frames_rejected"].as_u64().unwrap();
    assert_eq!(ok + rejected, 200, "every frame is accounted for");
    assert!(rejected > 0, "line noise must be detected by the frame CRC");
    assert!(v["link"]["sequence_gaps"].as_u64().unwrap() > 0);
    let faults = v["latest"]["sensor_faults"].as_u64().unwrap();
    assert!(faults > 0, "sensor NACK/CRC faults are reported");
    // Samples with a failed sensor transaction carry no temperature (NA), not a stale value.
    let count = v["stats"]["temperature_c"]["count"].as_u64().unwrap();
    assert!(count < ok, "{count} < {ok}");
}

#[test]
fn page_is_served_and_other_methods_and_paths_are_refused() {
    let addr = start(5, false, 2);
    let (head, body) = get(addr, "/");
    assert!(head.contains("text/html"));
    assert!(body.contains("fetch('/api/state')") && body.contains("SIMULATED DATA"));
    let (head, oled) = get(addr, "/display.txt");
    assert!(head.starts_with("HTTP/1.1 200"));
    assert_eq!(oled.lines().count(), 64);
    assert!(oled.lines().all(|l| l.len() == 128) && oled.contains('#'));
    let (head, _) = get(addr, "/nope");
    assert!(head.starts_with("HTTP/1.1 404"));
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "POST /api/state HTTP/1.1\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 405"));
}
