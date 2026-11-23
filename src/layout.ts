/**
 * AULA F75 physical layout in key units (1u = one key pitch), measured from the vendor app's key map.
 * `index` is the key-matrix slot; legends come from the keyboard itself, not from this table.
 * The firmware matrix has both ANSI and ISO keys; `only` marks keys that exist on one of them.
 */
export type KeyGeometry = {
  index: number;
  x: number;
  y: number;
  w?: number;
  h?: number;
  only?: "ansi" | "iso";
  knob?: true;
};

export const WIDTH = 16;
export const HEIGHT = 6.375;

const row = (y: number, keys: [index: number, x: number, w?: number][], only?: "ansi" | "iso"): KeyGeometry[] =>
  keys.map(([index, x, w]) => ({ index, x, y, w, only }));

export const KEYS: KeyGeometry[] = [
  ...row(0, [[0, 0], [12, 2], [18, 3], [24, 4], [30, 5], [36, 6.25], [42, 7.25], [48, 8.25], [54, 9.25],
    [60, 10.75], [66, 11.75], [72, 12.75], [78, 13.75]]),
  { index: 84, x: 15, y: 0, knob: true },
  ...row(1.375, [[1, 0], [7, 1], [13, 2], [19, 3], [25, 4], [31, 5], [37, 6], [43, 7], [49, 8], [55, 9], [61, 10],
    [67, 11], [73, 12], [79, 13, 2], [85, 15]]),
  ...row(2.375, [[2, 0, 1.5], [8, 1.5], [14, 2.5], [20, 3.5], [26, 4.5], [32, 5.5], [38, 6.5], [44, 7.5], [50, 8.5],
    [56, 9.5], [62, 10.5], [68, 11.5], [74, 12.5], [86, 15]]),
  ...row(2.375, [[80, 13.5, 1.5]], "ansi"),
  { index: 81, x: 13.5, y: 2.375, w: 1.5, h: 2, only: "iso" },
  ...row(3.375, [[3, 0, 1.75], [9, 1.75], [15, 2.75], [21, 3.75], [27, 4.75], [33, 5.75], [39, 6.75], [45, 7.75],
    [51, 8.75], [57, 9.75], [63, 10.75], [69, 11.75], [87, 15]]),
  ...row(3.375, [[81, 12.75, 2.25]], "ansi"),
  ...row(3.375, [[75, 12.75]], "iso"),
  ...row(4.375, [[4, 0, 2.25]], "ansi"),
  ...row(4.375, [[4, 0, 1.25], [76, 1.25]], "iso"),
  ...row(4.375, [[10, 2.25], [16, 3.25], [22, 4.25], [28, 5.25], [34, 6.25], [40, 7.25], [46, 8.25], [52, 9.25],
    [58, 10.25], [64, 11.25], [70, 12.25, 1.75], [82, 14], [88, 15]]),
  ...row(5.375, [[5, 0, 1.25], [11, 1.25, 1.25], [17, 2.5, 1.25], [35, 3.75, 6.25], [53, 10, 1.25], [59, 11.25, 1.25],
    [77, 13], [83, 14], [89, 15]]),
];

/** Keycap tones on the F75 "Gray Wood" colourway; every other key is the white alpha tone. */
export const TONES: Record<number, "mod" | "accent"> = {
  // pastel blue: Esc, Enter, Space, arrows
  0: "accent", 81: "accent", 35: "accent", 82: "accent", 77: "accent", 83: "accent", 89: "accent",
  // lavender grey: modifiers, F5-F8, Backspace and the navigation column
  2: "mod", 3: "mod", 4: "mod", 5: "mod", 11: "mod", 17: "mod", 53: "mod", 59: "mod", 70: "mod", 79: "mod",
  36: "mod", 42: "mod", 48: "mod", 54: "mod", 85: "mod", 86: "mod", 87: "mod", 88: "mod",
};
