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

# M4.5 (ADR-0062): the DESK rule catalogue, its references and the component families, carried as
# knowledge with DESK's own evidence state. Nothing here is an EAK limit or a verified fact.
desk="${1}"
commit="$(git -C "$desk" rev-parse HEAD 2>/dev/null || echo unknown)"
kout="$(dirname "$0")/../crates/eak-intelligence/data/desk_knowledge.json"
jq -n --arg commit "$commit" \
  --slurpfile rules <(jq -s '[.[].rules[]] | sort_by(.rule_id)' "$src"/rule_sets/*.json) \
  --slurpfile refs "$src/references/references.json" \
  --slurpfile fam "$src/component_families/families.json" '
{
  source: "https://github.com/NijoP/DESK library/pcb_domains (MIT; reference data re-expressed for EAK)",
  licence: "MIT",
  desk_commit: $commit,
  rules: [$rules[0][] | {
    rule_id, name, version, domain, category, scope, severity, applicability,
    required_inputs: (.required_inputs // []),
    evaluation_method: (.evaluation_method // ""),
    parameters: (.parameters // {}),
    source_references: (.source_references // []),
    verification_method: (.verification_method // ""),
    status, enforcement,
    human_review_required: (.human_review_required // false),
    rationale: (.rationale // "")
  }],
  references: [$refs[0].references[] | {id, title, organisation, kind, clause_notes: (.clause_notes // ""),
    url: (.url // ""), derived_values_used: (.derived_values_used // []), verification_status,
    copyrighted_text_stored}] | sort_by(.id),
  families: [$fam[0].families[] | {family_id, name, group, description,
    required_parameters: (.required_parameters // []),
    datasheet_verification: (.datasheet_verification // []),
    relevant_archetypes: (.relevant_archetypes // []),
    aliases: (.aliases // [])}] | sort_by(.family_id)
}' > "$kout"
echo "wrote $kout"
