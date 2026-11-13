# Integration test matrix: research, sourcing and manufacturing preparation

This matrix is separate from the unit and end-to-end tests listed in `TESTING.md`. It
covers the supplier-facing workflow: browser research, LCSC sourcing, library integrity,
JLCPCB preparation, failure handling and offline operation.

**Kinds**

- **offline-fixture**: runs from committed fixtures with no network. These run in CI.
- **network-dependent**: needs a live supplier service. **None exists yet.** EAK has no
  network client (see `docs/engineering/sourcing.md`, section 4). When one is added, its
  tests must be marked `#[ignore]` with the reason, so they never run in CI by default.

**Status values**

- **PASS**: the test exists and was executed with the result shown. Last executed
  2026-11-13 on Windows 11, `cargo test --workspace --release`. Linux and macOS: not yet executed.
- **NOT IMPLEMENTED**: no test exists, because the feature does not exist.
- **NOT ON THIS BRANCH**: owned by other work that is not part of this change; status unknown here.

Nothing is PASS unless it was executed. Fixtures are synthetic; see
`crates/eak-manufacturing/tests/fixtures/README.md`.

Test locations: **M** = `crates/eak-manufacturing/tests/sourcing.rs`,
**V** = `crates/eak-validation/src/sourcing.rs` (unit tests),
**E** = `crates/eak-manufacturing/tests/end_to_end.rs`, **C** = `crates/eak-core/src/builtin.rs`.

## 1. Browser research

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| BR-01 | Research browser opens a supplier or datasheet page and records the source | none | network-dependent | NOT IMPLEMENTED (no research browser in the product) |
| BR-02 | Fetched page content is handled as untrusted data | none | offline-fixture | NOT IMPLEMENTED |
| BR-03 | A refused page (access denied, robots exclusion) is reported and not retried around | none | network-dependent | NOT IMPLEMENTED |

The access research recorded in `sourcing.md` was done by hand. It is not a test.

## 2. LCSC sourcing

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| LS-01 | Record carries source URL, retrieval time, SHA-256 and size of the raw response | M `ingest_records_provenance_and_fields_exactly_as_parsed` | offline-fixture | PASS |
| LS-02 | LCSC number, MPN, manufacturer, package, datasheet URL, specs, stock and price are recorded exactly as parsed | M `ingest_records_provenance_and_fields_exactly_as_parsed` | offline-fixture | PASS |
| LS-03 | Stock and price are recorded only when present | M `stock_and_price_are_recorded_only_when_present` | offline-fixture | PASS |
| LS-04 | Absent fields are listed as missing and never invented | M `missing_fields_are_reported_and_never_invented` | offline-fixture | PASS |
| LS-05 | A new record is a search hit; a datasheet naming the MPN makes it datasheet-matched; only a named human reviewer makes it verified | M `datasheet_hash_matching_and_human_verification` | offline-fixture | PASS |
| LS-06 | Datasheet SHA-256 is recorded; replacing the datasheet voids the attestation | M `datasheet_hash_matching_and_human_verification` | offline-fixture | PASS |
| LS-07 | Provenance URL must be https on the provider domain | M `source_url_must_be_https_on_the_provider_domain` | offline-fixture | PASS |
| LS-08 | Live keyword search against LCSC | none | network-dependent | NOT IMPLEMENTED |
| LS-09 | Live product detail, stock and price retrieval | none | network-dependent | NOT IMPLEMENTED |
| LS-10 | Live datasheet retrieval | none | network-dependent | NOT IMPLEMENTED |
| LS-11 | Field mapping checked against a real LCSC response | none | network-dependent | NOT IMPLEMENTED (mapping is unverified) |
| LS-12 | Datasheet content (ratings, pinout, land pattern) compared with the library | none | offline-fixture | NOT IMPLEMENTED |

