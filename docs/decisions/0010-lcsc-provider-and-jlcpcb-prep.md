# ADR-0010: LCSC provider and JLCPCB preparation

Status: accepted (2026-11-14). Supplements ADR-0008 (editors over existing commands).

## Context
Outcome 2 needs a part search that brings parts into the library, a BOM, and JLCPCB manufacturing
preparation as a provider separate from LCSC. The owner's DESK repository (Python, reference only) has an
LCSC client that falls back to hard-coded fixtures and a JLCPCB module with mock ordering; neither
behaviour is acceptable here (no fake results, no ordering).

## Decisions
1. **No new `Command`.** Import is `Command::UpsertComponentDefinition` through `Engine::execute` as the
   Human actor with the rendered revision. The library change is therefore undoable and journaled like any
   other edit. No shared contract (IR, `Command`, report types) changes; `SourcingRecord` gains an optional
   `category` that older stored records deserialize without.
2. **LCSC supplies identity, never engineering data.** An import reuses the symbol, footprint and pin map of
   an existing library template (chip resistor, MLCC capacitor or LED in a package the library holds, or an
   exact MPN match). Anything else is refused with the reason; a footprint is never guessed. Ratings stay
   unverified. Record state stays `SearchHit` (the SRC-01 rules apply).
3. **Network boundary.** `eak-manufacturing::lcsc::Fetch` is one blocking GET. The live implementation uses
   `ureq` 3 with rustls (already approved for model providers) behind the `live-http` feature, on by default.
   Tests use a local HTTP server. Only the typed part number or keyword is sent; no design data and no
   credentials. A refusal (HTTP 401/403/429) is surfaced as `LcscError::Blocked`; it is never replaced by
   fixtures. Observed 2026-11-14: part-number lookup answers 200, keyword search answers 403.
4. **Provenance.** Records always carry the canonical `https://wmsc.lcsc.com/...` source URL and the SHA-256 of
   the raw bytes, even when a test server answered.
5. **JLCPCB preparation is a separate module** (`jlcpcb`, with `bom` and `zip`). Output is "PREPARED FOR REVIEW";
   it is generated into a folder the user names, never uploaded, quoted or ordered. The DFM profile carries its
   source URL, version and verification date; a rule the software cannot evaluate is `not_checked`, which is
   never a pass and is counted in the report.
6. **Dependency-free ZIP.** JLCPCB wants one archive of Gerber and drill files; a store-only writer (CRC-32,
   fixed timestamp, reproducible) avoids adding a zip crate. A compressed archive can replace it later.

## Consequences
- Parts outside the template set cannot be imported until a footprint source is decided (EasyEDA is not an
  allowed source of truth; author in the editors, then attach the LCSC number).
- The keyword search stays non-functional while LCSC blocks it; the UI says so.
- JLCPCB's limits and rotation conventions change; the profile must be re-verified before relying on it.
