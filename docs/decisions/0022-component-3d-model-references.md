# ADR-0022: Typed 3D model reference on library components

Status: accepted (code-reviewed only; nothing built or run)

## Context
Component bodies were boxes from courtyard and `body_height`; `Model3d.external_ref` was an
untyped string nothing read. The directive asks for real physical models, honest quality states,
one ECAD-to-viewer transform and lazy loading, without a second design representation or a new CAD stack.

## Decision
- Shared-contract change (IR): `Model3d` gains `model_ref: Option<ModelRef>` (serde default, omitted
  when absent, so existing projects and fixtures load unchanged). `ModelRef { source: File{path},
  format: step|wrl|glb, quality: Option<VENDOR_EXACT|LIBRARY_VERIFIED|DERIVED_VALIDATED>,
  transform { offset_mm, rotation_deg, scale }, licence, provenance }`. `ComponentDef::validate`
  checks extension/format agreement, finite values, positive scale and non-empty provenance.
- No new `Command`: a model is attached with the existing `UpsertComponentDefinition` (engineering
  impacting, so approval rules apply). The UI and AI use the same command.
- Quality is declared by whoever attaches the model; EAK does not yet verify model geometry against
  the footprint. An absent quality is shown as "not declared", never as exact. A part with no
  model, or whose model failed to load, is shown as MISSING and keeps its approximate box.
- Models are never bundled. The desktop reads the user's file through the `library_model_read` IPC
  (absolute path, model extension only, 64 MiB cap).
- Viewer: the existing three.js viewer. `viewer3d/modelTransform.ts` is the single place that maps
  model space to the board (documented frames, bottom-side flip, KiCad rotation order).
  `viewer3d/modelLoader.ts` lazy-loads occt-import-js (STEP), VRMLLoader and GLTFLoader, caching by
  format+path so unrelated edits never reload models. Footprint offsets are stored in footprint
  coordinates (y down); a KiCad `(offset (xyz ..))` y value must be negated when transcribed.
- Limits: WRL units are the file's own (KiCad .wrl is 0.1 inch; set scale); no collision or
  model-vs-footprint verification yet; no EasyEDA/LCSC model fetching yet.

## Consequences
`eak-intelligence` component schema and `scene3d::Body` carry the reference. CSP gains
`'wasm-unsafe-eval'` for the STEP WASM. Licence decisions: `docs/LICENSES_AND_ATTRIBUTION.md`.