## 3. Library integrity

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| LI-01 | Built-in library definitions are structurally valid | C `builtin_library_is_structurally_valid` | offline-fixture | PASS |
| LI-02 | Missing MPN, LCSC number, datasheet and ratings are errors; unverified ratings are warnings | V `missing_data_is_reported_and_rows_are_unmapped` | offline-fixture | PASS |
| LI-03 | A mapped part with no record, or with only a search hit, is an error | V `a_mapped_part_without_a_record_or_with_a_search_hit_is_an_error` | offline-fixture | PASS |
| LI-04 | MPN, package and pad-count mismatches between library and record are errors | V `identity_package_and_pad_count_mismatches_are_errors` | offline-fixture | PASS |
| LI-05 | Symbol pin to footprint pad inconsistency is reported; one LCSC number shared by several values is an error | V `symbol_footprint_inconsistency_and_shared_part_numbers_are_reported` | offline-fixture | PASS |
| LI-06 | Package designations are compared textually; aliases are not assumed equal | V `package_designations_are_compared_textually` | offline-fixture | PASS |
| LI-07 | LCSC part number format | V `lcsc_part_number_format` | offline-fixture | PASS |
| LI-08 | Selecting a record into the library goes through the command engine, leaves ratings, pins and pads unchanged, and refuses to replace an existing identity | M `lcsc_selection_reaches_the_library_bom_and_report_through_commands` | offline-fixture | PASS |

## 4. JLCPCB preparation

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| JP-01 | Gerber, drill, BOM and CPL are generated, self-validated and re-verified from disk | E `reference_design_end_to_end` | offline-fixture | PASS |
| JP-02 | BOM rows map to LCSC part numbers and unmapped rows are listed | M `lcsc_selection_reaches_the_library_bom_and_report_through_commands`, V `missing_data_is_reported_and_rows_are_unmapped` | offline-fixture | PASS |
| JP-03 | Board dimensions, layer count, thickness and copper weight checked against a JLCPCB profile | none here | offline-fixture | NOT ON THIS BRANCH |
| JP-04 | Assembly compatibility and listing of unsupported parts | none here | offline-fixture | NOT ON THIS BRANCH |
| JP-05 | DFM and readiness report | none here | offline-fixture | NOT ON THIS BRANCH |
| JP-06 | Live JLCPCB assembly-library availability | none | network-dependent | NOT IMPLEMENTED |

JP-01 shows the standard outputs are produced without any supplier data: they serve any fabricator.

## 5. Failure handling

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| FH-01 | Truncated JSON, an HTML error page, an empty body, invalid UTF-8, an out-of-range number and a nesting bomb are rejected | M `malformed_oversized_and_error_responses_are_rejected` | offline-fixture | PASS |
| FH-02 | A provider error envelope and unexpected document shapes are rejected | M `malformed_oversized_and_error_responses_are_rejected` | offline-fixture | PASS |
| FH-03 | An oversized response is rejected | M `malformed_oversized_and_error_responses_are_rejected` | offline-fixture | PASS |
| FH-04 | Hostile input (injection text, control characters, wrong types, `javascript:` URL, self-certification keys) is stored as inert data or rejected and cannot raise the state | M `hostile_input_is_treated_as_data_and_cannot_raise_the_state` | offline-fixture | PASS |
| FH-05 | A non-PDF datasheet body and a non-https datasheet URL are refused | M `datasheet_hash_matching_and_human_verification` | offline-fixture | PASS |
| FH-06 | Agents, CLI actors and unnamed reviewers cannot verify a part; a wrong datasheet hash is refused | M `datasheet_hash_matching_and_human_verification` | offline-fixture | PASS |
| FH-07 | A stored record claiming `verified` without evidence loads as a search hit; a corrupt record file is an error | M `records_work_offline_and_a_stored_state_claim_is_not_trusted` | offline-fixture | PASS |
| FH-08 | Network failure, timeout, rate limiting and refusal by the supplier | none | network-dependent | NOT IMPLEMENTED |

## 6. Offline operation

| ID | What is tested | Test | Kind | Status |
|---|---|---|---|---|
| OF-01 | Records are stored, reloaded and searched with no network | M `records_work_offline_and_a_stored_state_claim_is_not_trusted` | offline-fixture | PASS |
| OF-02 | The `SOURCING` report runs from stored records alone and is bound to the project revision | M `lcsc_selection_reaches_the_library_bom_and_report_through_commands` | offline-fixture | PASS |
| OF-03 | The whole test suite runs without network access | all of the above | offline-fixture | PASS (no network client exists in the dependency tree) |
| OF-04 | Behaviour when cached records are stale | none | offline-fixture | NOT IMPLEMENTED (no freshness policy yet) |
