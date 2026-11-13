//! `eak` — headless command-line front end to the shared EAK command engine.
//!
//! The CLI is a thin client: every design mutation goes through
//! `eak_runtime::Engine::execute` (or the engine's own undo/redo/restore), verification
//! runs in `eak-validation`, and packages are compiled by `eak-manufacturing`. The CLI
//! never writes project files itself. Each invocation opens the project (taking its
//! write lock), performs one action and releases the lock on exit.

use eak_manufacturing::{compile, verify_package, Manifest, PackageStatus};
use eak_runtime::{fixtures, Actor, ActorKind, Command, CommandRequest, CommandResponse, Engine};
use eak_validation::{drc, erc, mech, Report, Severity};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
eak — Electronics Agent Kit command-line interface

USAGE:
    eak [--json] [--take-over] <SUBCOMMAND> [ARGS]

SUBCOMMANDS:
    new <dir> [--name <name>]         Create a new, empty project folder
    demo <dir>                        Create a project holding the reference design
                                      (built through the command engine)
    open <dir>                        Open a project: revision, history, recovery state
    show <dir>                        Print the canonical EAK-IR project document
    acknowledge-recovery <dir>        Accept the on-disk state after an interrupted
                                      operation with an unknown outcome; lifts the
                                      write block
    apply <dir> <file|-> [--expect-rev <n>]
                                      Execute a JSON command (or array of commands)
                                      read from a file or stdin, one transaction each.
                                      In symbol_id/a_symbol/b_symbol/footprint_id, '@R1'
                                      means the object whose refdes is R1 at that point
    undo <dir> [--expect-rev <n>]     Undo the last operation
    redo <dir> [--expect-rev <n>]     Redo the last undone operation
    check <dir> <erc|drc|mech|all>    Run verification and store revision-bound evidence
    compile <dir> [--out <dir>] [--overwrite]
                                      Run ERC/DRC/MECH on the current revision, compile
                                      and re-verify a manufacturing package
    checkpoint <dir> <name>           Save a named checkpoint of the current revision
    checkpoints <dir>                 List checkpoints
    restore <dir> <checkpoint-id> [--expect-rev <n>]
                                      Restore a checkpoint as a new, undoable revision

PROJECT FILES:
    Every accepted command is saved immediately (atomic write + journal); there is
    no separate save step.

GLOBAL OPTIONS:
    --json        Print exactly one JSON object on stdout (for automation)
    --take-over   Take over a stale project lock left by a crashed session
    -h, --help    Show this help
    -V, --version Show the version

EXIT CODES:
    0  success
    1  failure (command rejected, project or I/O error)
    2  usage error
    3  verification found errors, or the package is only a DRAFT
";

const EXIT_OK: u8 = 0;
const EXIT_FAILED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_CHECKS: u8 = 3;

#[derive(Default)]
struct Args {
    json: bool,
    take_over: bool,
    name: Option<String>,
    expect_rev: Option<u64>,
    out: Option<PathBuf>,
    overwrite: bool,
    pos: Vec<String>,
    /// Options given, for per-subcommand validation.
    given: Vec<&'static str>,
}

enum CliError {
    Usage(String),
    Failed(String),
}

impl From<String> for CliError {
    fn from(s: String) -> CliError {
        CliError::Failed(s)
    }
}

/// Result of one subcommand: exit code, JSON payload and human-readable text.
struct Done {
    code: u8,
    json: Value,
    text: String,
}

fn parse(argv: Vec<String>) -> Result<Args, CliError> {
    let mut a = Args::default();
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .ok_or_else(|| CliError::Usage(format!("{flag} needs a value")))
        };
        match arg.as_str() {
            "--json" => a.json = true,
            "--take-over" => a.take_over = true,
            "--overwrite" => {
                a.overwrite = true;
                a.given.push("--overwrite");
            }
            "--name" => {
                a.name = Some(value("--name")?);
                a.given.push("--name");
            }
            "--out" => {
                a.out = Some(PathBuf::from(value("--out")?));
                a.given.push("--out");
            }
            "--expect-rev" => {
                let v = value("--expect-rev")?;
                a.expect_rev = Some(v.parse().map_err(|_| {
                    CliError::Usage(format!("--expect-rev needs a revision number, got {v:?}"))
                })?);
                a.given.push("--expect-rev");
            }
            "-h" | "--help" => a.pos.insert(0, "help".into()),
            "-V" | "--version" => a.pos.insert(0, "version".into()),
            "-" => a.pos.push(arg),
            s if s.starts_with('-') => return Err(CliError::Usage(format!("unknown option {s}"))),
            _ => a.pos.push(arg),
        }
    }
    Ok(a)
}

