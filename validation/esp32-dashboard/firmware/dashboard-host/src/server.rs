//! Minimal HTTP/1.1 dashboard server (std only).
//!
//! Routes:
//! - `GET /api/state`: latest values, statistics, alarms and link health.
//! - `GET /api/history`: rolling samples.
//! - `GET /display.txt`: the OLED frame-buffer mirror.
//! - `GET /` and `GET /assets/*`: the React + TypeScript dashboard page.
//!
//! The page is the static Vite bundle built from `validation/esp32-dashboard/dashboard-ui`
//! (`npm ci && npm run build` writes `dashboard-ui/dist`), served under a CSP with no inline
//! script. Only GET is served, request heads are capped at 8 KiB, and the default bind
//! address is loopback.

use crate::{host_ms, Shared};
use dashboard_core::model::{Alarm, Stats};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const MAX_HEAD: usize = 8 * 1024;

/// Same-origin module script and stylesheet only: no inline script or style, no plugins,
/// no framing.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// `dashboard-ui/dist` next to this crate's workspace (where `npm run build` writes it).
pub fn default_ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dashboard-ui/dist")
}

fn content_type(name: &str) -> Option<&'static str> {
    Some(match name.rsplit_once('.')?.1 {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => return None,
    })
}

/// Maps a request path to a file in the UI bundle, or `None` if it is not a servable path.
///
/// Only these are served:
/// - `/` (mapped to `index.html`);
/// - paths of at most two segments, where each segment uses only `[A-Za-z0-9._-]`, does not
///   start with `.`, and the last segment has a known extension.
///
/// No percent-decoding is done, so traversal, hidden files and arbitrary types are refused.
pub fn ui_file(ui_dir: &Path, path: &str) -> Option<(PathBuf, &'static str)> {
    let rel = if path == "/" {
        "index.html"
    } else {
        path.strip_prefix('/')?
    };
    let segments: Vec<&str> = rel.split('/').collect();
    let safe = |s: &&str| {
        !s.is_empty()
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    if segments.len() > 2 || !segments.iter().all(safe) {
        return None;
    }
    let ctype = content_type(segments.last()?)?;
    let mut file = ui_dir.to_path_buf();
    file.extend(&segments);
    Some((file, ctype))
}

fn stats(s: Option<Stats>, scale: f64) -> Value {
    match s {
        None => Value::Null,
        Some(s) => json!({
            "count": s.count,
            "min": s.min as f64 / scale,
            "max": s.max as f64 / scale,
            "mean": s.mean as f64 / scale,
        }),
    }
}

pub fn state_json(state: &Shared) -> Value {
    let s = state.lock().expect("state lock");
    let now = host_ms(&s);
    let d = &s.dashboard;
    let mut alarms = [Alarm::LinkStale; 7];
    let n = d.alarms(now, &mut alarms);
    let latest = d.latest().map(|(t, f)| {
        json!({
            "age_ms": now.saturating_sub(t),
            "seq": f.seq,
            "device_uptime_ms": f.uptime_ms,
            "temperature_c": f.temperature_mc.map(|v| v as f64 / 1000.0),
            "humidity_pct": f.humidity_mpct.map(|v| v as f64 / 1000.0),
            "vbus_v": f.vbus_mv.map(|v| v as f64 / 1000.0),
            "button_pressed": f.button_pressed,
            "sensor_faults": f.sensor_faults,
        })
    });
    json!({
        "source": s.source.label(),
        "simulated": matches!(s.source, crate::Source::Simulated { .. }),
        "latest": latest,
        "stats": {
            "temperature_c": stats(d.temperature_stats(), 1000.0),
            "humidity_pct": stats(d.humidity_stats(), 1000.0),
            "vbus_v": stats(d.vbus_stats(), 1000.0),
        },
        "alarms": alarms[..n].iter().map(|a| a.as_str()).collect::<Vec<_>>(),
        "link": {
            "frames_ok": d.frames_ok,
            "frames_rejected": d.frames_rejected,
            "sequence_gaps": d.sequence_gaps,
            "last_error": s.last_error,
        },
        "thresholds": {
            "temp_high_c": d.thresholds.temp_high_mc as f64 / 1000.0,
            "temp_low_c": d.thresholds.temp_low_mc as f64 / 1000.0,
            "rh_high_pct": d.thresholds.rh_high_mpct as f64 / 1000.0,
            "vbus_low_v": d.thresholds.vbus_low_mv as f64 / 1000.0,
        },
    })
}

pub fn history_json(state: &Shared) -> Value {
    let s = state.lock().expect("state lock");
    let rows: Vec<Value> = s
        .dashboard
        .iter()
        .map(|(t, f)| {
            json!([
                t,
                f.seq,
                f.temperature_mc.map(|v| v as f64 / 1000.0),
                f.humidity_mpct.map(|v| v as f64 / 1000.0),
                f.vbus_mv.map(|v| v as f64 / 1000.0)
            ])
        })
        .collect();
    json!({"columns": ["host_ms", "seq", "temperature_c", "humidity_pct", "vbus_v"], "rows": rows})
}

/// The SSD1306 frame buffer the firmware would show for the latest frame, as ASCII art
/// (rendered by the same `dashboard_core::display::render` the firmware calls).
pub fn display_ascii(state: &Shared) -> String {
    use dashboard_core::display::{render, FrameBuffer, PAGES, WIDTH};
    let s = state.lock().expect("state lock");
    let mut fb = FrameBuffer::default();
    if let Some((_, f)) = s.dashboard.latest() {
        render(&mut fb, f);
    }
    let mut out = String::with_capacity((WIDTH + 1) * PAGES * 8);
    for y in 0..PAGES * 8 {
        for x in 0..WIDTH {
            out.push(if fb.pixel(x, y) { '#' } else { '.' });
        }
        out.push('\n');
    }
    out
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: {CSP}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

pub fn handle(mut stream: TcpStream, state: &Shared, ui_dir: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut head = request_line.len();
    loop {
        let mut l = String::new();
        let n = reader.read_line(&mut l)?;
        head += n;
        if n == 0 || l == "\r\n" || l == "\n" {
            break;
        }
        if head > MAX_HEAD {
            return respond(
                &mut stream,
                "431 Request Header Fields Too Large",
                "text/plain",
                b"too large",
            );
        }
    }
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" {
        return respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            b"GET only",
        );
    }
    let path = path.split('?').next().unwrap_or("");
    match path {
        "/api/state" => {
            let body = serde_json::to_vec(&state_json(state)).expect("json");
            respond(&mut stream, "200 OK", "application/json", &body)
        }
        "/api/history" => {
            let body = serde_json::to_vec(&history_json(state)).expect("json");
            respond(&mut stream, "200 OK", "application/json", &body)
        }
        "/display.txt" => {
            let body = display_ascii(state);
            respond(
                &mut stream,
                "200 OK",
                "text/plain; charset=utf-8",
                body.as_bytes(),
            )
        }
        _ => match ui_file(ui_dir, path) {
            Some((file, ctype)) => match std::fs::read(&file) {
                Ok(body) => respond(&mut stream, "200 OK", ctype, &body),
                Err(_) if path == "/" || path == "/index.html" => respond(
                    &mut stream,
                    "503 Service Unavailable",
                    "text/plain; charset=utf-8",
                    b"dashboard UI bundle not built: run `npm ci && npm run build` in validation/esp32-dashboard/dashboard-ui, or pass --ui-dir. The JSON API (/api/state, /api/history) is available.",
                ),
                Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
            },
            None => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
        },
    }
}

