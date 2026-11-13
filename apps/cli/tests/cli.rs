//! End-to-end tests that drive the real `eak` binary on the reference design.
//! Every step runs in a fresh process, so history, locks and evidence must survive
//! process boundaries exactly as they do for automation.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn eak(args: &[&str]) -> (i32, Value) {
    eak_stdin(args, None)
}

fn eak_stdin(args: &[&str], stdin: Option<&str>) -> (i32, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_eak"));
    cmd.arg("--json")
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("eak binary starts");
    if let Some(input) = stdin {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().expect("eak finishes");
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("eak {args:?} printed non-JSON ({e}): {text}"));
    let code = out.status.code().expect("exit code");
    assert_eq!(
        v["exit_code"], code,
        "JSON exit_code matches process exit code"
    );
    assert_eq!(v["ok"], code == 0);
    (code, v)
}

/// A unique scratch directory, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("eak-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
    fn path(&self, p: &str) -> String {
        self.0.join(p).to_str().unwrap().to_string()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rev(v: &Value) -> u64 {
    v["revision"].as_u64().expect("revision")
}

fn report<'a>(v: &'a Value, check: &str) -> &'a Value {
    v["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["check"] == check)
        .unwrap_or_else(|| panic!("{check} report missing"))
}

/// The DRC-violating track used by the eak-manufacturing e2e test: +5V straight across
/// the top layer, colliding with other copper.
const SHORT: &str = r#"{"type":"AddTrack","layer":"top","a":{"x":1000000,"y":15000000},"b":{"x":39000000,"y":15000000},"net":"+5V"}"#;

#[test]
fn reference_design_through_the_cli() {
    let s = Scratch::new("ref");
    let dir = s.path("ref.eak");

    // Build the reference design through the command engine.
    let (code, v) = eak(&["demo", &dir]);
    assert_eq!(code, 0, "{v}");
    let r0 = rev(&v);
    assert!(r0 > 0);
    assert!(
        v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("0 failed")),
        "router reports no failed connections: {v}"
    );

    // Verification on the routed board: no errors, honest about gaps, evidence stored.
    let (code, v) = eak(&["check", &dir, "all"]);
    assert_eq!(code, 0, "{v}");
    for c in ["ERC", "DRC", "MECH"] {
        let r = report(&v, c);
        assert_eq!(r["errors"], 0, "{c}: {r}");
        assert_eq!(r["revision"], r0);
        assert!(!r["not_implemented"].as_array().unwrap().is_empty());
        assert_eq!(r["outcome"], "NO_ERRORS_IN_IMPLEMENTED_CHECKS");
    }
    for p in v["evidence"].as_array().unwrap() {
        assert!(Path::new(p.as_str().unwrap()).is_file());
    }

    // Checkpoint, then list it from a new process.
    let (code, v) = eak(&["checkpoint", &dir, "routed"]);
    assert_eq!(code, 0, "{v}");
    let cp = v["checkpoint"].as_str().unwrap().to_string();
    let (code, v) = eak(&["checkpoints", &dir]);
    assert_eq!(code, 0);
    assert_eq!(v["checkpoints"], serde_json::json!([cp]));

    // Inject a short through a JSON command file: DRC must catch it, and the package
    // compiled from that revision is only a DRAFT.
    let short = s.path("short.json");
    std::fs::write(&short, SHORT).unwrap();
    let (code, v) = eak(&["apply", &dir, &short]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["applied"], 1);
    assert_eq!(rev(&v), r0 + 1);
    let (code, v) = eak(&["check", &dir, "drc"]);
    assert_eq!(code, 3, "DRC errors give exit code 3: {v}");
    let drc = report(&v, "DRC");
    assert!(drc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["severity"] == "error"
            && (f["rule_id"] == "DRC-002" || f["rule_id"] == "DRC-003")));
    let draft_out = s.path("draft-pkg");
    let (code, v) = eak(&["compile", &dir, "--out", &draft_out]);
    assert_eq!(code, 3, "{v}");
    assert_eq!(v["manifest"]["status"], "DRAFT");
    assert!(!v["manifest"]["blocking_reasons"]
        .as_array()
        .unwrap()
        .is_empty());

    // Undo in a fresh process (history is rebuilt from the journal), then the board
    // verifies clean again and compiles to an internally verified release candidate.
    let (code, v) = eak(&["undo", &dir]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["response"]["revision_after"], r0 + 2);
    let (code, v) = eak(&["check", &dir, "all"]);
    assert_eq!(code, 0, "{v}");
    let (code, v) = eak(&["compile", &dir]);
    assert_eq!(code, 0, "{v}");
    let m = &v["manifest"];
    assert_eq!(m["status"], "INTERNALLY_VERIFIED_RELEASE_CANDIDATE");
    assert_eq!(m["revision"], r0 + 2);
    assert_eq!(m["files"].as_array().unwrap().len(), 12);
    assert!(v["verification_problems"].as_array().unwrap().is_empty());
    let out = PathBuf::from(v["out"].as_str().unwrap());
    assert!(out.join("manifest.json").is_file());
    assert!(m["statement"]
        .as_str()
        .unwrap()
        .contains("NOT a manufacturing-ready"));

    // Re-compiling into an existing package is refused unless explicitly overwritten.
    let (code, v) = eak(&["compile", &dir]);
    assert_eq!(code, 1, "{v}");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--overwrite"));
    let (code, v) = eak(&["compile", &dir, "--overwrite"]);
    assert_eq!(code, 0, "{v}");

    // Redo brings the short back; restoring the checkpoint removes it as a new revision.
    let (code, v) = eak(&["redo", &dir]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["response"]["revision_after"], r0 + 3);
    let (code, v) = eak(&["check", &dir, "drc"]);
    assert_eq!(code, 3, "{v}");
    let (code, v) = eak(&["restore", &dir, &cp]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["response"]["revision_after"], r0 + 4);
    let (code, v) = eak(&["check", &dir, "drc"]);
    assert_eq!(code, 0, "{v}");

    // Reopen: state survives, nothing left to recover, lock was released.
    let (code, v) = eak(&["open", &dir]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(rev(&v), r0 + 4);
    assert_eq!(v["can_undo"], true);
    assert!(v["blocked"].is_null());
    assert!(v["recovery"]["reconciled"].as_array().unwrap().is_empty());
}

