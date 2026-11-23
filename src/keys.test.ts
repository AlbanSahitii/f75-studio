import { test } from "node:test";
import assert from "node:assert/strict";
import { cap, colourName, describe, fromHex, fromKeyEvent, isFactory, legend, macroName, playbackName, shade, toHex, usageOf } from "./keys.ts";

// Entries as decoded from a real F75 (see src-tauri/fixtures).
const esc = { kind: "key", modifiers: 0, usage: 0x29 } as const;
const altTab = { kind: "key", modifiers: 0x04, usage: 0x2b } as const;
const leftShift = { kind: "key", modifiers: 0x02, usage: 0 } as const;

test("keycap legends are short", () => {
  assert.equal(legend(esc, true), "Esc");
  assert.equal(legend({ kind: "key", modifiers: 0, usage: 0x04 }, true), "A");
  assert.equal(legend(leftShift, true), "⇧");
  assert.equal(legend(leftShift, false), "Shift");
  assert.equal(legend(altTab, true), "⌥ Tab");
  assert.equal(legend({ kind: "fn" }, true), "Fn");
  assert.equal(legend({ kind: "media", usage: 0xe9 }, true), "Vol +");
});

test("descriptions spell actions out for the current platform", () => {
  assert.equal(describe(altTab, true), "Option + Tab");
  assert.equal(describe(altTab, false), "Alt + Tab");
  assert.equal(describe({ kind: "media", usage: 0xcd }, true), "Play / Pause");
  assert.equal(describe({ kind: "system", code: 0x18 }, true), "Windows mode"); // Fn+W
  assert.equal(describe({ kind: "system", code: 0x1a }, true), "Mac mode"); // Fn+E
  assert.equal(describe({ kind: "system", code: 0x1e }, true), "System function 1E");
  assert.equal(describe({ kind: "none" }, true), "Disabled");
  assert.equal(describe({ kind: "other", bytes: [8, 0, 0, 1] }, true), "Custom function 08 00 00 01");
});

// Printed legends of the F75 "Gray Wood" keycaps (US layout), from the product photo.
test("keycaps print like the real F75", () => {
  const key = (usage: number, modifiers = 0) => ({ kind: "key", modifiers, usage }) as const;
  assert.deepEqual(cap(key(0x1e)), { top: "!", main: "1", align: "corner" });
  assert.deepEqual(cap(key(0x2d)), { top: "_", main: "-", align: "corner" });
  assert.deepEqual(cap(key(0x04)), { main: "A", align: "corner", big: true });
  assert.deepEqual(cap(key(0x3a)), { main: "F1", align: "corner" });
  assert.deepEqual(cap(key(0x39)), { main: "Caps Lock", align: "left" });
  assert.deepEqual(cap(key(0, 0x02)), { main: "⇧ Shift", align: "left" });
  assert.deepEqual(cap(key(0, 0x08)), { main: "Win", align: "corner" });
  assert.deepEqual(cap(key(0x28)), { main: "↵ Enter", align: "center" });
  assert.deepEqual(cap(key(0x4c)), { main: "Delete", align: "center" });
  assert.deepEqual(cap(key(0x52)), { main: "↑", align: "center", big: true });
  assert.deepEqual(cap(key(0x2c)), { main: "", align: "center" });
  assert.deepEqual(cap(altTab), { main: "Alt+Tab", align: "center" });
  assert.deepEqual(cap({ kind: "fn" }), { main: "Fn", align: "corner" });
  assert.deepEqual(cap({ kind: "media", usage: 0xe9 }), { main: "Vol +", align: "center" });
  assert.deepEqual(cap({ kind: "none" }), { main: "", align: "center" });
});

test("captured key presses become key actions", () => {
  const press = (code: string, mods: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) =>
    fromKeyEvent({ code, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });
  assert.deepEqual(press("KeyQ"), { kind: "key", modifiers: 0, usage: 0x14 });
  assert.deepEqual(press("Digit4", { metaKey: true, shiftKey: true }), { kind: "key", modifiers: 0x0a, usage: 0x21 });
  assert.deepEqual(press("ShiftLeft", { shiftKey: true }), { kind: "key", modifiers: 0x02, usage: 0 });
  assert.deepEqual(press("AltRight", { altKey: true }), { kind: "key", modifiers: 0x40, usage: 0 });
  assert.equal(press("Unidentified"), null);
});

test("mouse, macro, FN2 and knob actions are named", () => {
  assert.equal(describe({ kind: "mouse", button: 1, repeat: 2, interval: 50 }, true), "Double click");
  assert.equal(legend({ kind: "mouse", button: 9, repeat: 0, interval: 0 }, true), "Scroll ↓");
  assert.equal(describe({ kind: "mouse", button: 6, repeat: 1, interval: 0 }, true), "Mouse button 6");
  const macros = [{ name: "H I", steps: [] }];
  assert.equal(describe({ kind: "macro", mode: 1, repeat: 1, index: 0 }, true, macros), "Macro “H I”");
  assert.equal(describe({ kind: "macro", mode: 1, repeat: 1, index: 3 }, true), "Macro 4");
  assert.equal(legend({ kind: "fn2" }, true), "FN2");
  // KB.ini lists the knob as key 84, "Wheel", with system function 0x1D.
  assert.equal(describe({ kind: "system", code: 0x1d }, true), "Volume knob");
});

test("recorded keys become macro steps with readable names", () => {
  assert.equal(usageOf("KeyA"), 0x04);
  assert.equal(usageOf("ShiftLeft"), 0xe1);
  assert.equal(usageOf("MetaRight"), 0xe7);
  assert.equal(usageOf("Fn"), undefined);
  const press = (code: number, up = false) => ({ kind: "key" as const, up, code, delay: 0 });
  assert.equal(macroName([press(0xe1), press(0x0b), press(0x0b, true), press(0x0c)]), "⇧ H I");
  assert.equal(macroName([]), "Macro");
});

test("macro playback is named for how often it repeats", () => {
  assert.equal(playbackName(1, 1), "Once");
  assert.equal(playbackName(1, 7), "7 times");
  assert.equal(playbackName(2, 1), "Over and over while held");
});

// The factory palette as read from a real F75 (src-tauri/fixtures/palette.bin).
const factory: [number, number, number][] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [255, 0, 255], [0, 255, 255], [255, 255, 255]];

test("colours keep their names until they are changed", () => {
  assert.equal(colourName([0, 0, 255], 2, factory), "Blue");
  assert.equal(colourName([0, 128, 128], 2, factory), "Custom #008080");
  assert.equal(colourName([0, 0, 255], 0, factory), "Custom #0000FF"); // blue in red's slot is custom
  assert.ok(isFactory([255, 0, 0], 0, factory));
  assert.ok(!isFactory([255, 0, 0], 0, []));
  assert.deepEqual(fromHex(toHex([0, 128, 255])), [0, 128, 255]);
});

test("stored colours are drawn tuned, exact, or unlit", () => {
  assert.equal(shade([255, 0, 0], factory), "oklch(64% 0.23 28)");
  assert.equal(shade([0, 128, 128], factory), "rgb(0 128 128)");
  assert.equal(shade([0, 0, 0], factory), "transparent");
});
