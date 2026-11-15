# Third-party reuse review

Not legal advice; a working record. Status: static-review only. **No third-party code, prompts or
library content has been copied into EAK by this work.** Decisions below are provisional until the
product owner confirms the distribution model.

| Source | Licence (as read) | Decision |
|---|---|---|
| KiCad application source (`KiCad/kicad-source-mirror` @ `52b43fb`) | GPL-3.0 (GitHub metadata) | No code reuse. Independent implementation from documented concepts and file formats only. |
| `kicad-symbols`, `kicad-footprints`, `kicad-packages3D` (commits in `docs/research/KICAD_LIBRARY_ARCHITECTURE_AUDIT.md`) | CC-BY-SA 4.0 with a design-use exception (`LICENSE.md` read in each) | Do not bundle in EAK releases without a separate decision (share-alike on the library content). User-supplied imports keep their licence in the library manifest. |
| Weft (`WeaveMindAI/weft` @ `61fdacd`) | O'Saasy: MIT-style grant, plus clause 2 forbidding use to compete with the licensor by offering the software as a hosted/SaaS product whose primary value is the software itself (`LICENSE` read) | No code, catalogue, prompt or doc-text reuse. Ideas (typed ports, append-only journal, invalidation) are re-implemented independently. If EAK ever offers a hosted service, re-review. |

Rules for any future reuse: record source URL, commit, licence text and copyright in
`third_party/licenses/`, and add an ADR for the decision.
