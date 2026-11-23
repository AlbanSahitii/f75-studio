# F75 Studio

Open-source configurator for the **AULA F75** keyboard, for macOS first, with Linux and Windows to follow. Change the lighting and per-key colours, remap keys on all four layers, record macros, and tune debounce and sleep, without the Windows-only vendor app.

> Unofficial. Not affiliated with or endorsed by AULA. It writes settings to your keyboard; it saves your original settings on first connect so you can always restore them.

## What works

F75 Studio lives in the **menu bar**. Its native menu changes lighting, debounce and sleep directly on the keyboard. At rest it's one small process (about 13 MB) using no CPU: no window, no web view, no background polling; it reads the keyboard when you open the menu. **Keys, Macros and Colours…** opens the full editor window, and closing it frees it again.

- **Your keyboard on screen:** the F75 drawn from its real geometry in the Gray Wood colourway, with legends read from the keyboard and its current lighting glowing through the seams.
- **Lighting:** 18 effects plus Per-key and Off, with brightness, speed and colour (when the effect supports them). Each effect's seven colours can be changed to any colour.
- **Per-key colours:** paint single keys or fill the board, in any colour.
- **Keys:** remap any key on the Default, Fn, FN2 and Tap layers. Record a key or shortcut, or pick a media, mouse (clicks, back/forward, scroll), lighting or system function. Give a key FN2 to open the FN2 layer. Disable a key, or revert it.
- **Macros:** record keystrokes with their timing onto any key, and play them once, while held, or until another key is pressed.
- **Mac or Windows mode** (the same as Fn+E / Fn+W on the keyboard), and what the **knob** does: volume and mute, or brightness and effects.
- **Performance:** debounce and the battery sleep timer.
- **Safety:** every change is read back from the keyboard to check it stuck, Undo is one click away, and *Restore original settings* puts back what the app found the first time, colours and macros included.
- **Battery** level and charging state.

The keyboard must be connected with **USB-C** to change settings (it shows as `258a:010c`). Settings live on the keyboard, so they keep working over Bluetooth and the 2.4 GHz dongle. Settings are never written over Bluetooth or the dongle: their packets get lost, and a radio write is known to have left an F75 unable to type until its firmware was reflashed.

On macOS, the first launch asks for **Input Monitoring** (System Settings › Privacy & Security › Input Monitoring), which macOS requires of any app that talks to a keyboard. Turn it on for F75 Studio and reopen the app. F75 Studio opens the keyboard in shared mode and never reads your typing.

## Develop

Needs Node 22.18+ and Rust (`rustup`).

```sh
npm install
npm run tauri dev       # the app
npm run dev             # browser preview with a simulated keyboard (?connection=none|wireless for empty states)
npm test                # UI helpers
cd src-tauri && cargo test               # protocol, against bytes captured from a real F75
cd src-tauri && cargo test -- --ignored   # read-only check against a connected F75, and regenerates the preview data
cd src-tauri && F75_WRITE_TEST=1 cargo test -- --ignored writes_and_restores   # writes test values to a connected F75, then restores them
```

macOS ties Input Monitoring to an app's signature. Without a signing identity every build is a new app to macOS and must be allowed again, even when F75 Studio already shows as on. To keep the permission across rebuilds, sign with a certificate trusted for code signing (a self-signed one works): `APPLE_SIGNING_IDENTITY="<certificate name>" npm run tauri build`.

Protocol notes live in [`src-tauri/src/protocol.rs`](src-tauri/src/protocol.rs). The structure is described in [ARCHITECTURE.md](ARCHITECTURE.md).

## Credits

Protocol research was cross-checked against [bakiakyol/aula-f75-hub](https://github.com/bakiakyol/aula-f75-hub) (MIT), which also supplied the macro step kinds (from its factory-macro backup, `src-tauri/fixtures/factory-macro.bin`), the firmware's macro-memory marker, and the meaning of the system and knob mode bytes.

## License

MIT