/// Serve forever (one short-lived thread per connection). `ui_dir` holds the built
/// dashboard page (`index.html` + `assets/`).
pub fn serve(listener: TcpListener, state: Shared, ui_dir: PathBuf) {
    let ui_dir = Arc::new(ui_dir);
    for stream in listener.incoming().flatten() {
        let (st, ui) = (state.clone(), ui_dir.clone());
        std::thread::spawn(move || {
            let _ = handle(stream, &st, &ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_safe_bundle_paths_map_to_files() {
        let d = Path::new("ui");
        let (f, t) = ui_file(d, "/").unwrap();
        assert_eq!((f, t), (d.join("index.html"), "text/html; charset=utf-8"));
        let (f, t) = ui_file(d, "/assets/index-D5SW_sTU6.js").unwrap();
        assert_eq!(f, d.join("assets").join("index-D5SW_sTU6.js"));
        assert!(t.starts_with("text/javascript"));
        for bad in [
            "/../Cargo.toml",
            "/assets/../../x.js",
            "/assets/%2e%2e/x.js",
            "/.git/config",
            "/assets/.hidden.js",
            "/a/b/c.js",
            "//x.js",
            "/assets/",
            r"/assets\..\x.js",
            "/main.rs",
            "assets/x.js",
            "/C:/x.js",
        ] {
            assert!(ui_file(d, bad).is_none(), "{bad}");
        }
    }
}
