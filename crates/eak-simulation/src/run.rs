//! ngspice subprocess runner: batch mode, timeout with plain `Child::kill`, typed results.

use crate::export::{export_netlist, ExtraSource};
use crate::{raw, SimError};
use eak_core::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TRAN_POINTS: f64 = 1_000_000.0;
const MAX_RETURNED_POINTS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Analysis {
    Op,
    Tran { stop: f64, step: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimRequest {
    pub analysis: Analysis,
    #[serde(default)]
    pub sources: Vec<ExtraSource>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeVoltage {
    pub net: String,
    pub volts: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchCurrent {
    pub element: String,
    pub amps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub net: String,
    pub volts: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transient {
    pub time: Vec<f64>,
    pub traces: Vec<Trace>,
    pub points_total: usize,
    pub decimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimResult {
    pub analysis: Analysis,
    pub ngspice_version: String,
    /// `.op`: operating point. `.tran`: values at the final time point.
    pub nodes: Vec<NodeVoltage>,
    pub currents: Vec<BranchCurrent>,
    pub tran: Option<Transient>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
    pub netlist: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NgspiceStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// Human-readable reason when unavailable.
    pub message: Option<String>,
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    std::env::split_paths(&path).find_map(|d| {
        exts.iter().map(|e| d.join(format!("{name}{e}"))).find(|p| p.is_file())
    })
}

fn find_ngspice() -> Result<PathBuf, SimError> {
    if let Some(p) = std::env::var_os("EAK_NGSPICE").map(PathBuf::from) {
        return if p.is_file() { Ok(p) } else { Err(SimError::NotInstalled) };
    }
    ["ngspice", "ngspice_con"].iter().find_map(|n| which(n)).ok_or(SimError::NotInstalled)
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Scratch, SimError> {
        let d = std::env::temp_dir().join(format!("eak-sim-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).map_err(|e| SimError::Io(e.to_string()))?;
        Ok(Scratch(d))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run `exe args` in `dir`, output redirected to files (no pipe deadlock), kill on timeout.
fn run_child(exe: &Path, args: &[&str], dir: &Path, timeout: Duration) -> Result<(std::process::ExitStatus, String), SimError> {
    let out = dir.join("stdout.txt");
    let f = |p: &Path| std::fs::File::create(p).map_err(|e| SimError::Io(e.to_string()));
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(f(&out)?)
        .stderr(f(&dir.join("stderr.txt"))?)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SimError::NotInstalled,
            _ => SimError::Io(e.to_string()),
        })?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| SimError::Io(e.to_string()))? {
            Some(s) => break s,
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SimError::Timeout(timeout.as_secs()));
            }
            None => std::thread::sleep(Duration::from_millis(15)),
        }
    };
    let mut text = std::fs::read_to_string(&out).unwrap_or_default();
    text.push_str(&std::fs::read_to_string(dir.join("stderr.txt")).unwrap_or_default());
    Ok((status, text))
}

fn tail(s: &str, n: usize) -> String {
    let t = s.trim();
    let start = t.char_indices().rev().nth(n).map_or(0, |(i, _)| i);
    t[start..].to_string()
}

/// Detect ngspice and its version. Never fails: absence is reported in the status.
pub fn probe() -> NgspiceStatus {
    let missing = |m: String| NgspiceStatus { available: false, path: None, version: None, message: Some(m) };
    let exe = match find_ngspice() {
        Ok(e) => e,
        Err(e) => return missing(e.to_string()),
    };
    let scratch = match Scratch::new() {
        Ok(s) => s,
        Err(e) => return missing(e.to_string()),
    };
    match run_child(&exe, &["-v"], &scratch.0, Duration::from_secs(10)) {
        Ok((_, text)) => match version_of(&text) {
            Some(v) => NgspiceStatus { available: true, path: Some(exe.display().to_string()), version: Some(v), message: None },
            None => missing(format!("{} did not identify itself as ngspice", exe.display())),
        },
        Err(e) => missing(e.to_string()),
    }
}

fn version_of(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let i = l.find("ngspice-")?;
        let v: String = l[i + 8..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        (!v.is_empty()).then_some(v)
    })
}

fn validate(a: &Analysis) -> Result<(), SimError> {
    if let Analysis::Tran { stop, step } = *a {
        let ok = stop.is_finite() && step.is_finite() && stop > 0.0 && step > 0.0 && step <= stop;
        if !ok {
            return Err(SimError::InvalidRequest("transient needs 0 < step <= stop".into()));
        }
        if stop / step > MAX_TRAN_POINTS {
            return Err(SimError::InvalidRequest(format!("stop/step exceeds {MAX_TRAN_POINTS:.0} points")));
        }
    }
    Ok(())
}

fn decimate(time: &[f64], cols: &[&[f64]]) -> (Vec<f64>, Vec<Vec<f64>>, bool) {
    let n = time.len();
    let stride = n.div_ceil(MAX_RETURNED_POINTS).max(1);
    let idx: Vec<usize> = (0..n).step_by(stride).chain((n > 0 && (n - 1) % stride != 0).then_some(n - 1)).collect();
    (
        idx.iter().map(|&i| time[i]).collect(),
        cols.iter().map(|c| idx.iter().map(|&i| c[i]).collect()).collect(),
        stride > 1,
    )
}

pub fn run(p: &Project, req: &SimRequest) -> Result<SimResult, SimError> {
    validate(&req.analysis)?;
    let exe = find_ngspice()?;
    let export = export_netlist(p, &req.sources)?;
    let control = match req.analysis {
        Analysis::Op => "op".to_string(),
        Analysis::Tran { stop, step } => format!("tran {step:e} {stop:e}"),
    };
    let deck = format!("{}.control\nset filetype=ascii\n{control}\nwrite out.raw\nquit\n.endc\n.end\n", export.text);
    let scratch = Scratch::new()?;
    std::fs::write(scratch.0.join("deck.cir"), &deck).map_err(|e| SimError::Io(e.to_string()))?;
    let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, 600));
    let start = Instant::now();
    let (status, log) = run_child(&exe, &["-b", "deck.cir"], &scratch.0, timeout)?;
    let rawtext = std::fs::read_to_string(scratch.0.join("out.raw"));
    let Ok(rawtext) = rawtext else {
        return Err(SimError::NgspiceFailed { status: status.to_string(), log: tail(&log, 1500) });
    };
    let plot = raw::parse(&rawtext)?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let net_of = |name: &str| -> Option<&String> {
        let n = name.to_ascii_lowercase();
        let n = n.strip_prefix("v(").and_then(|s| s.strip_suffix(')')).unwrap_or(&n).to_string();
        export.nodes.get(&n)
    };
    let last = |c: &[f64]| c.last().copied().unwrap_or(f64::NAN);
    let (mut nodes, mut currents) = (Vec::new(), Vec::new());
    for (v, col) in plot.variables.iter().zip(&plot.columns) {
        match v.kind.as_str() {
            "voltage" => {
                if let Some(net) = net_of(&v.name) {
                    nodes.push(NodeVoltage { net: net.clone(), volts: last(col) });
                }
            }
            "current" => {
                let base = v.name.trim_end_matches("#branch");
                let base = base.strip_prefix("i(").and_then(|s| s.strip_suffix(')')).unwrap_or(base);
                if let Some(e) = export.elements.iter().find(|e| e.line.split(' ').next().is_some_and(|id| id.eq_ignore_ascii_case(base))) {
                    currents.push(BranchCurrent { element: e.refdes.clone(), amps: last(col) });
                }
            }
            _ => {}
        }
    }
    nodes.sort_by(|a, b| a.net.cmp(&b.net));
    if nodes.is_empty() && currents.is_empty() {
        return Err(SimError::BadOutput("ngspice produced no node voltages".into()));
    }
    let tran = match req.analysis {
        Analysis::Op => None,
        Analysis::Tran { .. } => {
            let time = plot.column("time").ok_or_else(|| SimError::BadOutput("transient rawfile has no time vector".into()))?;
            let named: Vec<(String, &[f64])> = plot
                .variables
                .iter()
                .zip(&plot.columns)
                .filter(|(v, _)| v.kind == "voltage")
                .filter_map(|(v, c)| net_of(&v.name).map(|n| (n.clone(), c.as_slice())))
                .collect();
            let cols: Vec<&[f64]> = named.iter().map(|(_, c)| *c).collect();
            let (t, cs, decimated) = decimate(time, &cols);
            Some(Transient {
                time: t,
                traces: named.iter().zip(cs).map(|((n, _), volts)| Trace { net: n.clone(), volts }).collect(),
                points_total: time.len(),
                decimated,
            })
        }
    };
    Ok(SimResult {
        analysis: req.analysis,
        ngspice_version: version_of(&log).unwrap_or_else(|| probe().version.unwrap_or_default()),
        nodes,
        currents,
        tran,
        warnings: export.warnings,
        limitations: vec![
            "Computed by an external ngspice from the exported schematic; EAK does not verify it.".into(),
            "Diodes/LEDs use generic EAK models, not manufacturer SPICE models.".into(),
            "Parasitics, tolerances, temperature and PCB effects are not modelled.".into(),
        ],
        netlist: deck,
        elapsed_ms,
    })
}
