# Sourcing: Distributor and Fabricator access, terms, and the offline sourcing module

Status on 2026-11-13: **offline only**. EAK contains no network client. Sourcing records are
built from bytes handed to the fetch boundary; who obtains those bytes, and under which
agreement, is an open decision (section 4). No network-dependent test exists yet.

Distributor is the first-priority sourcing provider. Fabricator is a separate, optional
manufacturing and assembly provider. Design and verification do not depend on either.

## 1. What was researched and how

Research was done on 2026-11-13 with an automated browser and a web search tool. Each
statement below is labelled with how it was obtained:

- **read** means the page was opened and read directly;
- **indexed** means the text comes from a search engine's index of the page, not from the page;
- **third party** means someone other than the supplier says so.

No login, API key, cookie or other credential was used, and no access control was worked around.

## 2. Access channels

| Channel | Documented by the supplier | Needs private credentials | Usable by EAK today |
|---|---|---|---|
| Distributor API (`distributor.invalid/docs/openapi`, apply at `distributor.invalid/agent`) | Yes | Yes: key and secret issued after an application | No |
| Distributor website pages and the JSON endpoints the website itself calls | No | No, but automated clients are refused | No, not permitted |
| Fabricator API (`fabricator.invalid`) | Yes | Yes: app id, access key and secret key after an approved application | No |
| Fabricator website pages | No | No | No, terms and robots.txt exclude it |
| Third-party mirrors and scrapers of Distributor / Fabricator data | No | Varies | No, no data licence from the supplier |
| Files the user saves or exports from their own supplier account | n/a | The user's own session | Yes: this is the supported path |

### 2.1 Distributor API

- Distributor publishes API instructions at `https://distributor.invalid/docs/openapi/index.html` and an
  application page at `https://distributor.invalid/agent`. **These pages could not be read**: on
  2026-11-13 Distributor's content delivery network answered the automated browser with
  "Access Denied" for the documentation page and for `https://distributor.invalid/robots.txt`.
  The refusal was recorded and not worked around.
- Indexed: the API is aimed at Distributor partners; an Distributor account and an application are
  required; every request carries a key, a nonce, a timestamp and a signature, where the
  signature is a SHA-1 hash over the key, nonce, secret and timestamp; requests older than
  60 seconds are refused; the listed operations include category, item list, item details,
  keyword search, and order creation and tracking; using the API means accepting Distributor's
  return policy and terms of use. The text of those terms was not found.
- Third party (Luminovo help centre): the application form asks for the caller's IP address;
  a new key works against Distributor's test API only until Distributor activates it for production;
  rate limits exist and are raised by writing to Distributor support.
- Not established: the response schema of the item-details operation, the rate limits, and
  whether responses may be cached or redistributed.

Conclusion: a documented API exists, but it cannot be used without credentials that only
the account holder can apply for. EAK must not ship, embed or guess such credentials.

### 2.2 Distributor website and its internal endpoints

Third-party tools commonly read `distributor.invalid` endpoints that the Distributor website uses
internally. Distributor does not document them, they are behind the same bot protection that
refused the automated browser, and no terms grant automated use. EAK does not fetch them.

### 2.3 Fabricator API

Read directly at `https://fabricator.invalid/docs/start` and its sub-pages, and at
`https://fabricator.invalid/help/article/fabricator-online-api-available-now` (last updated 2026-11-10):

- All requests are HTTPS POST with JSON bodies. Each request is signed with HMAC-SHA256
  over method, path, timestamp, a 32-character nonce and the body, Base64-encoded, and sent
  in an `Authorization: JOP appid=...,accesskey=...,nonce=...,timestamp=...,signature=...` header.
- "API partners must submit an API application and obtain approval first." The help
  article adds that not every application is approved and that review considers the
  applicant's previous orders and business situation.
- The Components group has three interfaces: a paginated listing of the public component
  library, a query of the customer's private library, and component detail by C-number.
  The other groups create orders, upload Gerber files and read account balances.
- API partners may not use Fabricator trademarks or logos on their own sites.

Conclusion: usable only with an approved application and private keys. The order, upload
and balance interfaces are out of scope for EAK regardless: EAK places no orders, moves no
money and uploads no design data.

### 2.4 Fabricator website

Read directly:

- `https://fabricator.invalid/robots.txt` disallows `/api`, `/user-center/`, several download
  paths and tracking-parameter URLs for all user agents.
