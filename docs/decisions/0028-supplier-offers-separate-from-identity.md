# ADR-0028: Supplier offers are separate from component identity; supplier lookups are resilient

Status: accepted (code-reviewed only; nothing built or run)

## Context
Directive part 2 section 7 / 7.2: a component's engineering identity must not be mixed with
supplier-specific identifiers, a supplier listing is not proof of electrical suitability, and the
desktop must stay usable when a supplier is down. `ComponentDef` carried a bare `distributor` id beside
identity fields, and `parts_*` created a fresh un-cached `DistributorProvider` per call, bypassing the
cache/rate-limit/retry/offline layer of `eak-manufacturing::provider`.

## Decision
- Shared-contract change (IR): `ComponentDef` gains `supplier_offers: Vec<SupplierOffer>` (serde
  default, omitted when empty, so existing projects and fixtures load unchanged). `SupplierOffer`
  { supplier, supplier_part, url, stock, price_breaks[{min_quantity, unit_price text, currency}],
  lifecycle (supplier's own wording, `None` = not reported), fetched_at_unix_s, source_sha256 }.
  Identity stays in the existing fields (manufacturer, MPN, category, package via footprint, symbol,
  footprint, pin map, `model3d`, `provenance`).
- Migration: the legacy `distributor` field is kept and still serialised. `ComponentDef::supplier_part`
  reads the offer first and falls back to `distributor`; `migrate_offers` (idempotent) turns a bare legacy id
  into a bare offer; `set_offer` keeps the `distributor` mirror in step. `SetComponentProperties` keeps offers
  in step and drops an offer whose part number changed (its stock/price described another part).
  No new `Command`: offers travel inside `UpsertComponentDefinition`.
- `SourcingRecord::offer()` converts a fetched record into an offer; `import_plan` and
  `apply_identity` write it. Offers never feed electrical checks, ERC/DRC or ratings.
- Desktop `parts_search` / `parts_import` go through one process-wide `Resilient<DistributorProvider>`
  (cache, 500 ms spacing, retry with backoff on offline only). A supplier failure is a typed status
  in the IPC payload (`unreachable`, `blocked`, `not_found`, `bad_query`, `error`); an older cached
  answer is shown only as `stale` with its age. Library and stored-record results are returned
  regardless. Failures never produce invented rows.
- Fabricator remains a manufacturing-preparation adapter, not a part-lookup supplier; it has no search.

## Consequences
Older projects load with no offers and still resolve the Distributor id. New imports carry a
timestamped, hash-referenced offer. The cache is per process; persisting it is future work.
Lifecycle is not parsed from Distributor yet (always `None`).
