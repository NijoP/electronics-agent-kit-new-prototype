# herdr — upstream record

- Repository: https://github.com/herdrdev/herdr
- Pinned commit: `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde` (version 0.9.5, committed 2026-10-29)
- Licence: Apache License 2.0 (`LICENSE` in this directory, copied verbatim). The upstream
  repository has no NOTICE file at this commit.

## What EAK derives from it

| EAK file | Upstream source | Modification |
|---|---|---|
| `crates/eak-terminal/src/agents.rs` | `src/detect/mod.rs` (`lookup_agent`, `is_muse_versioned_binary`, `normalized_agent_lookup_name`, `path_basename`) | Reduced to the agent name/alias table and name normalisation; returns canonical label strings instead of an `Agent` enum. |

No other upstream source is copied. EAK's state derivation, local control API and session
persistence were compared with upstream (`src/detect`, `src/api`, `src/persist`) and kept as
EAK's own implementations, because they already provide the behaviour EAK needs in a smaller
form: process-first status with a labelled source, a token-authenticated loopback API with a
`since` cursor for `wait`, and a versioned layout manifest that restores sessions as `ended`.
No upstream binary is bundled or executed by EAK.
