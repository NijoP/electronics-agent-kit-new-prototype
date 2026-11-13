# Electrical rule provenance

| Rule | Implementation | Source | Verification status |
|---|---|---|---|
| Trace ampacity I = k·ΔT^0.44·A^0.725 (k = 0.048 ext / 0.024 int, A in mil²) | `eak-engineering/src/ipc.rs` | IPC-2221 via DESK `desk/ipc.py` @5ad8492 | Reference vectors pass; **not checked against standard text**. IPC-2152 not implemented. |
| Conductor clearance by voltage, B1–B4, A5–A7 | `ipc.rs::clearance_mm` | IPC-2221B Table 6-1 via DESK | **Unverified transcription** |
| Creepage, PD1 / PD2 (I, II, III) | `ipc.rs::creepage_mm` | IEC 60664-1 Table F.4 via DESK | **Flagged:** discontinuity at 50 V suggests mixed table columns; review required. Not used by DRC yet. |
| Via current (barrel π(d+t)t, internal k) | `ipc.rs::via_current_a` | IPC-2221 via DESK | 12 mil / 1 mil ≈ 1.0 A at 10 °C; DESK header/code inconsistency noted |
| Capacitor voltage margin (< rail error, < 1.5× warning) | ERC-011 | DESK `erc.py` | Heuristic; ratings in the built-in library are unverified |

`VERIFIED_AGAINST_STANDARD = false` is exported by `ipc.rs`, and DRC messages state it. Promotion to
verified requires a named engineer to check the tables against the purchased standards.