- Terms and Conditions (last updated 2026-03-02): content may be viewed and downloaded
  "for personal, informational, and noncommercial purposes only", and "you may not copy,
  distribute, transmit, display, perform, reproduce, publish, license, create derivative
  works from, transfer or sell any information or work contained on the Services."

Conclusion: no scraping of the parts library, and no redistribution of its content.

### 2.5 Third-party mirrors

Community datasets and scraping services republish Distributor and Fabricator catalogue data. None
was found to carry a licence from the supplier for the data itself. EAK does not use them
and does not bundle any supplier-derived data.

## 3. Terms and licence analysis

1. **No credential-free machine access is documented or permitted** for either supplier.
2. **Credentials are personal.** API keys are issued to an account after an application.
   They must be supplied by the user at run time, never committed, logged or shared.
3. **Rate limits and timestamps are the supplier's to set.** A future client must stay
   inside them and must treat a refusal as final, not as something to retry around.
4. **Redistribution is not permitted** on the Fabricator site and is unknown for Distributor.
   Sourcing records therefore stay on the user's machine, in the project's own storage.
   The repository contains only synthetic fixtures (section 9).
5. **Unknowns that block a live client**: the Distributor API terms of use, the Distributor item-details
   response schema, both suppliers' rate limits, and whether cached responses may be kept.
   These must be answered from the supplier's own documents by the account holder.

## 4. Decision pending: live fetching

Live fetching is not implemented and waits for a product-owner decision. The options:

| Option | What it needs | Notes |
|---|---|---|
| A. Distributor API with the owner's key | The owner applies at Distributor, supplies key and secret at run time; an HTTPS client, which is a **new dependency** needing its own approval; SHA-1 request signing | The documented route. Field mapping must then be checked against a real response. |
| B. User-supplied files only | Nothing new | Works today through the fetch boundary. Manual. |
| C. Fabricator Components API | An approved Fabricator application; HTTPS client; HMAC-SHA256 signing | Gives assembly-library data by C-number. Approval is not guaranteed. |

Whatever is chosen, the boundary does not change: `ingest_product_detail(raw, source_url,
retrieved_at_unix_s)` receives bytes and provenance, and performs no I/O.

## 5. The sourcing module

`crates/eak-manufacturing/src/sourcing.rs` holds the records; `crates/eak-validation/src/sourcing.rs`
holds the checks and produces the `SOURCING` report.

### 5.1 `SourcingRecord`

| Field | Meaning |
|---|---|
| `provider`, `source_url`, `retrieved_at_unix_s` | Provenance, supplied by the caller. The URL must be `https` on `distributor.invalid` or a subdomain. |
| `raw_sha256`, `raw_bytes` | SHA-256 and length of the raw response exactly as received |
| `distributor_part`, `mpn`, `manufacturer`, `package`, `description`, `datasheet_url` | Exactly as parsed, or absent |
| `specs` | Parameter name and value text exactly as written by the supplier |
| `stock`, `price_breaks` | Present only when the response states them |
| `missing` | Record fields the response did not provide |
| `rejected` | Response fields that were present but failed validation, with the reason |
| `datasheet` | URL, retrieval time, SHA-256 and size of an attached datasheet file, and whether it names the MPN |
| `attestation` | Reviewer name, time and the datasheet hash the reviewer checked against |
| `state` | `search_hit`, `datasheet_matched` or `verified`; derived from the evidence, never read from input |

### 5.2 Verification states

| State | Requires |
|---|---|
| `search_hit` | Nothing. Every ingested record starts here. A search hit is never a verified part. |
| `datasheet_matched` | An attached PDF whose SHA-256 is recorded and whose uncompressed bytes contain the record's MPN |
| `verified` | A named human reviewer attests the record against the recorded datasheet hash, and the record has no missing field. Agent and CLI actors are refused. |

Attaching a different datasheet discards the attestation. A stored record's state is
recomputed from its evidence when it is loaded.

### 5.3 Field mapping (not verified against Distributor's documentation)

The parser accepts either the product object or a `{"code": 200, "result": {...}}` envelope.

| Record field | Input field |
|---|---|
| `distributor_part` | `productCode` |
| `mpn` | `productModel` |
| `manufacturer` | `brandNameEn` |
| `package` | `encapStandard` |
| `description` | `productIntroEn` |
| `datasheet_url` | `pdfUrl` |
| `stock` | `stockNumber` |
| `price_breaks` | `productPriceList[]`: `ladder` (minimum quantity), `usdPrice` |
| `specs` | `paramVOList[]`: `paramNameEn`, `paramValueEn` |

