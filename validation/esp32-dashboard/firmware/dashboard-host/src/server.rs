//! Minimal HTTP/1.1 dashboard server (std only).
//!
//! Routes: `GET /` (dashboard page), `GET /api/state` (latest values, statistics, alarms,
//! link health), `GET /api/history` (rolling samples). Only GET is served, request heads are
//! capped at 8 KiB, and the default bind address is loopback.

use crate::{host_ms, Shared};
use dashboard_core::model::{Alarm, Stats};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const MAX_HEAD: usize = 8 * 1024;

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
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'unsafe-inline' 'self'; style-src 'unsafe-inline'\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

pub fn handle(mut stream: TcpStream, state: &Shared) -> std::io::Result<()> {
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
    match path.split('?').next().unwrap_or("") {
        "/" | "/index.html" => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        ),
        "/api/state" => {
            let body = serde_json::to_vec(&state_json(state)).expect("json");
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
        "/api/history" => {
            let body = serde_json::to_vec(&history_json(state)).expect("json");
            respond(&mut stream, "200 OK", "application/json", &body)
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Serve forever (one short-lived thread per connection).
pub fn serve(listener: TcpListener, state: Shared) {
    for stream in listener.incoming().flatten() {
        let st = state.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, &st);
        });
    }
}

/// Dashboard page. All values are inserted with `textContent` (no HTML injection).
pub const PAGE: &str = r##"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ESP32-C3 Environment Dashboard</title>
<style>
body{font-family:system-ui,sans-serif;margin:0;background:#f4f5f7;color:#1d2330}
header{background:#1d2330;color:#fff;padding:12px 20px}
header h1{margin:0;font-size:20px} #src{font-size:13px;opacity:.85;margin-top:4px}
#sim{display:none;background:#ffe08a;color:#3d2f00;padding:6px 20px;font-weight:600}
main{padding:16px 20px;display:grid;gap:16px;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))}
.card{background:#fff;border-radius:8px;padding:14px;box-shadow:0 1px 2px rgba(0,0,0,.08)}
.card h2{margin:0 0 6px;font-size:13px;text-transform:uppercase;color:#5a6275;letter-spacing:.04em}
.v{font-size:34px;font-weight:650} .s{font-size:12px;color:#5a6275;margin-top:4px}
.wide{grid-column:1/-1} .alarm{display:inline-block;background:#d93a3a;color:#fff;border-radius:4px;padding:2px 8px;margin:2px;font-size:13px}
.ok{color:#227a3a;font-weight:600} svg{width:100%;height:220px;background:#fafbfc;border-radius:6px}
</style></head><body>
<header><h1>ESP32-C3 Environment Dashboard</h1><div id="src">connecting…</div></header>
<div id="sim">SIMULATED DATA — not a physical hardware measurement</div>
<main>
<div class="card"><h2>Temperature</h2><div class="v" id="t">–</div><div class="s" id="ts"></div></div>
<div class="card"><h2>Relative humidity</h2><div class="v" id="h">–</div><div class="s" id="hs"></div></div>
<div class="card"><h2>USB VBUS</h2><div class="v" id="vb">–</div><div class="s" id="vs"></div></div>
<div class="card"><h2>Link</h2><div class="v" id="seq">–</div><div class="s" id="link"></div></div>
<div class="card wide"><h2>Alarms</h2><div id="alarms"></div></div>
<div class="card wide"><h2>History (temperature °C red, humidity % blue)</h2><svg id="chart" viewBox="0 0 600 220" preserveAspectRatio="none"></svg></div>
</main>
<script>
const $=id=>document.getElementById(id);
const f=(v,d,u)=>v===null||v===undefined?'no data':v.toFixed(d)+u;
const st=(s,d,u)=>s?`min ${s.min.toFixed(d)} · mean ${s.mean.toFixed(d)} · max ${s.max.toFixed(d)} ${u} (${s.count} samples)`:'no samples';
function line(vals,lo,hi,color){const n=vals.length;if(n<2)return'';let p='';
 vals.forEach((v,i)=>{if(v===null)return;const x=i*600/(n-1),y=210-(v-lo)/(hi-lo||1)*200;p+=(p?'L':'M')+x.toFixed(1)+' '+y.toFixed(1)});
 return `<path d="${p}" fill="none" stroke="${color}" stroke-width="2"/>`;}
async function tick(){try{
 const s=await (await fetch('/api/state')).json();
 $('src').textContent=s.source; $('sim').style.display=s.simulated?'block':'none';
 const l=s.latest||{};
 $('t').textContent=f(l.temperature_c,2,' °C'); $('ts').textContent=st(s.stats.temperature_c,2,'°C');
 $('h').textContent=f(l.humidity_pct,1,' %'); $('hs').textContent=st(s.stats.humidity_pct,1,'%');
 $('vb').textContent=f(l.vbus_v,2,' V'); $('vs').textContent=st(s.stats.vbus_v,2,'V');
 $('seq').textContent=l.seq===undefined?'–':'#'+l.seq;
 $('link').textContent=`${s.link.frames_ok} ok · ${s.link.frames_rejected} rejected · ${s.link.sequence_gaps} gaps · sensor faults ${l.sensor_faults??'–'} · button ${l.button_pressed?'PRESSED':'released'}`;
 const a=$('alarms'); a.replaceChildren();
 if(!s.alarms.length){const o=document.createElement('span');o.className='ok';o.textContent='No active alarms';a.append(o);}
 s.alarms.forEach(x=>{const e=document.createElement('span');e.className='alarm';e.textContent=x;a.append(e);});
 const h=await (await fetch('/api/history')).json();
 const T=h.rows.map(r=>r[2]),H=h.rows.map(r=>r[3]);
 const tv=T.filter(v=>v!==null),hv=H.filter(v=>v!==null);
 $('chart').innerHTML=line(T,Math.min(...tv)-0.5,Math.max(...tv)+0.5,'#d93a3a')+line(H,Math.min(...hv)-1,Math.max(...hv)+1,'#2f6fd6');
}catch(e){$('src').textContent='dashboard server unreachable: '+e;}}
tick(); setInterval(tick,1000);
</script></body></html>
"##;