#[test]
fn new_project_and_command_batches() {
    let s = Scratch::new("new");
    let dir = s.path("blank.eak");
    let (code, v) = eak(&["new", &dir, "--name", "Blank"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["name"], "Blank");
    assert_eq!(rev(&v), 0);
    // Never overwrite an existing project.
    let (code, v) = eak(&["new", &dir]);
    assert_eq!(code, 1, "{v}");

    // A batch from stdin: each command is its own transaction; the batch stops at the
    // first rejection and earlier commands remain applied (and undoable).
    let batch = r#"[
        {"type":"SetProjectName","name":"Renamed"},
        {"type":"AddRequirement","text":"5 V input"},
        {"type":"SetProjectName","name":""},
        {"type":"AddRequirement","text":"never reached"}
    ]"#;
    let (code, v) = eak_stdin(&["apply", &dir, "-"], Some(batch));
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["applied"], 2);
    assert_eq!(v["total"], 4);
    assert_eq!(v["responses"].as_array().unwrap().len(), 3);
    assert_eq!(v["responses"][2]["error"]["code"], "Invalid");
    assert_eq!(rev(&v), 2);

    // Optimistic concurrency: a stale --expect-rev is rejected without changes.
    let one = s.path("one.json");
    std::fs::write(&one, r#"{"type":"AddRequirement","text":"late"}"#).unwrap();
    let (code, v) = eak(&["apply", &dir, &one, "--expect-rev", "1"]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["responses"][0]["error"]["code"], "StaleRevision");
    assert_eq!(rev(&v), 2);
    let (code, v) = eak(&["apply", &dir, &one, "--expect-rev", "2"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(rev(&v), 3);

    // Malformed input never touches the project.
    let bad = s.path("bad.json");
    std::fs::write(&bad, r#"{"type":"NoSuchCommand"}"#).unwrap();
    let (code, v) = eak(&["apply", &dir, &bad]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["error"]["kind"], "failed");
    std::fs::write(&bad, "not json").unwrap();
    let (code, _) = eak(&["apply", &dir, &bad]);
    assert_eq!(code, 1);
    let (_, v) = eak(&["open", &dir]);
    assert_eq!(rev(&v), 3);

    // Undo everything, then undo again: nothing to undo is a failure, not a silent no-op.
    for expected in [4, 5, 6] {
        let (code, v) = eak(&["undo", &dir]);
        assert_eq!(code, 0, "{v}");
        assert_eq!(v["response"]["revision_after"], expected);
    }
    let (code, v) = eak(&["undo", &dir]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["response"]["error"]["code"], "NothingTo");
    let (_, v) = eak(&["open", &dir]);
    assert_eq!(v["name"], "Blank");
    assert_eq!(v["can_redo"], true);
}

#[test]
fn project_lock_and_usage_errors() {
    let s = Scratch::new("lock");
    let dir = s.path("locked.eak");
    assert_eq!(eak(&["new", &dir]).0, 0);
    // A lock left by a crashed session blocks access until explicitly taken over.
    std::fs::write(Path::new(&dir).join(".lock"), "99999").unwrap();
    let (code, v) = eak(&["open", &dir]);
    assert_eq!(code, 1, "{v}");
    assert!(v["error"]["message"].as_str().unwrap().contains("locked"));
    let (code, v) = eak(&["--take-over", "open", &dir]);
    assert_eq!(code, 0, "{v}");
    assert!(
        !Path::new(&dir).join(".lock").exists(),
        "lock released on exit"
    );

    // Usage errors exit with 2 and never touch a project.
    for args in [
        vec!["frobnicate"],
        vec!["open"],
        vec!["check", dir.as_str(), "spice"],
        vec!["open", dir.as_str(), "--out", "x"],
        vec!["apply", dir.as_str(), "f.json", "--expect-rev", "abc"],
        vec!["--bogus-flag"],
    ] {
        let (code, v) = eak(&args);
        assert_eq!(code, 2, "{args:?}: {v}");
        assert_eq!(v["error"]["kind"], "usage");
    }
    let (code, v) = eak(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    // Opening a folder that is not a project fails cleanly.
    let empty = s.path("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let (code, v) = eak(&["open", &empty]);
    assert_eq!(code, 1, "{v}");
}

#[test]
fn unknown_outcome_blocks_writes_across_invocations_until_acknowledged() {
    let s = Scratch::new("recover");
    let dir = s.path("crashed.eak");
    assert_eq!(eak(&["new", &dir]).0, 0);
    // Simulate a crash leaving a pending operation whose revisions match neither side.
    let journal = Path::new(&dir).join("journal.jsonl");
    let mut j = std::fs::read_to_string(&journal).unwrap_or_default();
    j.push_str(
        r#"{"op_id":"op_lost","state":"pending","kind":"do","revision_before":40,"revision_after":41,"actor":"human:user","command":"X","timestamp_ms":0}"#,
    );
    j.push('\n');
    std::fs::write(&journal, j).unwrap();
    let cmd = s.path("cmd.json");
    std::fs::write(&cmd, r#"{"type":"SetProjectName","name":"edited"}"#).unwrap();

    let (code, v) = eak(&["open", &dir]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["recovery"]["reconciled"][0][1], "unknown_outcome");
    assert!(v["blocked"].as_str().unwrap().contains("op_lost"));
    // Every later invocation stays blocked: reopening never lifts it silently.
    for _ in 0..2 {
        let (code, v) = eak(&["apply", &dir, &cmd]);
        assert_eq!(code, 1, "{v}");
        assert_eq!(v["responses"][0]["error"]["code"], "Precondition");
    }
    let (code, v) = eak(&["undo", &dir]);
    assert_eq!(code, 1, "{v}");
    let (code, v) = eak(&["acknowledge-recovery", &dir]);
    assert_eq!(code, 0, "{v}");
    assert!(v["acknowledged"].as_str().unwrap().contains("op_lost"));
    assert!(v["blocked"].is_null());
    let (code, v) = eak(&["apply", &dir, &cmd]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(rev(&v), 1);
}

/// A small LED board written as ONE command file, using `@REFDES` references to objects
/// created earlier in the same batch, then verified and compiled — the scripted workflow
/// validation projects use.
const LED_BOARD: &str = r#"[
  {"type":"SetProjectName","name":"LED board"},
  {"type":"PlaceSymbol","sheet_id":null,"component_id":"CONN_1x02_P2.54","position":{"x":20000000,"y":60000000},"refdes":"J1"},
  {"type":"PlaceSymbol","sheet_id":null,"component_id":"R_0603","position":{"x":50000000,"y":40000000},"refdes":"R1","value":"1k"},
  {"type":"PlaceSymbol","sheet_id":null,"component_id":"LED_0603","position":{"x":80000000,"y":50000000},"refdes":"D1","value":"Red"},
  {"type":"LabelPin","symbol_id":"@J1","pin":"1","name":"+5V","kind":"power"},
  {"type":"LabelPin","symbol_id":"@J1","pin":"2","name":"GND","kind":"power"},
  {"type":"LabelPin","symbol_id":"@R1","pin":"1","name":"+5V"},
  {"type":"ConnectPins","a_symbol":"@R1","a_pin":"2","b_symbol":"@D1","b_pin":"2"},
  {"type":"LabelPin","symbol_id":"@D1","pin":"1","name":"GND"},
  {"type":"SetBoardOutline","points":[{"x":0,"y":0},{"x":25000000,"y":0},{"x":25000000,"y":20000000},{"x":0,"y":20000000}]},
  {"type":"SetNetCurrent","net":"+5V","amps":0.02},
  {"type":"SetNetVoltage","net":"+5V","volts":5.0},
  {"type":"SyncSchematicToPcb"},
  {"type":"AutoPlace","grid":1000000},
  {"type":"SetFootprintLocked","footprint_id":"@J1","locked":true},
  {"type":"AutoRoute"}
]"#;

#[test]
fn scripted_board_with_refdes_references() {
    let s = Scratch::new("script");
    let dir = s.path("led.eak");
    assert_eq!(eak(&["new", &dir]).0, 0);
    let (code, v) = eak_stdin(&["apply", &dir, "-"], Some(LED_BOARD));
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["applied"], 16);
    let route = v["responses"][15]["warnings"].to_string();
    assert!(route.contains("0 failed"), "{route}");

    // `show` exposes the canonical IR; @J1 resolved to J1's footprint.
    let (code, v) = eak(&["show", &dir]);
    assert_eq!(code, 0);
    let fps = v["project"]["pcb"]["footprints"].as_object().unwrap();
    assert_eq!(fps.len(), 3);
    assert!(fps
        .values()
        .any(|f| f["refdes"] == "J1" && f["locked"] == true));

    // Exit code and report agree: 0 means no errors in implemented checks, else 3.
    let (code, v) = eak(&["check", &dir, "all"]);
    let errors = v["errors"].as_u64().unwrap();
    assert_eq!(code, if errors == 0 { 0 } else { 3 }, "{v}");
    let (code, v) = eak(&["compile", &dir]);
    let status = v["manifest"]["status"].as_str().unwrap();
    assert_eq!(code, if status == "DRAFT" { 3 } else { 0 }, "{v}");
    assert!(v["verification_problems"].as_array().unwrap().is_empty());

    // Unknown references stop the batch before execution, with a clear error.
    let (code, v) = eak_stdin(
        &["apply", &dir, "-"],
        Some(
            r#"[{"type":"SetSymbolValue","symbol_id":"@R1","value":"2k2"},{"type":"SetSymbolValue","symbol_id":"@R99","value":"x"}]"#,
        ),
    );
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["applied"], 1);
    assert_eq!(v["error"]["kind"], "reference");
    assert!(v["error"]["message"].as_str().unwrap().contains("R99"));
}
