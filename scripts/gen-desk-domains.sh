#!/usr/bin/env bash
# Regenerate crates/eak-intelligence/data/desk_domains.json from a checkout of the DESK
# library (github.com/NijoP/DESK, MIT, reference only). Needs jq. Usage:
#   scripts/gen-desk-domains.sh /path/to/DESK
set -euo pipefail
src="${1:?path to a DESK checkout}/library/pcb_domains"
out="$(dirname "$0")/../crates/eak-intelligence/data/desk_domains.json"
jq -n --slurpfile tax "$src/taxonomy.json" \
  --slurpfile arch <(jq -s 'sort_by(.archetype_id)' "$src"/archetypes/*.json) \
  --slurpfile ovl <(jq -s 'sort_by(.overlay_id)' "$src"/overlays/*.json) '
{
  source: "https://github.com/NijoP/DESK library/pcb_domains (MIT; reference data re-expressed for EAK)",
  schema_version: $tax[0].schema_version,
  generated: $tax[0].generated,
  domains: [$tax[0].domains[] | {domain_id, name, description}],
  overlays: [$ovl[0][] | {overlay_id, name, description: (.description // ""), applies_when, adds_component_families, precedence}],
  archetypes: [$arch[0][] | {
    archetype_id, name, domain, description,
    intended_applications: (.intended_applications // []),
    typical_overlays: (.typical_overlays // []),
    required_component_roles: (.required_component_roles // []),
    typical_component_families: (.typical_component_families // []),
    critical_electrical_constraints: (.critical_electrical_constraints // []),
    verification_requirements: (.verification_requirements // []),
    applicable_standards: (.applicable_standards // []),
    unsupported_conditions: (.unsupported_conditions // []),
    classification: .classification,
    maturity: {status: .maturity.status, evidence: (.maturity.evidence // ""), rule_ids: (.maturity.rule_ids // [])}
  }]
}' > "$out"
echo "wrote $out"
