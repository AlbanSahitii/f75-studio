---
name: f75-feature
description: Build a user-facing F75 Studio feature end to end (protocol → Rust command → UI). Use when adding or changing a setting the user can control, such as lighting, key remap, performance or profiles.
---

A feature is one **vertical slice**: bytes, command, screen, shipped together.

## Steps

1. **Protocol.** The bytes it needs are verified (see `protocol.rs` and its fixtures). If anything is unknown, run the `f75-protocol` skill first.
2. **Encode in `protocol.rs`, test first.** A pure read-modify-write on the block (like `Config::with_lighting`), with a test asserting exactly which bytes change in a real fixture. `cargo test` goes green.
3. **Command in `lib.rs`.** A thin `#[tauri::command(async)]` inside `with_keyboard`: read the block, apply the change, `Keyboard::write` (which verifies by reading back), and return the decoded value. Mirror new types in `src/device.ts`, and regenerate the preview data with `cargo test -- --ignored preview` if the snapshot shape changed.
4. **UI.** Read `.impeccable.md`, then build with the `impeccable:frontend-design` skill. The interaction model follows Logitech Options+: changes apply immediately (optimistic, through `save` in `App.tsx` so Undo works), and the device stays the centre of the screen. Check it in the browser preview, light and dark.
5. **Review.** Run `impeccable:critique` and `impeccable:audit` on the new screen, and fix what they flag at P0/P1.
6. **Hardware check.** Ask the user to try it on the real keyboard and confirm the effect. A slice isn't done until the keyboard responds.

Done when: the fixture test passes, the review shows no P0/P1 issues, and the user has confirmed the change on the keyboard.