fn actor() -> Actor {
    Actor {
        kind: ActorKind::Cli,
        name: "eak-cli".into(),
    }
}

fn open(dir: &Path, a: &Args) -> Result<Engine, CliError> {
    Engine::open(dir, a.take_over).map_err(|e| CliError::Failed(format!("{}: {e}", dir.display())))
}

fn response_text(r: &CommandResponse) -> String {
    match &r.error {
        None => format!(
            "accepted {} (revision {} -> {}){}",
            r.command,
            r.revision_before,
            r.revision_after,
            if r.warnings.is_empty() {
                String::new()
            } else {
                format!("\n  warnings: {}", r.warnings.join("; "))
            }
        ),
        Some(e) => format!("REJECTED {}: {e}", r.command),
    }
}

fn response_done(r: CommandResponse) -> Done {
    Done {
        code: if r.accepted { EXIT_OK } else { EXIT_FAILED },
        text: response_text(&r),
        json: json!({ "response": r }),
    }
}

fn status_json(e: &Engine) -> Value {
    let p = e.project();
    json!({
        "project_id": p.id,
        "name": p.name,
        "revision": p.revision,
        "schema_version": p.schema_version,
        "dir": e.dir().map(|d| d.display().to_string()),
        "can_undo": e.can_undo(),
        "can_redo": e.can_redo(),
        "blocked": e.blocked_reason(),
        "recovery": e.recovery,
        "checkpoints": e.list_checkpoints(),
        "counts": {
            "symbols": p.schematic.symbols.len(),
            "wires": p.schematic.wires.len(),
            "labels": p.schematic.labels.len(),
            "footprints": p.pcb.footprints.len(),
            "tracks": p.pcb.tracks.len(),
            "vias": p.pcb.vias.len(),
            "zones": p.pcb.zones.len(),
        },
    })
}

fn status_text(e: &Engine) -> String {
    let p = e.project();
    let mut s = format!(
        "project {} ({})\nrevision {}  undo: {}  redo: {}\nsymbols {}  footprints {}  tracks {}  vias {}  zones {}",
        p.name,
        p.id,
        p.revision,
        if e.can_undo() { "yes" } else { "no" },
        if e.can_redo() { "yes" } else { "no" },
        p.schematic.symbols.len(),
        p.pcb.footprints.len(),
        p.pcb.tracks.len(),
        p.pcb.vias.len(),
        p.pcb.zones.len(),
    );
    for (op, state) in &e.recovery.reconciled {
        s += &format!("\nrecovered interrupted operation {op}: {state:?}");
    }
    if let Some(b) = e.blocked_reason() {
        s += &format!("\nBLOCKED: {b}");
    }
    s
}

/// Read a command file (one JSON command or an array). Every command is checked against
/// the typed `Command` schema before anything executes.
fn read_commands(src: &str) -> Result<Vec<Value>, CliError> {
    let text = if src == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| CliError::Failed(format!("reading stdin: {e}")))?;
        s
    } else {
        std::fs::read_to_string(src).map_err(|e| CliError::Failed(format!("{src}: {e}")))?
    };
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::Failed(format!("{src}: not valid JSON: {e}")))?;
    let items = match v {
        Value::Array(items) => items,
        one => vec![one],
    };
    if items.is_empty() {
        return Err(CliError::Failed(format!("{src}: no commands")));
    }
    for (i, c) in items.iter().enumerate() {
        serde_json::from_value::<Command>(c.clone())
            .map_err(|e| CliError::Failed(format!("{src}: command #{}: {e}", i + 1)))?;
    }
    Ok(items)
}

/// Command fields that hold a schematic symbol id / a PCB footprint id.
const SYMBOL_FIELDS: [&str; 3] = ["symbol_id", "a_symbol", "b_symbol"];
const FOOTPRINT_FIELDS: [&str; 1] = ["footprint_id"];

