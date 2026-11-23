import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  device,
  factoryLayer,
  isPreview,
  type KeyAction,
  type Lighting,
  type Modes,
  type Performance,
  type Rgb,
  type Settings,
  type Snapshot,
  type Status,
  type Step,
} from "./device.ts";
import { Keyboard, PER_KEY } from "./Keyboard.tsx";
import { macroName, shade } from "./keys.ts";
import { KEYS } from "./layout.ts";
import { KeyboardSection, KeySection, LightingSection } from "./panels.tsx";

type Notice = { text: string; undo?: () => void; error?: boolean };

const mac = navigator.userAgent.includes("Mac");
/** Every key on the board that the keyboard keeps a colour for. */
const lit = (colours: number) => [...new Set(KEYS.map((k) => k.index))].filter((i) => i < colours);

/** Groups keys by colour, so each colour is one write. */
function byColour(colours: Map<number, Rgb>) {
  const groups = new Map<string, { keys: number[]; colour: Rgb }>();
  for (const [key, colour] of colours) {
    const group = groups.get(colour.join());
    if (group) group.keys.push(key);
    else groups.set(colour.join(), { keys: [key], colour });
  }
  return [...groups.values()];
}

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [layer, setLayer] = useState(0);
  const [selected, setSelected] = useState<number | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  /** Why the keyboard couldn't be read, until a read succeeds. */
  const [loadError, setLoadError] = useState<string | null>(null);
  const [iso, setIso] = useState(false);
  const [painting, setPainting] = useState(false);
  const [paint, setPaint] = useState<Rgb>([255, 255, 255]);
  const original = useRef<KeyAction[][] | null>(null);
  const snapRef = useRef(snap);
  snapRef.current = snap;

  const loading = useRef(false);
  const load = useCallback(async () => {
    if (loading.current) return;
    loading.current = true;
    try {
      const s = await device.snapshot();
      original.current ??= s.layers;
      setSnap(s);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    } finally {
      loading.current = false;
    }
  }, []);

  // Status polling only while the window is visible, and a re-render only when something changed.
  useEffect(() => {
    let alive = true;
    const tick = () => {
      if (document.hidden) return;
      device.status().then(
        (s) => alive && setStatus((prev) => (JSON.stringify(prev) === JSON.stringify(s) ? prev : s)),
        () => {},
      );
    };
    tick();
    const t = setInterval(tick, 2500);
    document.addEventListener("visibilitychange", tick);
    return () => {
      alive = false;
      clearInterval(t);
      document.removeEventListener("visibilitychange", tick);
    };
  }, []);

  const usb = status?.connection === "usb";
  // Runs on every status poll, so a failed first read keeps being retried.
  useEffect(() => {
    if (!usb) setSnap(null);
    else if (!snapRef.current) load();
  }, [status, usb, load]);

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), notice.error ? 8000 : 5000);
    return () => clearTimeout(t);
  }, [notice]);

  /** Runs a write; on failure re-reads the keyboard so the screen never shows a state it doesn't have. */
  const save = useCallback(
    async (run: () => Promise<unknown>, done: string, undo?: () => void) => {
      try {
        await run();
        setNotice({ text: done, undo });
      } catch (e) {
        setNotice({ text: `Couldn't save: ${e}`, error: true });
        load();
      }
    },
    [load],
  );

  // Optimistic writes: the screen changes at once, and only the newest reply may correct it.
  const lightingSeq = useRef(0);
  const writeLighting = useCallback(
    (shown: Lighting, run: () => Promise<Lighting>, undo?: () => void) => {
      const seq = ++lightingSeq.current;
      setSnap((s) => s && { ...s, lighting: shown });
      const write = async () => {
        const stored = await run();
        if (seq === lightingSeq.current) setSnap((s) => s && { ...s, lighting: stored });
      };
      save(write, "Lighting saved to keyboard", undo);
    },
    [save],
  );

  const changeEffect = useCallback(
    (effect: number) => {
      const before = snapRef.current?.lighting;
      if (!before || before.effect === effect) return;
      writeLighting({ ...before, effect }, () => device.setEffect(effect), () => changeEffect(before.effect));
    },
    [writeLighting],
  );

  const changeLighting = useCallback(
    (next: Lighting) => {
      const before = snapRef.current?.lighting;
      writeLighting(next, () => device.setLighting(next), before && (() => changeLighting(before)));
    },
    [writeLighting],
  );

  const performanceSeq = useRef(0);
  const changePerformance = useCallback(
    (next: Performance) => {
      const before = snapRef.current?.performance;
      const seq = ++performanceSeq.current;
      setSnap((s) => s && { ...s, performance: next });
      const write = async () => {
        const stored = await device.setPerformance(next);
        if (seq === performanceSeq.current) setSnap((s) => s && { ...s, performance: stored });
      };
      save(write, "Saved to keyboard", before && (() => changePerformance(before)));
    },
    [save],
  );

  const modesSeq = useRef(0);
  const changeModes = useCallback(
    (next: Modes) => {
      const before = snapRef.current?.modes;
      const seq = ++modesSeq.current;
      setSnap((s) => s && { ...s, modes: next });
      const write = async () => {
        const stored = await device.setModes(next);
        if (seq === modesSeq.current) setSnap((s) => s && { ...s, modes: stored });
      };
      save(write, "Saved to keyboard", before && (() => changeModes(before)));
    },
    [save],
  );

  const assign = useCallback(
    (l: number, index: number, action: KeyAction) => {
      const before = snapRef.current?.layers[l][index];
      const put = (s: Snapshot, a: KeyAction) => ({
        ...s,
        layers: s.layers.map((acts, i) => (i === l ? acts.map((x, j) => (j === index ? a : x)) : acts)),
      });
      setSnap((s) => s && put(s, action));
      const write = async () => {
        const stored = await device.setKey(l, index, action);
        setSnap((s) => s && put(s, stored));
      };
      save(write, "Key saved to keyboard", before && (() => assign(l, index, before)));
    },
    [save],
  );

  const onAssign = useCallback((a: KeyAction) => selected !== null && assign(layer, selected, a), [assign, layer, selected]);

  const recordMacro = useCallback(
    (steps: Step[]) => {
      if (selected === null) return;
      const [l, index] = [layer, selected];
      const before = snapRef.current?.layers[l][index];
      // A key that played another macro can't go back to it: its number now plays the new one.
      const undo = before && before.kind !== "macro" ? () => assign(l, index, before) : undefined;
      const write = async () => {
        const result = await device.setMacro(l, index, { name: macroName(steps), steps });
        setSnap((s) => s && { ...s, layers: s.layers.map((acts, i) => (i === l ? result.layer : acts)), macros: result.macros });
      };
      save(write, "Macro saved to keyboard", undo);
    },
    [save, assign, layer, selected],
  );

  // Paint writes run one at a time; clicks made meanwhile in the same colour join the next write. Undo
  // puts back every key the finished writes changed.
  const paintQueue = useRef<{ keys: number[]; colour: Rgb }[]>([]);
  const paintBefore = useRef(new Map<number, Rgb>());
  const paintBusy = useRef(false);
  const paintRef = useRef<(keys: number[], colour: Rgb) => void>(() => {});
  const pumpPaint = useCallback(async () => {
    if (paintBusy.current) return;
    paintBusy.current = true;
    try {
      for (let next = paintQueue.current.shift(); next; next = paintQueue.current.shift()) {
        const stored = await device.setKeyColours(next.keys, next.colour);
        if (!paintQueue.current.length) setSnap((s) => s && { ...s, keyColours: stored });
      }
      const before = new Map(paintBefore.current);
      paintBefore.current.clear();
      const undo = () => byColour(before).forEach(({ keys, colour }) => paintRef.current(keys, colour));
      setNotice({ text: "Colours saved to keyboard", undo });
    } catch (e) {
      paintQueue.current = [];
      paintBefore.current.clear();
      setNotice({ text: `Couldn't save: ${e}`, error: true });
      load();
    } finally {
      paintBusy.current = false;
    }
  }, [load]);

  const paintKeys = useCallback(
    (keys: number[], colour: Rgb) => {
      const shown = snapRef.current?.keyColours;
      if (!shown) return;
      for (const k of keys) if (!paintBefore.current.has(k)) paintBefore.current.set(k, shown[k]);
      setSnap((s) => s && { ...s, keyColours: s.keyColours.map((c, i) => (keys.includes(i) ? colour : c)) });
      const queue = paintQueue.current;
      const last = queue[queue.length - 1];
      if (last && last.colour.every((v, i) => v === colour[i])) last.keys.push(...keys);
      else queue.push({ keys: [...keys], colour });
      pumpPaint();
    },
    [pumpPaint],
  );
  paintRef.current = paintKeys;

  const setPaletteColour = useCallback(
    (effect: number, slot: number, colour: Rgb) => {
      const before = snapRef.current?.palette[effect]?.[slot];
      const put = (s: Snapshot, row: (old: Rgb[]) => Rgb[]) => ({ ...s, palette: s.palette.map((r, e) => (e === effect ? row(r) : r)) });
      setSnap((s) => s && put(s, (r) => r.map((c, i) => (i === slot ? colour : c))));
      const write = async () => {
        const stored = await device.setPaletteColour(effect, slot, colour);
        setSnap((s) => s && put(s, () => stored));
      };
      save(write, "Colour saved to keyboard", before && (() => setPaletteColour(effect, slot, before)));
    },
    [save],
  );

  const perKey = snap?.lighting.effect === PER_KEY;
  useEffect(() => {
    if (!perKey) setPainting(false);
  }, [perKey]);

  const restore = useCallback(
    () =>
      save(async () => {
        await device.restoreOriginal();
        await load();
      }, "Original settings restored"),
    [save, load],
  );

  // The layout is an app setting shared with the menu bar; follow changes made there.
  useEffect(() => {
    device.settings().then((s) => setIso(s.iso), () => {});
    if (isPreview) return;
    const off = listen<Settings>("settings-changed", (e) => setIso(e.payload.iso));
    return () => void off.then((unlisten) => unlisten());
  }, []);

  const chooseIso = (value: boolean) => {
    setIso(value);
    device.setLayout(value).catch((e) => setNotice({ text: `Couldn't save: ${e}`, error: true }));
  };

  const effect = snap?.effects.find((e) => e.code === snap.lighting.effect);
  const battery = status?.battery;

  return (
    <div className="app">
      <header className="titlebar" data-tauri-drag-region>
        <h1 data-tauri-drag-region>F75</h1>
        <span className="chip" data-on={usb || undefined} data-tauri-drag-region>
          {status === null ? "Looking…" : usb ? "USB-C" : status.connection === "wireless" ? "Wireless" : "Not connected"}
        </span>
        {battery && <BatteryChip level={battery.level} charging={battery.charging} />}
        <div className="notice" role="status" data-error={notice?.error || undefined}>
          {notice && (
            <>
              <span key={notice.text}>{notice.text}</span>
              {notice.undo && (
                <button
                  className="link"
                  onClick={() => {
                    notice.undo?.();
                    setNotice(null);
                  }}
                >
                  Undo
                </button>
              )}
            </>
          )}
        </div>
        {isPreview && <span className="chip preview">Preview</span>}
      </header>

      <main>
        <section className="hero" aria-label="Keyboard">
          <Keyboard
            actions={snap ? snap.layers[layer] : null}
            base={snap ? snap.layers[0] : factoryLayer}
            original={original.current?.[layer] ?? null}
            lighting={snap?.lighting ?? null}
            effect={effect}
            palette={snap ? (snap.palette[snap.lighting.effect] ?? []).map((c) => shade(c, snap.factoryPalette)) : null}
            keyLights={snap ? snap.keyColours.map((c) => shade(c, snap.factoryPalette)) : null}
            iso={iso}
            mac={mac}
            layer={layer}
            selected={painting ? null : selected}
            onSelect={painting ? (i) => paintKeys([i], paint) : setSelected}
          />
          {!snap && <Connect status={status} error={usb ? (loadError ?? undefined) : undefined} />}
        </section>

        {snap && (
          <section className="controls" aria-label="Settings">
            <LightingSection
              snap={snap}
              onEffect={changeEffect}
              onChange={changeLighting}
              onPaletteColour={(slot, colour) => setPaletteColour(snap.lighting.effect, slot, colour)}
              paint={{
                colour: paint,
                active: painting,
                onColour: setPaint,
                onActive: setPainting,
                onFill: () => paintKeys(lit(snap.keyColours.length), paint),
              }}
            />
            <KeySection
              snap={snap}
              original={original.current}
              layer={layer}
              onLayer={setLayer}
              selected={selected}
              mac={mac}
              onAssign={onAssign}
              onMacro={recordMacro}
            />
            <KeyboardSection
              snap={snap}
              iso={iso}
              onIso={chooseIso}
              onChange={changePerformance}
              onModes={changeModes}
              onRestore={restore}
            />
          </section>
        )}
      </main>
    </div>
  );
}

