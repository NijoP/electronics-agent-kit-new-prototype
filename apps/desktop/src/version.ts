// The desktop app's version, from its package manifest (kept equal to tauri.conf.json and the
// workspace Cargo.toml by the release process; version.test.ts checks the first two agree).
import pkg from "../package.json";

export const APP_VERSION: string = pkg.version;
