import { useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { Effect, KeyAction, Lighting, Modes, Performance, Rgb, Snapshot, Step } from "./device.ts";
import { CYCLING, PER_KEY } from "./Keyboard.tsx";
import {
  LAYERS,
  LIGHTING,
  MEDIA,
  MOUSE,
  PLAYBACK,
  SYSTEM,
  colourName,
  describe,
  fromHex,
  fromKeyEvent,
  isFactory,
  legend,
  macroName,
  playbackName,
  same,
  shade,
  toHex,
  usageOf,
} from "./keys.ts";

/** A button that opens a menu anchored beneath it (native popover: light-dismiss and Escape come free). */
function Picker(props: { label: string; value: ReactNode; wide?: boolean; children: (close: () => void) => ReactNode }) {
  const id = useId();
  const anchor = `--p${id.replace(/[^a-zA-Z0-9]/g, "")}`;
  const menu = useRef<HTMLDivElement>(null);
  return (
    <>
      <button className="picker" popoverTarget={id} aria-label={props.label} style={{ anchorName: anchor } as CSSProperties}>
        {props.value}
        <svg className="chevron" viewBox="0 0 10 6" aria-hidden>
          <path d="M1 1l4 4 4-4" />
        </svg>
      </button>
      <div
        id={id}
        ref={menu}
        popover="auto"
        className="menu"
        data-wide={props.wide || undefined}
        style={{ positionAnchor: anchor } as CSSProperties}
      >
        {props.children(() => menu.current?.hidePopover())}
      </div>
    </>
  );
}

/** A stepped slider that shows its value and writes once, when the drag or key press ends. */
function Slider(props: { label: string; value: number; max: number; onChange: (v: number) => void }) {
  const draft = useRef<number | null>(null);
  const [shown, setShown] = useState<number | null>(null);
  const commit = () => {
    if (draft.current !== null && draft.current !== props.value) props.onChange(draft.current);
    draft.current = null;
    setShown(null);
  };
  const value = shown ?? props.value;
  return (
    <div className="row">
      <span className="row-label">{props.label}</span>
      <input
        type="range"
        className="slider"
        min={0}
        max={props.max}
        step={1}
        value={value}
        aria-label={props.label}
        aria-valuetext={`${value + 1} of ${props.max + 1}`}
        style={{ "--pct": `${(value / props.max) * 100}%` } as CSSProperties}
        onChange={(e) => {
          draft.current = Number(e.target.value);
          setShown(draft.current);
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="row-value">{value + 1}</span>
    </div>
  );
}

/** How an effect's mini preview should move. */
function look(e: Effect) {
  if (e.code === 0) return "off";
  if (e.code === 21) return "perkey";
  if (e.code === 1) return "static";
  if (e.code === 2) return "breathe";
  return !e.colour || CYCLING.includes(e.code) ? "cycle" : "shimmer";
}

function Swatch({ effect, colour, snap }: { effect: Effect; colour: number; snap: Snapshot }) {
  const stored = colour < 7 ? snap.palette[effect.code]?.[colour] : undefined;
  const tint = stored && shade(stored, snap.factoryPalette);
  return <span className="fx" data-look={look(effect)} data-rainbow={!tint || undefined} style={{ "--tint": tint } as CSSProperties} />;
}

/** Black reads as "off" on the keyboard, but a swatch still needs to show it. */
const swatchColour = (c: Rgb, factory: Rgb[]) => (shade(c, factory) === "transparent" ? "oklch(18% 0 0)" : shade(c, factory));

/**
 * The system colour picker, behind a small well. Picks are applied when they settle, so dragging through
 * colours doesn't send a stream of writes to the keyboard.
 */
function ColourWell(props: { value: Rgb; label: string; text?: string; live?: boolean; onCommit: (c: Rgb) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const commit = useRef(props.onCommit);
  commit.current = props.onCommit;
  const hex = toHex(props.value);
  useEffect(() => {
    if (input.current && document.activeElement !== input.current) input.current.value = hex;
  }, [hex]);
  useEffect(() => {
    const el = input.current;
    if (!el) return;
    let timer = 0;
    const settle = () => {
      clearTimeout(timer);
      timer = window.setTimeout(() => commit.current(fromHex(el.value)), props.live ? 0 : 250);
    };
    el.addEventListener(props.live ? "input" : "change", settle);
    return () => {
      clearTimeout(timer);
      el.removeEventListener(props.live ? "input" : "change", settle);
    };
  }, [props.live]);
  return (
    <label className="well" title={props.label}>
      <input ref={input} type="color" defaultValue={hex} aria-label={props.label} />
      {props.text && <span>{props.text}</span>}
    </label>
  );
}

/** Painting keys in the Per-key effect. */
export type Paint = {
  colour: Rgb;
  active: boolean;
  onColour: (colour: Rgb) => void;
  onActive: (on: boolean) => void;
  onFill: () => void;
};

type LightingProps = {
  snap: Snapshot;
  onEffect: (effect: number) => void;
  onChange: (l: Lighting) => void;
  onPaletteColour: (slot: number, colour: Rgb) => void;
  paint: Paint;
};

export function LightingSection({ snap, onEffect, onChange, onPaletteColour, paint }: LightingProps) {
  const { lighting, effects, factoryPalette: factory } = snap;
  const effect = effects.find((e) => e.code === lighting.effect);
  const set = (patch: Partial<Lighting>) => onChange({ ...lighting, ...patch });
  const row = snap.palette[lighting.effect] ?? [];
  const selectedColour = lighting.colour < 7 ? row[lighting.colour] : undefined;

  return (
    <section className="section" aria-labelledby="lighting-title">
      <h2 id="lighting-title">Lighting</h2>
      <div className="row">
        <span className="row-label">Effect</span>
        <Picker
          label="Effect"
          wide
          value={
            <>
              {effect && <Swatch effect={effect} colour={lighting.colour} snap={snap} />}
              <span>{effect?.name ?? `Effect ${lighting.effect}`}</span>
            </>
          }
        >
          {(close) => (
            <div className="effects" role="listbox" aria-label="Effects">
              {effects.map((e) => (
                <button
                  key={e.code}
                  role="option"
                  aria-selected={e.code === lighting.effect}
                  className="effect"
                  onClick={() => {
                    onEffect(e.code);
                    close();
                  }}
                >
                  <Swatch effect={e} colour={lighting.colour} snap={snap} />
                  {e.name}
                </button>
              ))}
            </div>
          )}
        </Picker>
      </div>
      {effect?.brightness ? (
        <Slider label="Brightness" value={lighting.brightness} max={9} onChange={(v) => set({ brightness: v })} />
      ) : null}
      {effect?.speed ? <Slider label="Speed" value={lighting.speed} max={4} onChange={(v) => set({ speed: v })} /> : null}
      {effect?.colour ? (
        <>
          <div className="row">
            <span className="row-label">Colour</span>
            <div className="swatches" role="radiogroup" aria-label="Colour">
              {row.map((c, i) => (
                <button
                  key={i}
                  role="radio"
                  aria-checked={lighting.colour === i}
                  aria-label={colourName(c, i, factory)}
                  title={colourName(c, i, factory)}
                  className="swatch"
                  style={{ background: swatchColour(c, factory) }}
                  onClick={() => set({ colour: i })}
                />
              ))}
              <button
                role="radio"
                aria-checked={lighting.colour === 7}
                aria-label="Colourful"
                title="Colourful"
                className="swatch"
                data-colourful
                onClick={() => set({ colour: 7 })}
              />
            </div>
          </div>
          {selectedColour && (
            <div className="row">
              <span className="row-label">Custom</span>
              <div className="paint">
                <ColourWell
                  value={selectedColour}
                  label={`Change ${colourName(selectedColour, lighting.colour, factory)}`}
                  text={toHex(selectedColour).toUpperCase()}
                  onCommit={(c) => onPaletteColour(lighting.colour, c)}
                />
                {!isFactory(selectedColour, lighting.colour, factory) && factory[lighting.colour] && (
                  <button className="text" onClick={() => onPaletteColour(lighting.colour, factory[lighting.colour])}>
                    Reset
                  </button>
                )}
              </div>
            </div>
          )}
        </>
      ) : effect?.code === PER_KEY ? (
        <>
          <div className="row">
            <span className="row-label">Paint</span>
            <div className="paint">
              <ColourWell value={paint.colour} label="Paint colour" text={toHex(paint.colour).toUpperCase()} live onCommit={paint.onColour} />
              <button className={paint.active ? "primary" : "secondary"} aria-pressed={paint.active} onClick={() => paint.onActive(!paint.active)}>
                {paint.active ? "Done" : "Paint keys"}
              </button>
              <button className="text" onClick={paint.onFill}>
                Fill all
              </button>
            </div>
          </div>
          <p className="hint">
            {paint.active ? "Click keys on the keyboard to paint them." : "Pick a colour, then paint single keys or fill the whole board."}
          </p>
        </>
      ) : (
        effect && <p className="hint">{effect.code === 0 ? "The backlight is off." : `${effect.name} picks its own colours.`}</p>
      )}
    </section>
  );
}

type KeyProps = {
  snap: Snapshot;
  original: KeyAction[][] | null;
  layer: number;
  onLayer: (layer: number) => void;
  selected: number | null;
  mac: boolean;
  onAssign: (action: KeyAction) => void;
  onMacro: (steps: Step[]) => void;
};

/** Matrix slot of the volume knob (KB.ini key 84, "Wheel"). */
const KNOB = 84;

/** Records key presses and releases with the time between them, until stopped. */
function useMacroRecorder(active: boolean, onDone: (steps: Step[]) => void) {
  const recorded = useRef<Step[]>([]);
  const [shown, setShown] = useState<Step[]>([]);
  const held = useRef(new Map<string, number>());

  const stop = () => {
    // Keys still down when recording stops are released at the end.
    const ends: Step[] = [...held.current.values()].map((code) => ({ kind: "key", up: true, code, delay: 0 }));
    const all = [...recorded.current, ...ends];
    recorded.current = [];
    held.current.clear();
    setShown([]);
    if (all.length) onDone(all);
  };

  useEffect(() => {
    if (!active) return;
    recorded.current = [];
    held.current.clear();
    setShown([]);
    let last = performance.now();
    const record = (e: KeyboardEvent, up: boolean) => {
      e.preventDefault();
      const code = usageOf(e.code);
      if (e.repeat || code === undefined || (up && !held.current.has(e.code))) return;
      if (up) held.current.delete(e.code);
      else held.current.set(e.code, code);
      const now = performance.now();
      const steps = recorded.current;
      // Each step waits until the next one, so the macro replays with the recorded rhythm.
      if (steps.length) steps[steps.length - 1].delay = Math.round(now - last);
      last = now;
      steps.push({ kind: "key", up, code, delay: 0 });
      setShown([...steps]);
    };
    const down = (e: KeyboardEvent) => record(e, false);
    const up = (e: KeyboardEvent) => record(e, true);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [active]);

  return { steps: shown, stop };
}

export function KeySection({ snap, original, layer, onLayer, selected, mac, onAssign, onMacro }: KeyProps) {
  const [recording, setRecording] = useState<"shortcut" | "macro" | null>(null);
  const macro = useMacroRecorder(recording === "macro", (steps) => {
    setRecording(null);
    onMacro(steps);
  });

  useEffect(() => setRecording(null), [selected, layer]);
  useEffect(() => {
    if (recording !== "shortcut") return;
    // Keys are taken on press. A modifier pressed on its own is held back, and assigned only if it is
    // released before any other key, so the key release that started recording is never captured.
    let lone: KeyAction | null = null;
    const finish = (action: KeyAction) => {
      setRecording(null);
      onAssign(action);
    };
    const onDown = (e: KeyboardEvent) => {
      e.preventDefault();
      const action = fromKeyEvent(e);
      if (action?.kind !== "key") return;
      if (action.usage) finish(action);
      else lone = action;
    };
    const onUp = (e: KeyboardEvent) => {
      e.preventDefault();
      const released = fromKeyEvent(e);
      if (lone && released && same(released, lone)) finish(lone);
    };
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
    };
  }, [recording, onAssign]);

  const base = snap.layers[0];
  const action = selected === null ? null : snap.layers[layer][selected];
  const before = selected === null ? null : original?.[layer]?.[selected];
  const isFnKey = selected !== null && base[selected].kind === "fn";
  const knob = selected === KNOB;
  // Keys are named by what they did when first read, so a remapped key keeps its own name.
  const named = original?.[0] ?? base;
  const keyName = selected === null ? "" : LAYERS[layer].prefix + (knob ? "Knob" : legend(named[selected], mac) || "Key");
  const noneLabel = layer ? `No ${LAYERS[layer].name} function` : "Disable key";
  const current = action && (action.kind === "none" && layer ? noneLabel : describe(action, mac, snap.macros));
  const layerHint = [
    "Click any key on the keyboard to change what it does.",
    "Click any key to change what it does while Fn is held.",
    base.some((a) => a.kind === "fn2")
      ? "Click any key to change what it does while FN2 is held."
      : "Nothing opens FN2 yet: on the Default layer, give a key the FN2 function, then hold it.",
    "Experimental: the F75 keeps a Tap layer, but what it does isn't confirmed yet.",
  ][layer];

  const group = (title: string, items: [string, KeyAction][], close: () => void) => (
    <div className="group" role="group" aria-label={title}>
      <span className="group-title">{title}</span>
      {items.map(([name, a]) => (
        <button
          key={name}
          className="item"
          aria-current={(action && same(action, a)) || undefined}
          onClick={() => {
            onAssign(a);
            close();
          }}
        >
          {name}
        </button>
      ))}
    </div>
  );

  return (
    <section className="section key" aria-labelledby="key-title">
      <div className="section-head">
        <h2 id="key-title">Key</h2>
        <div className="segmented" role="radiogroup" aria-label="Layer">
          {LAYERS.map(({ name }, i) => (
            <button key={name} role="radio" aria-checked={layer === i} onClick={() => onLayer(i)}>
              {name}
            </button>
          ))}
        </div>
      </div>

      {selected === null || !action ? (
        <p className="hint">{layerHint}</p>
      ) : isFnKey && layer === 0 ? (
        <div className="assignment">
          <span className="key-name">{keyName}</span>
          <p className="hint">This key opens the Fn layer, so it stays as it is.</p>
        </div>
      ) : (
        <div className="assignment">
          <span className="key-name">{keyName}</span>
          <span className="key-action">{current}</span>
          {action.kind === "macro" && (
            <div className="row">
              <span className="row-label">Plays</span>
              <Picker label="Macro playback" value={<span>{playbackName(action.mode, action.repeat)}</span>}>
                {(close) => (
                  <div className="list" role="listbox" aria-label="Macro playback">
                    {PLAYBACK.map(({ mode, repeat, name }) => (
                      <button
                        key={name}
                        role="option"
                        aria-selected={action.mode === mode && action.repeat === repeat}
                        className="item"
                        onClick={() => {
                          onAssign({ ...action, mode, repeat });
                          close();
                        }}
                      >
                        {name}
                      </button>
                    ))}
                  </div>
                )}
              </Picker>
            </div>
          )}
          <div className="actions">
            {recording === "shortcut" ? (
              <div className="recording" role="status">
                <span className="dot" aria-hidden />
                Press a key or shortcut
                <button className="text" onClick={() => setRecording(null)}>
                  Cancel
                </button>
              </div>
            ) : recording === "macro" ? (
              <div className="recording" role="status">
                <span className="dot" aria-hidden />
                <span className="typed">{macro.steps.length ? macroName(macro.steps) : "Type your macro"}</span>
                <button className="text" onClick={macro.stop}>
                  Stop
                </button>
                <button className="text" onClick={() => setRecording(null)}>
                  Cancel
                </button>
              </div>
            ) : (
              <>
                <button className="primary" onClick={() => setRecording("shortcut")}>
                  Record shortcut
                </button>
                <button className="secondary" onClick={() => setRecording("macro")}>
                  Record macro
                </button>
                <Picker label="Assign a function" value={<span>Function</span>}>
                  {(close) => (
                    <div className="functions">
                      {group("Media", Object.entries(MEDIA).map(([u, [name]]) => [name, { kind: "media", usage: Number(u) }]), close)}
                      <div className="stack">
                        {group("Mouse", MOUSE.map(([name, , a]) => [name, a]), close)}
                        {group(
                          "Lighting",
                          Object.entries(LIGHTING).map(([k, [name]]) => {
                            const [a, v] = k.split(",").map(Number);
                            return [name, { kind: "lighting", action: a, value: v }];
                          }),
                          close,
                        )}
                      </div>
                      <div className="stack">
                        {group("System", Object.entries(SYSTEM).map(([c, [name]]) => [name, { kind: "system", code: Number(c) }]), close)}
                        {group(
                          "Other",
                          [[noneLabel, { kind: "none" }], ...(layer === 0 ? ([["FN2", { kind: "fn2" }]] as [string, KeyAction][]) : [])],
                          close,
                        )}
                      </div>
                    </div>
                  )}
                </Picker>
              </>
            )}
          </div>
          {knob && layer === 0 && <p className="hint">This changes what pressing the knob does. Turning it works as before.</p>}
          {before && !same(before, action) && (
            <button className="text" onClick={() => onAssign(before)}>
              Revert to {before.kind === "none" ? "nothing" : describe(before, mac, snap.macros)}
            </button>
          )}
        </div>
      )}
    </section>
  );
}

const sleepLabel = (step: number) => (step === 0 ? "Never" : `After ${(step * 30) / 60} min`);

type KeyboardProps = {
  snap: Snapshot;
  iso: boolean;
  onIso: (iso: boolean) => void;
  onChange: (p: Performance) => void;
  onModes: (m: Modes) => void;
  onRestore: () => void;
};

/** The keyboard's own modes, with the Fn shortcut that switches each on the keyboard. Android shows only
 * when the keyboard is already in it; the menu bar offers all three. */
const SYSTEMS: [system: number, name: string, shortcut: string][] = [
  [2, "Mac", "Fn+E"],
  [0, "Windows", "Fn+W"],
  [1, "Android", "Fn+Q"],
];
const KNOB_MODES = ["Volume", "Lighting"];

export function KeyboardSection({ snap, iso, onIso, onChange, onModes, onRestore }: KeyboardProps) {
  const [confirming, setConfirming] = useState(false);
  const p = snap.performance;
  const steps = snap.sleepSteps.includes(p.sleep) ? snap.sleepSteps : [...snap.sleepSteps, p.sleep];

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 4000);
    return () => clearTimeout(t);
  }, [confirming]);

  return (
    <section className="section" aria-labelledby="keyboard-title">
      <h2 id="keyboard-title">Keyboard</h2>
      <div className="row">
        <span className="row-label" id="debounce-label" title="Lower responds faster. Raise it if a key ever types twice.">
          Debounce
        </span>
        <div className="segmented" role="radiogroup" aria-labelledby="debounce-label">
          {[1, 2, 3, 4].map((d) => (
            <button key={d} role="radio" aria-checked={p.debounce === d} onClick={() => onChange({ ...p, debounce: d })}>
              {d}
            </button>
          ))}
        </div>
      </div>
      <div className="row">
        <span className="row-label">Sleep</span>
        <Picker label="Sleep" value={<span>{sleepLabel(p.sleep)}</span>}>
          {(close) => (
            <div className="list" role="listbox" aria-label="Sleep">
              {steps.map((s) => (
                <button
                  key={s}
                  role="option"
                  aria-selected={s === p.sleep}
                  className="item"
                  onClick={() => {
                    onChange({ ...p, sleep: s });
                    close();
                  }}
                >
                  {sleepLabel(s)}
                </button>
              ))}
            </div>
          )}
        </Picker>
      </div>
      <div className="row">
        <span className="row-label" id="system-label" title="Mac swaps the keys next to the space bar to Option and Command">
          System
        </span>
        <div className="segmented" role="radiogroup" aria-labelledby="system-label">
          {SYSTEMS.filter(([system]) => system !== 1 || snap.modes.system === 1).map(([system, name, shortcut]) => (
            <button
              key={name}
              role="radio"
              title={`The same as ${shortcut} on the keyboard`}
              aria-checked={snap.modes.system === system}
              onClick={() => onModes({ ...snap.modes, system })}
            >
              {name}
            </button>
          ))}
        </div>
      </div>
      <div className="row">
        <span className="row-label" id="knob-label" title="A long press on the knob also switches this">
          Knob
        </span>
        <div className="segmented" role="radiogroup" aria-labelledby="knob-label">
          {KNOB_MODES.map((name, knob) => (
            <button key={name} role="radio" aria-checked={snap.modes.knob === knob} onClick={() => onModes({ ...snap.modes, knob })}>
              {name}
            </button>
          ))}
        </div>
      </div>
      <div className="row">
        <span className="row-label">Layout</span>
        <div className="segmented" role="radiogroup" aria-label="Physical layout">
          {["ANSI", "ISO"].map((name) => (
            <button key={name} role="radio" aria-checked={iso === (name === "ISO")} onClick={() => onIso(name === "ISO")}>
              {name}
            </button>
          ))}
        </div>
      </div>
      <button
        className={confirming ? "danger restore" : "text restore"}
        onClick={() => (confirming ? (setConfirming(false), onRestore()) : setConfirming(true))}
      >
        {confirming ? "Click again to restore" : "Restore original settings"}
      </button>
    </section>
  );
}
