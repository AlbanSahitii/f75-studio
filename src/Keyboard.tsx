import type { CSSProperties } from "react";
import type { Effect, KeyAction, Lighting } from "./device.ts";
import { LAYERS, PALETTE, cap, describe, legend, same } from "./keys.ts";
import { HEIGHT, KEYS, TONES, WIDTH } from "./layout.ts";

/** Effects that cycle through colours on their own. */
export const CYCLING = [3, 6, 15, 16, 17, 18];
export const PER_KEY = 21;
const NONE: KeyAction = { kind: "none" };

/** The keyboard's light as drawn on screen: its level, and one colour or the rainbow. */
function glow(lighting: Lighting | null, effect: Effect | undefined, palette: string[] | null) {
  if (!lighting || !effect || effect.code === 0) return { level: 0, hue: "fixed", colour: PALETTE[6] };
  if (effect.code === PER_KEY) return { level: 0.9, hue: "fixed", colour: PALETTE[6] };
  const fixedColour = effect.colour && lighting.colour < 7 && !CYCLING.includes(effect.code);
  return {
    level: effect.brightness ? (lighting.brightness + 1) / 10 : 0.5,
    hue: fixedColour ? "fixed" : "cycle",
    colour: fixedColour ? (palette?.[lighting.colour] ?? PALETTE[lighting.colour]) : PALETTE[6],
  };
}

type Props = {
  /** Actions on the layer being shown; null when no keyboard is connected. */
  actions: KeyAction[] | null;
  /** The default layer, used to name keys and to draw keys without an Fn function. */
  base: KeyAction[];
  /** The shown layer as first read this session, to mark remapped keys. */
  original: KeyAction[] | null;
  lighting: Lighting | null;
  effect: Effect | undefined;
  /** The current effect's seven colours, as CSS colours. */
  palette: string[] | null;
  /** Each key's light (CSS colours, by matrix slot), when the effect is Per-key. */
  keyLights: string[] | null;
  iso: boolean;
  mac: boolean;
  /** 0 default, 1 Fn, 2 FN2, 3 Tap. */
  layer: number;
  selected: number | null;
  onSelect: (index: number) => void;
};

/** The AULA F75 ("Gray Wood" colourway), drawn from its real geometry, lit like the real board. */
export function Keyboard(props: Props) {
  const { actions, base, original, lighting, effect, palette, keyLights, iso, mac, layer, selected, onSelect } = props;
  const g = glow(lighting, effect, palette);
  const perKey = effect?.code === PER_KEY && keyLights;
  const fnLayer = layer > 0;
  const style = { "--level": g.level, "--fixed": g.colour } as CSSProperties;
  const layout = KEYS.filter((k) => !k.only || k.only === (iso ? "iso" : "ansi"));
  const pct = (v: number, of: number) => `${(v / of) * 100}%`;

  return (
    <div className="stage" data-ghost={!actions || undefined} style={style}>
      <div className="f75" data-hue={g.hue}>
        <span className="side-led" aria-hidden />
        <div className="frame">
          <div className="keys" style={{ aspectRatio: `${WIDTH} / ${HEIGHT}` }}>
            <span
              className="indicator"
              aria-hidden
              style={{ left: pct(1.43, WIDTH), top: pct(0.14, HEIGHT), width: pct(0.14, WIDTH), height: pct(0.72, HEIGHT) }}
            />
            {layout.map((k) => {
              const action = actions?.[k.index] ?? NONE;
              const own = base[k.index] ?? NONE;
              const blank = fnLayer && action.kind === "none";
              const printed = k.knob ? cap(NONE) : cap(blank || !actions ? own : action);
              const remapped = !!actions && !fnLayer && !!original && !same(action, original[k.index] ?? NONE);
              const name = legend(own, mac) || "Key";
              const keyStyle = {
                left: pct(k.x, WIDTH),
                top: pct(k.y, HEIGHT),
                width: pct(k.w ?? 1, WIDTH),
                height: pct(k.h ?? 1, HEIGHT),
                "--row": Math.round(k.y),
                "--kx": k.x + (k.w ?? 1) / 2,
                "--key-light": perKey ? keyLights[k.index] : undefined,
              } as CSSProperties;
              return (
                <button
                  key={`${k.index}-${k.x}`}
                  className="cap"
                  data-tone={TONES[k.index] ?? "alpha"}
                  data-knob={k.knob || undefined}
                  data-iso-enter={k.h === 2 || undefined}
                  data-align={printed.align}
                  data-big={printed.big}
                  data-dim={blank || undefined}
                  data-remapped={remapped || undefined}
                  style={keyStyle}
                  disabled={!actions}
                  aria-pressed={selected === k.index}
                  aria-label={actions ? `${LAYERS[layer].prefix}${name}: ${describe(action, mac)}` : undefined}
                  onClick={() => onSelect(k.index)}
                >
                  <span className="seam" aria-hidden />
                  <span className="body">
                    <span className="face">
                      {printed.top && <span className="sub">{printed.top}</span>}
                      <span className="main">{printed.main}</span>
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
