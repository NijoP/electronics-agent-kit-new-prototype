# Licence and attribution audit: EasyEDA ecosystem

Licence status of every repository in the public [EasyEDA organisation](https://github.com/easyeda) as
read on 2026-11-14, and the rules EAK follows when it learns from them. EAK itself is licensed MIT OR
Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSING.md`).

**This document is an engineering record, not legal advice.** Any decision to bundle, link or
redistribute third-party code should be reviewed by the owner before it is made.

## What EAK took

**Nothing.** No source code, data file, schema, font, image, icon or other asset from any EasyEDA
repository was copied into EAK by this Phase 0 work. The research documents cite file paths and API
names and describe behaviour in our own words. `LICENSING.md`'s statement "No third-party source code
has been copied into this repository" remains true.

## Policy for future phases

1. **Learn concepts, implement natively.** EAK re-implements ideas in its own Rust/TypeScript code from
   its own design or from public literature and specifications. This applies even to permissively
   licensed repositories, because most `eext-*` code is glue to the closed EasyEDA runtime and several
   bundles have unclear provenance (see below).
2. **Copyleft and unlicensed code is never reused.** GPL-2.0, GPL-3.0, AGPL-3.0 and repositories without
   a licence file are concept-only references. For GPL-derived algorithms (OpenAutoRouter, Freerouting)
   EAK designs from the published algorithm literature, not from the port, and does not reuse constants
   from it (the OpenAutoRouter port's `NOTICE` says its constants derive from the GPL original).
3. **If permissive code is ever copied** (Apache-2.0, MIT, BSD), the copy keeps its licence text and
   copyright notice, Apache-2.0 `NOTICE` content is carried over, modified files are marked, and the
   item is added to `LICENSING.md` before merge.
4. **External tools stay external.** A GPL program (for example Freerouting) or a simulator is only ever
   run as a separate, optional, user-installed process through `eak-runtime/src/process.rs`; it is not
   linked, bundled or required. Bundling any such program needs a separate licence review.
5. **Data needs provenance too.** Capability tables (JLCPCB limits), stack-up templates or calibration
   data are facts with a source and a date; EAK records them as provenance-tagged data, and only after
   checking the source's terms (the impedance surrogate in `eext-design-rule-generator` was fitted to a
   third-party web calculator; see below).
6. **No EasyEDA branding.** EAK does not use EasyEDA or JLC names, logos, icons, colour schemes or UI
   assets (directive Part 1, item 11; Part 4.2).

## The 37 directive repositories

"Licence file" is what the repository's `LICENSE` file says (read through `gh api
repos/easyeda/<repo>/license` and in the clone). "Reusable in EAK" applies the policy above; even
"yes" means "with attribution, if code were ever copied", and EAK's default is not to copy.

| # | Repository | Licence file | Reusable in EAK | Notes from the inspection |
|---|---|---|---|---|
| 1 | easyeda-api-skill | **none** | No (unlicensed) | `SKILL.md` front matter says `license: MIT`, but there is no licence file and `package.json` has no licence field |
| 2 | easyeda-api-sdk | Apache-2.0 | Yes, with attribution | `package.json` and `extension.json` also say Apache-2.0; the typings it depends on (`@jlceda/pro-api-types`) were not inspected |
| 3 | easyeda-format-skill | MIT ("Copyright (c) 2026 EasyEDA") | Yes, with attribution | JSON Schemas and validator under MIT |
| 4 | easyeda-eprj3-skill | MIT ("Copyright (c) 2026 easyeda-eprj3-skill contributors") | Yes, with attribution | `package.json` has no licence field |
| 5 | easyeda-file-format | **none** | No (unlicensed) | Documentation only |
| 6 | easyeda-documents | **none** (archived) | No (unlicensed) | Vendors `parsedown` with its own MIT licence; content is outdated |
| 7 | extension-dev-skill | **none** | No (unlicensed) | `SKILL.md` front matter claims MIT; no licence file |
| 8 | easyeda-client-cli | **none** | No (unlicensed) | Documentation only; the CLI is closed source |
| 9 | easyeda-viewer | Apache-2.0 | Yes, with attribution | Depends on LeaferJS (`leafer-ui`) |
| 10 | online-gencad-viewer | Apache-2.0 | Yes, with attribution | `dist/` contains a built single-file bundle including LeaferJS |
| 11 | eext-gencad-viewer | Apache-2.0 | Concepts only | Ships a minified build of #10 including third-party code whose licence text is not in the repository |
| 12 | eext-gerber-viewer | Apache-2.0 | Concepts only | The bundled viewer's About dialog says "Open Source License: MIT" and credits tracespace v5 for parsing logic; this conflicts with the repository's Apache-2.0 statement, so provenance is unclear |
| 13 | eext-interactive-html-bom | Apache-2.0 | Concepts only | Viewer derived from InteractiveHtmlBom (via `turbobabr/easyeda-ibom-extension`); embeds Split.js (MIT), LZString and jQuery Foundation notices; upstream licences are not reproduced in a `NOTICE` |
| 14 | eext-ai-library-builder | Apache-2.0 | Concepts only | Vendors minified pdf.js and JSZip (JSZip upstream is MIT or GPL-3.0); per-file licences not verified |
| 15 | eext-ai-symbol-builder | Apache-2.0 | Yes, with attribution | |
| 16 | eext-ai-device-standardization | Apache-2.0 | Concepts only | Vendors minified JSZip |
| 17 | eext-datasheet-helper | Apache-2.0 | Yes, with attribution | Downloads third-party models at runtime (Qwen3-0.6B, granite-docling-258M, multilingual-e5-small, opus-mt), each with its own licence; sends datasheet URLs to the third-party `r.jina.ai` reader |
| 18 | eext-component-box | Apache-2.0 | Yes, with attribution | Vendors SheetJS `xlsx` 0.18.5; the `Hardware/` firmware has no separate licence file |
| 19 | eext-open-auto-router | **GPL-3.0** | **No** | `NOTICE`: port of OpenAutoRouter (GPL-3.0); algorithms and many constants derive from it |
| 20 | eext-freerouting-intergration | Apache-2.0 | Yes, with attribution | Client only; Freerouting itself (GPL-3.0) runs as a separate program the user installs |
| 21 | easyeda-pcb-router | **GPL-3.0** | **No** | `engine/NOTICE.md`: Freerouting, GPL-3.0-or-later; vendors Jetty JARs; contains a Python test |
| 22 | eext-kirouting-integration | **none** | No (no licence file) | `package.json`/`extension.json` say Apache-2.0 and the README says MIT for KiCadRoutingTools, but no licence file exists; requires Python at runtime (excluded from EAK in any case) |
| 23 | eext-deeppcb-autorouter | Apache-2.0 | Yes, with attribution | `NOTICE`: unofficial port of `instadeepai/deeppcb-kicad-plugin` (Apache-2.0); "DeepPCB" is an InstaDeep trademark; the routing is a paid cloud service |
| 24 | eext-diff-pair-assistant | Apache-2.0 | Yes, with attribution | |
| 25 | eext-quick-equal-length | Apache-2.0 | Yes, with attribution | |
| 26 | eext-pad-fanout | Apache-2.0 | Yes, with attribution | |
| 27 | eext-pcb-component-grouping | Apache-2.0 | Yes, with attribution | |
| 28 | eext-balance-copper | Apache-2.0 | Yes, with attribution | |
| 29 | eext-jlc-order-dfm-checker | Apache-2.0 | Code: yes, with attribution. Data: provenance required | Capability numbers come from JLCPCB public process pages; EAK must record source and date and must not treat them as universal rules |
| 30 | eext-design-rule-generator | Apache-2.0 | Code: yes, with attribution. Data: **do not import** | The impedance model is a surrogate fitted to samples of the JLCPCB web calculator; the terms of that source were not checked; the training pipeline references Python and is not in the repository |
| 31 | eext-netlist-explorer | Apache-2.0 | Yes, with attribution | |
| 32 | eext-bom-compare | Apache-2.0 | Yes, with attribution | Depends on `jschardet`, `papaparse`, `xlsx`; test BOM samples have recorded sources (`tests/data/SOURCES.md`) |
| 33 | eext-export-gencad | Apache-2.0 | Yes, with attribution | Includes the GenCAD 1.4 specification (`docs/GenCAD-Specification-v1.4.{md,pdf}`) whose own licence is not stated |
| 34 | eext-simulation-with-ngspice | BSD-3-Clause ("Copyright (c) 2026, chensiyu and contributors"); GitHub reports NOASSERTION | **No** (mixed bundle) | `THIRD_PARTY_NOTICES.md` and `licenses/` (16 texts) list ECharts, zrender, Lucide, JSZip, pako and others. The bundled ngspice 46 is "Modified BSD" with exceptions (`KLU`, `numparam`, `tclspice` LGPL; `osdi` MPL-2.0; `xspice/icm/table` GPLv2-or-newer, and `table.cm` is in the loader's code-model list). The sharedspice wrapper sources in `wasm-build/sharedspice-wrapper/` carry GPL-3.0-or-later headers ("Modified (C) 2026 by EasyEDA & JLC Technology Group"), so the compiled WebAssembly contains GPL code although the repository claims BSD-3-Clause overall (checked against the clone) |
| 35 | eext-simulation-with-ltspice | Apache-2.0 | n/a | README only; LTspice is proprietary freeware |
| 36 | easyeda-simulation-engine | **AGPL-3.0** | **No** | `copyright.txt`: SimulIDE-derived files AGPL-3.0, others GPL-3.0; commits Windows binaries (`ngspice.dll`, MinGW runtime DLLs) |
| 37 | eext-export-design-report | Apache-2.0 | Yes, with attribution | |

**Summary of the 37:** 25 Apache-2.0, 2 MIT, 1 BSD-3-Clause (with copyleft contents), 2 GPL-3.0,
1 AGPL-3.0, 6 without a licence file.

## The other 67 repositories (metadata only)

Not inspected beyond the licence-file lookup. Across all 104 repositories, `gh api
repos/easyeda/<repo>/license` reported: Apache-2.0 78, no licence file 13, MIT 3, GPL-3.0 3, AGPL-3.0 3,
GPL-2.0 2, MulanPSL-2.0 1, NOASSERTION 1 (the BSD-3-Clause ngspice repository above).

Copyleft or unlicensed repositories outside the directive list, which must also be treated as
concept-only if they are ever studied:

| Repository | Licence |
|---|---|
| circuitjs1-easyeda (a fork) | GPL-2.0 |
| eext-circuitjs1-simulator | GPL-2.0 |
| eext-dwg-importer | GPL-3.0 |
| eext-power-net-highlighter | AGPL-3.0 |
| eext-simulation-with-simulide | AGPL-3.0 |
| easyeda-enhanced-schematic-skill, easyeda-eprj3-format, easyeda-file-format-v2, easyeda-netlist-format, easyeda-std-file-format, easyeda-std-i18n, extension-dev-mcp-tools | no licence file |
| eext-signal-absence-detection | MulanPSL-2.0 (permissive, but review its terms before any reuse) |

The full list with metadata is the appendix of the [ecosystem audit](EASYEDA_ECOSYSTEM_AUDIT.md).

## Third-party components that matter for EAK's own roadmap

| Component | Where it came up | Licence as recorded in the inspected material | Consequence for EAK |
|---|---|---|---|
| ngspice | repos 34, 36 | "Modified BSD" core with LGPL, MPL-2.0 and GPLv2+ exceptions (`wasm-lib/NGSPICE-COPYING.txt` in repo 34) | Run a user-installed ngspice as an external process first (plan Phase 7); before bundling or linking, review the upstream ngspice licence of the exact version and exclude or isolate the copyleft parts |
| Freerouting | repos 20, 21 | GPL-3.0-or-later (repo 21 `engine/NOTICE.md`) | Optional external process over DSN/SES only; never bundled or required |
| OpenAutoRouter | repo 19 | GPL-3.0 (repo 19 `NOTICE`) | Concepts from public literature only |
| SimulIDE | repo 36 | AGPL-3.0 (repo 36 `copyright.txt`) | Not used |
| KiCadRoutingTools | repo 22 | MIT per repo 22's README (not verified at the source) | Not used (Python; KiCad is not an EAK engine) |
| tracespace | repo 12 | MIT per the bundled About dialog (not verified at the source) | Concepts only; EAK writes its own Gerber reader |
| InteractiveHtmlBom | repo 13 | not reproduced in repo 13 | Concepts only |
| LeaferJS | repos 9-13 | not inspected | Not used (EAK renders with SVG) |
| JLCPCB capability data | repo 29 | public web pages; terms not checked | Provenance-tagged fabrication-profile data only after checking terms |

## Attribution in EAK

Because nothing was copied, EAK has no attribution obligation from this research. For courtesy and
traceability, the research documents name every repository they learned from with its URL and the
commit inspected. If a later phase copies permissive code, the policy above applies and
`LICENSING.md` is updated in the same pull request.
