import type { KeyAction, Macro, Rgb, Step } from "./device.ts";

/** Modifier bits as stored on the keyboard: left Ctrl, Shift, Alt, GUI, then the same four on the right. */
const MODIFIERS = [
  { symbol: "⌃", mac: "Control", pc: "Ctrl" },
  { symbol: "⇧", mac: "Shift", pc: "Shift" },
  { symbol: "⌥", mac: "Option", pc: "Alt" },
  { symbol: "⌘", mac: "Command", pc: "Win" },
];

/** Standard HID keyboard usages → names, with short keycap legends where the name is long. */
const NAMES: Record<number, [name: string, short?: string]> = {
  0x28: ["Enter"], 0x29: ["Escape", "Esc"], 0x2a: ["Backspace"], 0x2b: ["Tab"], 0x2c: ["Space"],
  0x2d: ["-"], 0x2e: ["="], 0x2f: ["["], 0x30: ["]"], 0x31: ["\\"], 0x32: ["#"], 0x33: [";"], 0x34: ["'"],
  0x35: ["`"], 0x36: [","], 0x37: ["."], 0x38: ["/"], 0x39: ["Caps Lock", "Caps"],
  0x46: ["Print Screen", "PrtSc"], 0x47: ["Scroll Lock", "ScrLk"], 0x48: ["Pause"], 0x49: ["Insert", "Ins"],
  0x4a: ["Home"], 0x4b: ["Page Up", "PgUp"], 0x4c: ["Delete", "Del"], 0x4d: ["End"], 0x4e: ["Page Down", "PgDn"],
  0x4f: ["Right arrow", "→"], 0x50: ["Left arrow", "←"], 0x51: ["Down arrow", "↓"], 0x52: ["Up arrow", "↑"],
  0x53: ["Num Lock", "Num"], 0x64: ["ISO key", "< >"], 0x65: ["Menu"],
};
for (let i = 0; i < 26; i++) NAMES[0x04 + i] = [String.fromCharCode(65 + i)];
for (let i = 0; i < 10; i++) NAMES[0x1e + i] = [String((i + 1) % 10)];
for (let i = 0; i < 12; i++) NAMES[0x3a + i] = [`F${i + 1}`];
for (let i = 0; i < 12; i++) NAMES[0x68 + i] = [`F${i + 13}`];

export const MEDIA: Record<number, [name: string, short: string]> = {
  0xcd: ["Play / Pause", "Play"], 0xb5: ["Next track", "Next"], 0xb6: ["Previous track", "Prev"],
  0xb7: ["Stop", "Stop"], 0xe2: ["Mute", "Mute"], 0xe9: ["Volume up", "Vol +"], 0xea: ["Volume down", "Vol −"],
  0x18a: ["Mail", "Mail"], 0x192: ["Calculator", "Calc"], 0x194: ["My computer", "PC"],
  0x223: ["Browser home", "Home"], 0x22a: ["Favourites", "Fav"], 0x183: ["Media player", "Player"],
};

/** Only codes whose meaning is known; the rest show their number. */
export const SYSTEM: Record<number, [name: string, short: string]> = {
  0x01: ["Windows key lock", "Win lock"], 0x18: ["Windows mode", "Win"], 0x19: ["Android mode", "Android"],
  0x1a: ["Mac mode", "Mac"],
  0x1d: ["Volume knob", "Knob"],
};

/** Mouse actions as the vendor app writes them (button 1 left, 2 right, 3 middle, 4 forward, 5 back, 8/9 wheel). */
export const MOUSE: [name: string, short: string, action: KeyAction][] = [
  ["Left click", "Click", { kind: "mouse", button: 1, repeat: 1, interval: 0 }],
  ["Right click", "R-click", { kind: "mouse", button: 2, repeat: 1, interval: 0 }],
  ["Middle click", "M-click", { kind: "mouse", button: 3, repeat: 1, interval: 0 }],
  ["Double click", "2× click", { kind: "mouse", button: 1, repeat: 2, interval: 50 }],
  ["Back", "Back", { kind: "mouse", button: 5, repeat: 1, interval: 0 }],
  ["Forward", "Fwd", { kind: "mouse", button: 4, repeat: 1, interval: 0 }],
  ["Scroll up", "Scroll ↑", { kind: "mouse", button: 8, repeat: 0, interval: 0 }],
  ["Scroll down", "Scroll ↓", { kind: "mouse", button: 9, repeat: 0, interval: 0 }],
];

