# F75 Studio

Open-source configurator for the AULA F75 keyboard. Tauri 2: Rust core in `src-tauri/`, React + TS UI in `src/`.

- Structure, and the rules for where code goes: `ARCHITECTURE.md`.
- Shipped protocol facts: the module docs and fixture tests in `src-tauri/src/protocol.rs`. Research notes (`docs/`) stay local and are gitignored.
- Unknown or unverified bytes go through the `f75-protocol` skill (REA-driven). User-facing settings go through the `f75-feature` skill.
- UI work: read `.impeccable.md` and use the impeccable skills. Check the design in the browser preview (`npm run dev`).
- Checks: `npm test`, `npx tsc --noEmit`, `cargo test` and `cargo clippy --all-targets` in `src-tauri/`. `cargo test -- --ignored` talks to a connected F75 (read-only).
- Licensing: port code only from MIT/compatible sources, with attribution. Unlicensed repos are for facts only. Vendor binaries and anything extracted from them stay out of git.