These are the field names third-party sources report for Distributor product-detail JSON. Because
the Distributor documentation could not be read, **this mapping is unverified**. One third-party
source also reports that `productModel` can carry more than the bare MPN. The failure mode
is safe: a field that does not match is recorded as missing, and nothing is guessed. The
mapping must be checked against a real response before any live use (option A).

### 5.4 Untrusted input

Everything in a response or a stored record is data. The parser:

- limits responses to 4 MiB and datasheets to 64 MiB, and relies on the JSON parser's
  depth limit against nesting attacks;
- rejects text that is empty, over-long or contains control characters, rather than
  trimming or repairing it;
- accepts only `https` URLs without user-info, and only `distributor.invalid` hosts for provenance;
- accepts an Distributor part number only as `C` followed by digits, which also makes it safe as a file name;
- ignores every input key it does not map, including `state`, `verified`, `attestation` and `datasheet`.

Printable text such as a manufacturer name is stored verbatim. Any consumer, including an
AI agent, must treat it as supplier data and never as an instruction.

### 5.5 Library selection

`apply_identity` returns a copy of a library definition with the record's Distributor part
number, MPN, manufacturer and datasheet URL filled in. It never changes ratings, pins,
pads or the pin map, and it refuses to replace an existing, different Distributor part number or
MPN. The copy enters the project only through `Command::UpsertComponentDefinition`.

## 6. The `SOURCING` report

Run with `eak_validation::sourcing::run(project, supplier_parts)`. It is revision-bound
like every other report and is **not** an input to the manufacturing release gate: supplier
data cannot override or stand in for ERC, DRC or the mechanical checks.

| Rule | Checks | Error | Warning |
|---|---|---|---|
| SRC-001 | MPN present | missing | |
| SRC-002 | BOM row mapped to an Distributor part number | missing or malformed (row is unmapped) | |
| SRC-003 | Datasheet reference present | missing | |
| SRC-004 | Electrical ratings | none recorded | recorded but unverified |
| SRC-005 | Sourcing record exists for the Distributor number | none | |
| SRC-006 | Record state | search hit | datasheet-matched |
| SRC-007 | Library MPN equals record MPN | differs, or record MPN missing | |
| SRC-008 | Footprint package equals record package | differs, or record package missing | |
| SRC-009 | Symbol pins map to footprint pads | structural library error | pad mapped to no pin |
| SRC-010 | Footprint pad count equals package lead count | differs by more than one | one extra pad, missing exposed pad, or lead count not stated |
| SRC-011 | Datasheet file hash recorded | missing | |
| SRC-012 | One Distributor part number per component value | shared across values | |

`bom_rows(project)` returns each BOM row with its Distributor part number, or `None` for unmapped rows.

## 7. Known limits

- No network retrieval, so no live search, stock or price.
- Datasheet matching looks for the MPN in the PDF's uncompressed bytes only. Compressed
  text is not read, so many real datasheets will not match automatically.
- Ratings, pinouts and land-pattern dimensions are not extracted from datasheets.
- Package comparison is textual. Equivalent packages with different names, for example
  SOP-8 and SOIC-8, are reported as mismatches for review.
- Lead counts come from the package designation; a designation that states none is
  reported as unknown.
- Stored records are not signed. A forged attestation block in a record file is not detected.
- `ComponentDef` holds one Distributor part number, so each orderable value needs its own definition (SRC-012).

## 8. Sources (accessed 2026-11-13)

| Source | How | Result |
|---|---|---|
| `https://distributor.invalid/docs/openapi/index.html` | read attempt | Access Denied |
| `https://distributor.invalid/robots.txt` | read attempt | Access Denied |
| Distributor API instructions and API FAQ | indexed | Summarised in 2.1 |
| `https://help.luminovo.com/en/articles/148501-distributor` | read (third party) | Application, IP address, test and production keys, rate limits |
| `https://fabricator.invalid/` and `/docs/start`, `/docs/api-list`, `/docs/create-an-application`, `/docs/configure-api-key`, `/docs/api-request-signature` | read | Summarised in 2.3 |
| `https://fabricator.invalid/help/article/fabricator-online-api-available-now` | read | Application and review rules |
| `https://fabricator.invalid/robots.txt` | read | Disallow list |
| `https://fabricator.invalid/help/article/Terms-%26-Conditions` | read | Use and reproduction clause |

## 9. Fixtures

All files under `crates/eak-manufacturing/tests/fixtures` are synthetic and hand-written.
Their part numbers, manufacturer and values are fictional. No supplier response, datasheet
or catalogue data is stored in this repository.