/// Replace `"@R1"`-style reference-designator references in id fields with the id of the
/// symbol (or footprint) carrying that refdes in the *current* project state. This lets a
/// command file refer to objects created by earlier commands in the same batch. Only the
/// id fields above are rewritten; every other string is passed through unchanged.
fn resolve_refs(p: &eak_core::Project, cmd: &Value) -> Result<Command, String> {
    let mut cmd = cmd.clone();
    if let Value::Object(map) = &mut cmd {
        for (key, val) in map.iter_mut() {
            let Some(refdes) = val.as_str().and_then(|s| s.strip_prefix('@')) else {
                continue;
            };
            let ids: Vec<&String> = if SYMBOL_FIELDS.contains(&key.as_str()) {
                p.schematic
                    .symbols
                    .values()
                    .filter(|s| s.refdes == refdes)
                    .map(|s| &s.id)
                    .collect()
            } else if FOOTPRINT_FIELDS.contains(&key.as_str()) {
                p.pcb
                    .footprints
                    .values()
                    .filter(|f| f.refdes == refdes)
                    .map(|f| &f.id)
                    .collect()
            } else {
                continue;
            };
            match ids.as_slice() {
                [id] => *val = json!(id),
                [] => {
                    return Err(format!(
                        "{key}: no object with reference designator {refdes}"
                    ))
                }
                _ => {
                    return Err(format!(
                        "{key}: reference designator {refdes} is ambiguous ({} objects)",
                        ids.len()
                    ))
                }
            }
        }
    }
    serde_json::from_value(cmd).map_err(|e| e.to_string())
}

/// Execute commands in order, one engine transaction each; stop at the first rejection.
/// Earlier accepted commands stay applied (each is individually undoable).
fn apply_commands(e: &mut Engine, cmds: Vec<Value>, expect: Option<u64>) -> Done {
    let total = cmds.len();
    let mut expected = expect.unwrap_or(e.project().revision);
    let mut responses = vec![];
    let mut text = vec![];
    let mut failure = None;
    for (i, raw) in cmds.iter().enumerate() {
        let cmd = match resolve_refs(e.project(), raw) {
            Ok(c) => c,
            Err(m) => {
                let m = format!("command #{}: {m}", i + 1);
                text.push(format!("REJECTED {m}"));
                failure = Some(json!({ "kind": "reference", "message": m }));
                break;
            }
        };
        let r = e.execute(CommandRequest {
            expected_revision: expected,
            actor: actor(),
            command: cmd,
            approval: None,
        });
        text.push(response_text(&r));
        expected = r.revision_after;
        let accepted = r.accepted;
        responses.push(r);
        if !accepted {
            failure = Some(json!({
                "kind": "rejected",
                "message": format!("command #{} rejected", i + 1),
            }));
            break;
        }
    }
    let applied = responses.iter().filter(|r| r.accepted).count();
    if failure.is_some() && total > 1 {
        text.push(format!(
            "stopped: {applied} of {total} command(s) applied; the rest were not executed"
        ));
    }
    let mut json = json!({
        "applied": applied,
        "total": total,
        "revision": e.project().revision,
        "responses": responses,
    });
    let code = match failure {
        Some(f) => {
            json["error"] = f;
            EXIT_FAILED
        }
        None => EXIT_OK,
    };
    Done {
        code,
        text: text.join("\n"),
        json,
    }
}

fn run_check(p: &eak_core::Project, which: &str) -> Report {
    match which {
        "ERC" => erc::run(p),
        "DRC" => drc::run(p),
        _ => mech::run(p),
    }
}

/// Run the requested checks on the current revision and store each report as evidence.
fn checks(e: &mut Engine, which: &[&str]) -> Result<Vec<(Report, PathBuf)>, CliError> {
    let mut out = vec![];
    for w in which {
        let r = run_check(e.project(), w);
        let body = serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?;
        let path = e
            .save_evidence(w, &body)
            .ok_or_else(|| CliError::Failed(format!("could not store {w} evidence")))?;
        out.push((r, path));
    }
    Ok(out)
}

fn report_text(r: &Report) -> String {
    let mut s = format!(
        "{} revision {}: {} error(s), {} warning(s), {} rule(s) run, {} check(s) not implemented",
        r.check,
        r.revision,
        r.errors,
        r.warnings,
        r.rules.len(),
        r.not_implemented.len()
    );
    for f in r.findings.iter().filter(|f| f.severity != Severity::Info) {
        s += &format!("\n  {} {:?}: {}", f.rule_id, f.severity, f.message);
    }
    s
}

fn manifest_text(m: &Manifest, out: &Path, problems: &[String]) -> String {
    let mut s = format!(
        "package {} for revision {}: {:?}\n{}",
        out.display(),
        m.revision,
        m.status,
        m.statement
    );
    for b in &m.blocking_reasons {
        s += &format!("\n  blocking: {b}");
    }
    s += &format!("\n  {} file(s) written", m.files.len());
    if problems.is_empty() {
        s += "; on-disk re-verification: no problems";
    }
    for p in problems {
        s += &format!("\n  VERIFY: {p}");
    }
    s
}