/** How a macro key can play its macro, as stored on the key. */
export const PLAYBACK: { mode: number; repeat: number; name: string }[] = [
  { mode: 1, repeat: 1, name: "Once" },
  { mode: 1, repeat: 2, name: "Twice" },
  { mode: 1, repeat: 3, name: "3 times" },
  { mode: 1, repeat: 5, name: "5 times" },
  { mode: 1, repeat: 10, name: "10 times" },
  { mode: 2, repeat: 1, name: "Over and over while held" },
  { mode: 4, repeat: 1, name: "Over and over until another key" },
];

export function playbackName(mode: number, repeat: number): string {
  return PLAYBACK.find((p) => p.mode === mode && p.repeat === repeat)?.name ?? (mode === 1 ? `${repeat} times` : `Mode ${mode}`);
}

/** The key layers, and how a key on each is announced. */
export const LAYERS = [
  { name: "Default", prefix: "" },
  { name: "Fn", prefix: "Fn + " },
  { name: "FN2", prefix: "FN2 + " },
  { name: "Tap", prefix: "Tap " },
];

/** The keyboard's seven palette colours, tuned for screens, in factory order. */
export const PALETTE = [
  "oklch(64% 0.23 28)",
  "oklch(80% 0.22 145)",
  "oklch(62% 0.21 262)",
  "oklch(89% 0.17 100)",
  "oklch(65% 0.27 330)",
  "oklch(85% 0.13 205)",
  "oklch(97% 0.01 90)",
];
export const COLOUR_NAMES = ["Red", "Green", "Blue", "Yellow", "Magenta", "Cyan", "White", "Colourful"];