/** Battery in the title bar. Over the cable the F75 reports charging but not its level (it sends 0). */
function BatteryChip({ level, charging }: { level: number; charging: boolean }) {
  const known = level > 0 && level <= 100;
  const title = known ? undefined : "The F75 doesn't report its battery level over the cable.";
  return (
    <span className="chip battery" title={title} data-tauri-drag-region>
      <span className="cell" aria-hidden style={{ "--level": `${known ? level : 0}%` } as CSSProperties}>
        {charging && (
          <svg viewBox="0 0 8 12">
            <path d="M5 0 0 7h3l-1 5 6-7H5z" />
          </svg>
        )}
      </span>
      {known ? `${level}%` : charging ? "Charging" : "Battery"}
    </span>
  );
}

function Connect({ status, error }: { status: Status | null; error?: string }) {
  const blocked = !!error?.includes("Input Monitoring");
  const [title, body] =
    status === null
      ? ["Looking for your F75…", ""]
      : status.connection === "usb"
        ? blocked
          ? [
              "Allow F75 Studio to use your keyboard",
              "Turn on F75 Studio under Input Monitoring, then reopen the app. If it's already on, macOS is remembering an older copy: remove it with −, add it again, then reopen.",
            ]
          : error
            ? ["Can't read your keyboard", ""]
            : ["Reading your keyboard…", ""]
        : status.connection === "wireless"
          ? [
              "Plug in the USB-C cable",
              "Your F75 is connected wirelessly. Changes are made over the cable, and they stay on the keyboard after you unplug it.",
            ]
          : ["Connect your F75 with USB-C", "F75 Studio saves your settings on the keyboard itself, so they work on any computer."];
  return (
    <div className="connect" role="status">
      <h2>{title}</h2>
      {body && <p>{body}</p>}
      {blocked ? (
        <button className="primary" onClick={() => device.openPrivacySettings()}>
          Open Input Monitoring Settings
        </button>
      ) : (
        error && <p className="error">{error}</p>
      )}
    </div>
  );
}