fn need(a: &Args, n: usize, form: &str) -> Result<(), CliError> {
    if a.pos.len() != n + 1 {
        return Err(CliError::Usage(format!("usage: eak {form}")));
    }
    Ok(())
}

fn run(a: &Args) -> Result<Done, CliError> {
    let sub = a.pos.first().map(String::as_str).unwrap_or("help");
    let allowed: &[&str] = match sub {
        "new" => &["--name"],
        "apply" | "undo" | "redo" | "restore" => &["--expect-rev"],
        "compile" => &["--out", "--overwrite"],
        _ => &[],
    };
    if let Some(bad) = a.given.iter().find(|g| !allowed.contains(g)) {
        return Err(CliError::Usage(format!("{bad} is not valid for `{sub}`")));
    }
    let dir = || PathBuf::from(&a.pos[1]);
    match sub {
        "help" => Ok(Done {
            code: EXIT_OK,
            json: json!({ "usage": USAGE }),
            text: USAGE.trim_end().into(),
        }),
        "version" => Ok(Done {
            code: EXIT_OK,
            json: json!({ "version": env!("CARGO_PKG_VERSION") }),
            text: format!("eak {}", env!("CARGO_PKG_VERSION")),
        }),
        "new" => {
            need(a, 1, "new <dir> [--name <name>]")?;
            let d = dir();
            let name = a.name.clone().unwrap_or_else(|| {
                d.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string()
            });
            let e = Engine::create(&d, &name)?;
            Ok(Done {
                code: EXIT_OK,
                text: format!("created {}\n{}", d.display(), status_text(&e)),
                json: status_json(&e),
            })
        }
        "demo" => {
            need(a, 1, "demo <dir>")?;
            let d = dir();
            let mut e = Engine::create(&d, "reference")?;
            fixtures::build_schematic(&mut e)?;
            let notes = fixtures::build_pcb(&mut e)?;
            let mut json = status_json(&e);
            json["notes"] = json!(notes);
            Ok(Done {
                code: EXIT_OK,
                text: format!(
                    "created reference design in {}\n{}\n{}",
                    d.display(),
                    status_text(&e),
                    notes.join("\n")
                ),
                json,
            })
        }
        "open" => {
            need(a, 1, "open <dir>")?;
            let e = open(&dir(), a)?;
            Ok(Done {
                code: EXIT_OK,
                text: status_text(&e),
                json: status_json(&e),
            })
        }
        "show" => {
            need(a, 1, "show <dir>")?;
            // Opened through the engine (lock, recovery) so it reflects the true state.
            let e = open(&dir(), a)?;
            Ok(Done {
                code: EXIT_OK,
                text: eak_core::to_json(e.project()),
                json: json!({ "project": e.project() }),
            })
        }
        "acknowledge-recovery" => {
            need(a, 1, "acknowledge-recovery <dir>")?;
            let mut e = open(&dir(), a)?;
            let was = e.blocked_reason().map(String::from);
            e.acknowledge_recovery()?;
            let mut json = status_json(&e);
            json["acknowledged"] = json!(was);
            Ok(Done {
                code: EXIT_OK,
                text: match was {
                    Some(w) => format!("acknowledged: {w}\n{}", status_text(&e)),
                    None => format!("nothing to acknowledge\n{}", status_text(&e)),
                },
                json,
            })
        }
        "apply" => {
            need(a, 2, "apply <dir> <file|-> [--expect-rev <n>]")?;
            // Parse before opening so a malformed file never touches the project.
            let cmds = read_commands(&a.pos[2])?;
            let mut e = open(&dir(), a)?;
            Ok(apply_commands(&mut e, cmds, a.expect_rev))
        }
        "undo" | "redo" => {
            need(a, 1, &format!("{sub} <dir> [--expect-rev <n>]"))?;
            let mut e = open(&dir(), a)?;
            let rev = a.expect_rev.unwrap_or(e.project().revision);
            let r = if sub == "undo" {
                e.undo(rev, &actor())
            } else {
                e.redo(rev, &actor())
            };
            Ok(response_done(r))
        }
        "restore" => {
            need(a, 2, "restore <dir> <checkpoint-id> [--expect-rev <n>]")?;
            let mut e = open(&dir(), a)?;
            let rev = a.expect_rev.unwrap_or(e.project().revision);
            Ok(response_done(e.restore_checkpoint(
                &a.pos[2],
                rev,
                &actor(),
            )))
        }
        "check" => {
            need(a, 2, "check <dir> <erc|drc|mech|all>")?;
            let which: Vec<&str> = match a.pos[2].to_ascii_lowercase().as_str() {
                "erc" => vec!["ERC"],
                "drc" => vec!["DRC"],
                "mech" => vec!["MECH"],
                "all" => vec!["ERC", "DRC", "MECH"],
                other => {
                    return Err(CliError::Usage(format!(
                        "unknown check {other:?}; use erc, drc, mech or all"
                    )))
                }
            };
            let mut e = open(&dir(), a)?;
            let done = checks(&mut e, &which)?;
            let errors: usize = done.iter().map(|(r, _)| r.errors).sum();
            Ok(Done {
                code: if errors == 0 { EXIT_OK } else { EXIT_CHECKS },
                text: done
                    .iter()
                    .map(|(r, p)| format!("{}\n  evidence: {}", report_text(r), p.display()))
                    .collect::<Vec<_>>()
                    .join("\n"),
                json: json!({
                    "revision": e.project().revision,
                    "errors": errors,
                    "reports": done.iter().map(|(r, _)| r).collect::<Vec<_>>(),
                    "evidence": done.iter().map(|(_, p)| p.display().to_string()).collect::<Vec<_>>(),
                }),
            })
        }
        "compile" => {
            need(a, 1, "compile <dir> [--out <dir>] [--overwrite]")?;
            let d = dir();
            let mut e = open(&d, a)?;
            let out = a.out.clone().unwrap_or_else(|| {
                d.join("outputs")
                    .join(format!("r{:06}", e.project().revision))
            });
            // Overwriting an existing package is a destructive release action.
            if out.join("manifest.json").exists() && !a.overwrite {
                return Err(CliError::Failed(format!(
                    "{} already holds a manufacturing package; pass --overwrite to replace it",
                    out.display()
                )));
            }
            let done = checks(&mut e, &["ERC", "DRC", "MECH"])?;
            let reports: Vec<&Report> = done.iter().map(|(r, _)| r).collect();
            let m = compile(e.project(), &reports, &out)?;
            let problems = verify_package(&out)?;
            let code = if !problems.is_empty() {
                EXIT_FAILED
            } else if m.status == PackageStatus::Draft {
                EXIT_CHECKS
            } else {
                EXIT_OK
            };
            Ok(Done {
                code,
                text: manifest_text(&m, &out, &problems),
                json: json!({
                    "out": out.display().to_string(),
                    "manifest": m,
                    "verification_problems": problems,
                    "evidence": done.iter().map(|(_, p)| p.display().to_string()).collect::<Vec<_>>(),
                }),
            })
        }
        "checkpoint" => {
            need(a, 2, "checkpoint <dir> <name>")?;
            let mut e = open(&dir(), a)?;
            let id = e.create_checkpoint(&a.pos[2])?;
            Ok(Done {
                code: EXIT_OK,
                text: format!("checkpoint {id}"),
                json: json!({ "checkpoint": id, "revision": e.project().revision }),
            })
        }
        "checkpoints" => {
            need(a, 1, "checkpoints <dir>")?;
            let e = open(&dir(), a)?;
            let list = e.list_checkpoints();
            Ok(Done {
                code: EXIT_OK,
                text: if list.is_empty() {
                    "no checkpoints".into()
                } else {
                    list.join("\n")
                },
                json: json!({ "checkpoints": list }),
            })
        }
        other => Err(CliError::Usage(format!(
            "unknown subcommand {other:?}; run `eak --help`"
        ))),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let json_mode = argv.iter().any(|a| a == "--json");
    let result = parse(argv).and_then(|a| run(&a));
    let (code, json, text, to_stderr) = match result {
        Ok(d) => (d.code, d.json, d.text, false),
        Err(CliError::Usage(m)) => (
            EXIT_USAGE,
            json!({ "error": { "kind": "usage", "message": m } }),
            format!("error: {m}\n\n{USAGE}"),
            true,
        ),
        Err(CliError::Failed(m)) => (
            EXIT_FAILED,
            json!({ "error": { "kind": "failed", "message": m } }),
            format!("error: {m}"),
            true,
        ),
    };
    if json_mode {
        let mut obj = json;
        obj["ok"] = json!(code == EXIT_OK);
        obj["exit_code"] = json!(code);
        println!(
            "{}",
            serde_json::to_string_pretty(&obj).expect("JSON value is serialisable")
        );
    } else if to_stderr {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }
    ExitCode::from(code)
}