export const toHex = (c: Rgb) => `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
export const fromHex = (h: string): Rgb => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
const sameRgb = (a: Rgb, b: Rgb) => a.every((v, i) => v === b[i]);

/** Whether palette slot `slot` still holds its factory colour. */
export const isFactory = (c: Rgb, slot: number, factory: Rgb[]) => !!factory[slot] && sameRgb(c, factory[slot]);

/** A stored colour as drawn on screen: factory colours use the tuned palette, black is unlit. */
export function shade(c: Rgb, factory: Rgb[]): string {
  const i = factory.findIndex((f) => sameRgb(c, f));
  if (i >= 0) return PALETTE[i];
  return c.every((v) => v === 0) ? "transparent" : `rgb(${c.join(" ")})`;
}

/** A palette slot's name, or its hex value once it was changed. */
export const colourName = (c: Rgb, slot: number, factory: Rgb[]) =>
  isFactory(c, slot, factory) ? COLOUR_NAMES[slot] : `Custom ${toHex(c).toUpperCase()}`;

export const LIGHTING: Record<string, [name: string, short: string]> = {
  "0,0": ["Next lighting effect", "Effect"],
  "3,1": ["Brightness up", "Light +"], "3,2": ["Brightness down", "Light −"],
  "4,1": ["Effect speed up", "Speed +"], "4,2": ["Effect speed down", "Speed −"],
};

const hex = (n: number) => n.toString(16).padStart(2, "0").toUpperCase();
const mouse = (a: KeyAction) => MOUSE.find(([, , m]) => same(m, a));

function modifiers(bits: number, mac: boolean): string[] {
  return MODIFIERS.filter((_, i) => bits & (0x11 << i)).map((m) => (mac ? m.mac : m.pc));
}

/** Short text for a keycap. */
export function legend(a: KeyAction, mac: boolean): string {
  switch (a.kind) {
    case "none":
      return "";
    case "key": {
      const mods = MODIFIERS.filter((_, i) => a.modifiers & (0x11 << i)).map((m) => (mac ? m.symbol : m.pc));
      const [name, short] = NAMES[a.usage] ?? [`Key ${hex(a.usage)}`];
      if (!a.usage) return mods.join(mac ? "" : "+");
      return mods.length ? `${mods.join(mac ? "" : "+")} ${short ?? name}` : (short ?? name);
    }
    case "mouse":
      return mouse(a)?.[1] ?? "Mouse";
    case "media":
      return MEDIA[a.usage]?.[1] ?? "Media";
    case "macro":
      return "Macro";
    case "system":
      return SYSTEM[a.code]?.[1] ?? "Sys";
    case "lighting":
      return LIGHTING[`${a.action},${a.value}`]?.[1] ?? "Light";
    case "fn":
      return "Fn";
    case "fn2":
      return "FN2";
    case "other":
      return "•••";
  }
}

/** Full description for the assignment panel. Macro keys are named from `macros` when given. */
export function describe(a: KeyAction, mac: boolean, macros?: Macro[]): string {
  switch (a.kind) {
    case "none":
      return "Disabled";
    case "key": {
      const parts = modifiers(a.modifiers, mac);
      if (a.usage) parts.push(NAMES[a.usage]?.[0] ?? `Key ${hex(a.usage)}`);
      return parts.join(" + ");
    }
    case "mouse":
      return mouse(a)?.[0] ?? `Mouse button ${a.button}`;
    case "media":
      return MEDIA[a.usage]?.[0] ?? `Media key ${hex(a.usage)}`;
    case "macro": {
      const name = macros?.[a.index]?.name;
      return name ? `Macro “${name}”` : `Macro ${a.index + 1}`;
    }
    case "system":
      return SYSTEM[a.code]?.[0] ?? `System function ${hex(a.code)}`;
    case "lighting":
      return LIGHTING[`${a.action},${a.value}`]?.[0] ?? `Lighting function ${hex(a.action)} ${hex(a.value)}`;
    case "fn":
      return "Fn";
    case "fn2":
      return "FN2 layer";
    case "other":
      return `Custom function ${a.bytes.map(hex).join(" ")}`;
  }
}

const MODIFIER_CODES: Record<string, number> = {
  ControlLeft: 0x01, ShiftLeft: 0x02, AltLeft: 0x04, MetaLeft: 0x08,
  ControlRight: 0x10, ShiftRight: 0x20, AltRight: 0x40, MetaRight: 0x80,
};

const CODES: Record<string, number> = {
  Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c, Minus: 0x2d, Equal: 0x2e,
  BracketLeft: 0x2f, BracketRight: 0x30, Backslash: 0x31, Semicolon: 0x33, Quote: 0x34, Backquote: 0x35,
  Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39, PrintScreen: 0x46, ScrollLock: 0x47, Pause: 0x48,
  Insert: 0x49, Home: 0x4a, PageUp: 0x4b, Delete: 0x4c, End: 0x4d, PageDown: 0x4e, ArrowRight: 0x4f,
  ArrowLeft: 0x50, ArrowDown: 0x51, ArrowUp: 0x52, NumLock: 0x53, IntlBackslash: 0x64, ContextMenu: 0x65,
};
for (let i = 0; i < 26; i++) CODES[`Key${String.fromCharCode(65 + i)}`] = 0x04 + i;
for (let i = 0; i < 10; i++) CODES[`Digit${(i + 1) % 10}`] = 0x1e + i;
for (let i = 0; i < 24; i++) CODES[`F${i + 1}`] = i < 12 ? 0x3a + i : 0x68 + i - 12;

type KeyPress = { code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean };

/** The HID usage a key sends, with modifiers as usages E0-E7, for macro steps. */
export function usageOf(code: string): number | undefined {
  const bit = MODIFIER_CODES[code];
  return bit ? 0xe0 + Math.log2(bit) : CODES[code];
}

/** A macro name from the keys it presses ("H E L L O"). The app shortens it to fit the keyboard. */
export function macroName(steps: Step[]): string {
  const keyLabel = (code: number) =>
    code >= 0xe0 && code <= 0xe7 ? MODIFIERS[(code - 0xe0) % 4].symbol : (NAMES[code]?.[1] ?? NAMES[code]?.[0] ?? "•");
  return steps.filter((s) => s.kind === "key" && !s.up).map((s) => keyLabel(s.code)).join(" ") || "Macro";
}

/** A captured key press as a key action, or null for keys the keyboard can't send. */
export function fromKeyEvent(e: KeyPress): KeyAction | null {
  if (e.code in MODIFIER_CODES) return { kind: "key", modifiers: MODIFIER_CODES[e.code], usage: 0 };
  const usage = CODES[e.code];
  if (usage === undefined) return null;
  const modifiers = (e.ctrlKey ? 0x01 : 0) | (e.shiftKey ? 0x02 : 0) | (e.altKey ? 0x04 : 0) | (e.metaKey ? 0x08 : 0);
  return { kind: "key", modifiers, usage };
}

/** A printed keycap legend: `top` is the shifted symbol on two-legend keys. */
export type Cap = { main: string; top?: string; align: "corner" | "center" | "left"; big?: true };

/** Shifted symbols printed above the main legend on US keycaps. */
const SHIFTED: Record<number, string> = {
  0x1e: "!", 0x1f: "@", 0x20: "#", 0x21: "$", 0x22: "%", 0x23: "^", 0x24: "&", 0x25: "*", 0x26: "(", 0x27: ")",
  0x2d: "_", 0x2e: "+", 0x2f: "{", 0x30: "}", 0x31: "|", 0x33: ":", 0x34: '"', 0x35: "~", 0x36: "<", 0x37: ">",
  0x38: "?",
};

/** Keys printed with words or arrows on the F75, and where the legend sits. */
const PRINTED: Record<number, Cap> = {
  0x28: { main: "↵ Enter", align: "center" },
  0x29: { main: "Esc", align: "center" },
  0x2a: { main: "← Backspace", align: "center" },
  0x2b: { main: "Tab", align: "left" },
  0x2c: { main: "", align: "center" },
  0x39: { main: "Caps Lock", align: "left" },
  0x4c: { main: "Delete", align: "center" },
  0x4f: { main: "→", align: "center", big: true },
  0x50: { main: "←", align: "center", big: true },
  0x51: { main: "↓", align: "center", big: true },
  0x52: { main: "↑", align: "center", big: true },
};

/** Physical keycap names for the modifiers (the F75 prints Windows names). */
const CAP_MODIFIERS = ["Ctrl", "⇧ Shift", "Alt", "Win"];

/** How a key looks printed on an F75 keycap (US layout), for the keyboard render. */
export function cap(a: KeyAction): Cap {
  switch (a.kind) {
    case "key": {
      const mods = CAP_MODIFIERS.filter((_, i) => a.modifiers & (0x11 << i));
      if (!a.usage) {
        if (mods.length !== 1) return { main: mods.map((m) => m.replace("⇧ ", "")).join("+"), align: "center" };
        return { main: mods[0], align: mods[0] === "⇧ Shift" ? "left" : "corner" };
      }
      const [name, short] = NAMES[a.usage] ?? [`Key ${hex(a.usage)}`];
      if (mods.length) return { main: [...mods.map((m) => m.replace("⇧ ", "")), short ?? name].join("+"), align: "center" };
      if (PRINTED[a.usage]) return PRINTED[a.usage];
      if (a.usage >= 0x04 && a.usage <= 0x1d) return { main: name, align: "corner", big: true };
      if (SHIFTED[a.usage]) return { top: SHIFTED[a.usage], main: name, align: "corner" };
      if (/^F\d+$/.test(name)) return { main: name, align: "corner" };
      return { main: short ?? name, align: "center" };
    }
    case "fn":
      return { main: "Fn", align: "corner" };
    case "none":
      return { main: "", align: "center" };
    default:
      return { main: legend(a, false), align: "center" };
  }
}

export const same = (a: KeyAction, b: KeyAction) => JSON.stringify(a) === JSON.stringify(b);
