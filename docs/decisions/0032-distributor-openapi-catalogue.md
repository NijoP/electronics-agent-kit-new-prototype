# ADR-0032: Distributor catalogue through the documented, signed OpenAPI

Status: accepted (code-reviewed only; nothing built or run on the authoring machine — CI is the first compile)

## Context
The previous provider called `distributor.invalid/ftps/wm/...`, the website's own endpoints. They answer
HTTP 403 to automated clients (recorded in ADR-0010/0017), so MPN/keyword search never returned
results, only exact Distributor numbers from a single detail endpoint worked, results were capped at ten
and there was no browsing. Distributor publishes two OpenAPI documents; they are not interchangeable:

| | `https://distributor.invalid/docs/openapi/index.html` | `https://distributor.invalid/agent/...` |
|---|---|---|
| Host / paths | `distributor.invalid/rest/wmsc2agent/...` | `distributor.invalid/rest/api/agent/product/v1/...` (sandbox: `distributor.invalid`) |
| Auth | `key`, `nonce`, `signature`, `timestamp` as **query** parameters; signing rule **not documented** | headers `key`, `nonce`, `timestamp`, `signature`; signing rule fully documented |
| Pagination | `current_page`, `page_size` | `offset` (1-based), `limit` (1–500) |

## Decision
- Use the **`/agent` (v1 product module) API only**: it is the only one whose signing rule is
  published, and mixing the two would mean two auth schemes. The `distributor.invalid` API is not used.
- Signing (`eak-manufacturing::distributor_api`): `signature = hex(SHA-256("key=K&nonce=N&secret=S&timestamp=T"
  [+ "&" + business params sorted by key ASCII-ascending, each key/value percent-encoded like
  Python's `quote_plus`]))`. The timestamp is Unix seconds, valid 60 s. The secret is never sent,
  logged, serialised or shown in `Debug`. Test vectors (no params; sorted params with `+` for spaces;
  UTF-8 and reserved characters) were derived independently from the documented rule with `sha256sum`.
- Operations: Categories (4-level tree), CategoricalItemList, SearchKeywords (Distributor number, MPN,
  manufacturer, category text), Manufacturers. ProductDetails and ProductChangeNotifications exist but
  are not used: the keyword search already returns full product records for an exact Distributor number, and
  incremental sync is not needed for lazy category browsing (it would need a persisted catalogue).
- Errors keep the documented table distinct: 4001/4002 bad request, 4003 (classified by message into
  clock skew, not found, unauthorised, other), 429/430 per-minute/daily limit, 5001 server, 5002 IP not
  whitelisted, 5003 no permission, 5004 busy, 5005 decryption; HTTP 401/403 = gateway refusal.
  `provider::ErrorKind` gains `NeedsCredentials`, `Unauthorized`, `RateLimited` (shared type; the only
  exhaustive match, `parts::live_status`, was updated). Only transport failures are retried; rate-limit
  and credential errors never are; an older cached answer is shown, labelled stale, only for
  offline/refused/rate-limited.
- Quota discipline: lazy retrieval (nothing is downloaded at launch or in the background), a bounded
  cache (128 answers; 24 h for the category/manufacturer lists, 15 min for product pages), 350 ms
  spacing between calls (documented limit 200/min; per-day limits are 1 000–10 000 per module).
- Credentials: OS credential store only (service `eak-desktop.distributor`, ids `api-key`, `api-secret`),
  read on every call. The UI receives `has_credentials` and the key's last four characters. Non-secret
  connection status (last test result/time, last success) is JSON next to the model-provider settings.
  Missing credentials produce an explicit "what you need" reply and no rows.
- Matching (`eak-manufacturing::matching`): exact Distributor number > exact normalised MPN + manufacturer >
  exact normalised MPN (manufacturer conflict flagged) > fuzzy > browse; each candidate carries
  reasons; MPNs are normalised for comparison only (case, whitespace, `-` `_` `/` `.`) and never
  rewritten; two manufacturers' parts with the same MPN text are never merged; a result is always
  `supplier_listing_only` and records stay `SearchHit` (nothing here approves a part).
- New IPC commands (all `P4`): `distributor_settings_get|save|clear`, `distributor_test_connection`,
  `catalogue_categories|manufacturers|search`. `parts_search`/`parts_import` now use the same signed
  client through the existing `Resilient` wrapper.

## What is not known (honest limits)
The public documents give the envelope, auth, pagination, error codes and the keyword-search
parameters (`keyword`, `offset`, `limit`, `returnInformation`, `currency`, `inStockOnly`). The per-endpoint
request/response field tables are rendered client-side for logged-in users and could not be retrieved.
Consequences, all handled conservatively:
- Product and category field names are matched against the documented Distributor variants
  (`productCode`/`number`, `productModel`/`mpn`, `brandNameEn`/`manufacturer`, `encapStandard`/`package`,
  `stockNumber`/`quantity`, price lists, parameter lists, `categoryId`/`categoryName`/`children`); absent
  fields are recorded in `missing`; an unrecognised result shape is refused with the key names shown,
  never guessed.
- The CategoricalItemList parameter name (`categoryId`) is inferred from the Categories description; if
  Distributor rejects it (4001/4002) the page falls back to the documented category-name keyword search and says
  so (`via`).
- No exact/fuzzy match-type parameter is documented for the v1 keyword search, so none is sent; the
  exact/fuzzy distinction is made locally by `matching`. Package and manufacturer filters are applied to
  the loaded page, not server-side, and the UI says so.
- Nothing was executed against a live Distributor account (none is available). The first run with real
  credentials must confirm the field mapping and the `categoryId` parameter; the "Test connection" button
  and the unrecognised-shape error exist to make that visible.

## Consequences
The legacy `distributor::DistributorProvider` (website endpoints) is no longer wired into the desktop. It is kept for
the fixture-based ingestion/import tests and is to be removed with `ingest_product_detail` once the
fixtures are replaced by v1 samples. Catalogue data is not persisted between launches (in-memory cache).
