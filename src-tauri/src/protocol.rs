//! AULA F75 wired protocol (USB `258a:010c`). Pure: builds and parses bytes, never touches a device.
//!
//! Reverse engineered from the vendor configurator (class `CDevG5KB`) and verified against a real F75.
//! Every transfer is a 520-byte HID feature report `0x06` on the vendor collection (usage page `0xFF00`):
//!
//! ```text
//! [0x06, command, sub, 0, chunk count, chunk index, len lo, len hi, data…]
//! ```
//!
//! A write sends the data. A read sends the header with bit 7 of the command set, waits about 20 ms,
//! then reads feature report `0x06`; the reply echoes the header and carries the data from byte 8.
//!
//! | Block            | Write | Read | Sub     | Bytes             |
//! |------------------|-------|------|---------|-------------------|
//! | Key layer        | 0x03  | 0x83 | 0–3     | 512               |
//! | Config           | 0x04  | 0x84 | 0       | 128               |
//! | Macros           | 0x05  | 0x85 | 0       | 4096 (8 chunks)   |
//! | Per-key colours  | 0x06  | 0x86 | group   | 378               |
//! | Battery          | —     | 0x87 | 0x02    | 2                 |
//! | Effect palette   | 0x0A  | 0x8A | 0       | 512               |
//!
//! Blocks over 512 bytes go in 512-byte chunks, each with its own header (chunk count, chunk index, length).
//! Command 0x11 is factory reset; this module cannot build it.

pub const REPORT_ID: u8 = 0x06;
pub const PACKET_LEN: usize = 520;
const CHUNK: usize = 512;

/// What F75 Studio reads or writes. Factory reset (command 0x11) is deliberately not representable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Block {
    Config,
    Layer(u8),
    Battery,
    Macros,
    /// Per-key colour group n (the config selects which one per-key mode shows).
    Colours(u8),
    Palette,
}

impl Block {
    /// Write command; the read command is the same with bit 7 set.
    fn command(self) -> u8 {
        match self {
            Block::Layer(_) => 0x03,
            Block::Config => 0x04,
            Block::Macros => 0x05,
            Block::Colours(_) => 0x06,
            Block::Battery => 0x07,
            Block::Palette => 0x0a,
        }
    }

    fn sub(self) -> u8 {
        match self {
            Block::Layer(n) | Block::Colours(n) => n,
            Block::Battery => 0x02,
            Block::Config | Block::Macros | Block::Palette => 0,
        }
    }

    pub fn len(self) -> usize {
        match self {
            Block::Layer(_) | Block::Palette => 512,
            Block::Config => 128,
            Block::Battery => 2,
            Block::Macros => MACRO_BYTES,
            Block::Colours(_) => 3 * LIT_KEYS,
        }
    }
}

/// One packet per 512-byte chunk: [report id, command, sub, 0, chunk count, chunk index, len lo, len hi, data…].
fn packets(command: u8, block: Block, data: Option<&[u8]>) -> Vec<[u8; PACKET_LEN]> {
    let chunks = block.len().div_ceil(CHUNK);
    (0..chunks)
        .map(|i| {
            let start = i * CHUNK;
            let len = (block.len() - start).min(CHUNK);
            let mut p = [0u8; PACKET_LEN];
            p[..8].copy_from_slice(&[REPORT_ID, command, block.sub(), 0, chunks as u8, i as u8, len as u8, (len >> 8) as u8]);
            if let Some(data) = data {
                p[8..8 + len].copy_from_slice(&data[start..start + len]);
            }
            p
        })
        .collect()
}

pub fn read_requests(block: Block) -> Vec<[u8; PACKET_LEN]> {
    packets(block.command() | 0x80, block, None)
}

#[derive(Debug, PartialEq)]
pub enum Error {
    NotWritable,
    WrongLength,
    UnexpectedReply,
    Invalid(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::NotWritable => write!(f, "this block is read-only"),
            Error::WrongLength => write!(f, "data has the wrong length"),
            Error::UnexpectedReply => write!(f, "the keyboard sent an unexpected reply"),
            Error::Invalid(why) => write!(f, "{why}"),
        }
    }
}

pub fn write_requests(block: Block, data: &[u8]) -> Result<Vec<[u8; PACKET_LEN]>, Error> {
    if block == Block::Battery {
        return Err(Error::NotWritable);
    }
    if data.len() != block.len() {
        return Err(Error::WrongLength);
    }
    Ok(packets(block.command(), block, Some(data)))
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lighting {
    pub effect: u8,
    pub brightness: u8,
    pub speed: u8,
    pub colour: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    pub debounce: u8,
    pub sleep: u8,
}

/// Modes the keyboard also switches itself (Fn+W/Q/E, a long press on the knob).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Modes {
    /// 0 Windows, 1 Android, 2 Mac: the same as Fn+W, Fn+Q and Fn+E.
    pub system: u8,
    /// 0 volume (turn for volume, press to mute), 1 lighting (turn for brightness, press for the next effect).
    pub knob: u8,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub code: u8,
    pub name: &'static str,
    pub speed: bool,
    pub brightness: bool,
    pub colour: bool,
}

const fn fx(code: u8, name: &'static str, speed: bool, colour: bool) -> Effect {
    Effect { code, name, speed, brightness: true, colour }
}

const fn bare(code: u8, name: &'static str) -> Effect {
    Effect { code, name, speed: false, brightness: false, colour: false }
}

/// Vendor list order (KB.ini `LedOpt1..20`, decoded in OemDrv `0x4131d3`). An effect's position is its
/// settings slot: slot i lives at 0x3a + 2i as `[brightness 0-9, speed 0-4 << 4 | colour 0-7]`.
/// Colours 0-6 index the effect's palette (red, green, blue, yellow, magenta, cyan, white); 7 = colourful.
pub const EFFECTS: [Effect; 20] = [
    fx(1, "Static", false, true),
    fx(2, "Breathing", true, true),
    fx(3, "Spectrum", true, false),
    fx(4, "Flash away", true, true),
    fx(5, "Raindrops", true, true),
    fx(6, "Rainbow wheel", true, true),
    fx(7, "Ripples", true, true),
    fx(8, "Starlight", true, true),
    fx(9, "Fade away", true, true),
    fx(10, "Snake", true, true),
    fx(11, "Neon stream", true, true),
    fx(12, "Reactive", true, true),
    fx(13, "Sine wave", true, true),
    fx(14, "Scanner", true, true),
    fx(15, "Windmill", true, false),
    fx(16, "Waterfall", true, false),
    fx(17, "Blossom", true, false),
    fx(18, "Storm", true, true),
    bare(PER_KEY, "Per-key"),
    bare(0, "Off"),
];
const PER_KEY: u8 = 21;
const SLOTS: usize = 19;
const SLOT_BASE: usize = 0x3a;

/// Config block offsets (standard layout, OemDrv filler `0x49bbf0`).
const DEBOUNCE: usize = 0x03;
const PER_KEY_ACTIVE: usize = 0x09;
const EFFECT: usize = 0x0a;
const COLOUR_GROUP: usize = 0x0b;
const SLEEP: usize = 0x18;
const KNOB_MODE: usize = 0x1a;
const SYSTEM_MODE: usize = 0x1b;
/// Sleep timer in 30 s units (KB.ini `SleepHW`); 0 = never.
pub const SLEEP_STEPS: [u8; 10] = [0, 2, 3, 4, 6, 8, 10, 20, 30, 40];

/// The 128-byte settings block (commands 0x04 / 0x84).
#[derive(Clone)]
pub struct Config([u8; 128]);

fn slot(code: u8) -> Option<(usize, &'static Effect)> {
    let i = EFFECTS.iter().position(|e| e.code == code).filter(|&i| i < SLOTS)?;
    Some((SLOT_BASE + 2 * i, &EFFECTS[i]))
}

impl Config {
    /// Only the `5A A5` trailer marks a config block. The vendor app writes `FF FF` at 0x38-0x39, but the
    /// firmware changes them itself (meaning unknown), so they're kept as read.
    pub fn parse(bytes: &[u8]) -> Result<Config, Error> {
        let block: [u8; 128] = bytes.try_into().map_err(|_| Error::WrongLength)?;
        if block[0x7e..] != [0x5a, 0xa5] {
            return Err(Error::Invalid("not an F75 config block"));
        }
        Ok(Config(block))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn lighting(&self) -> Lighting {
        let effect = self.0[EFFECT];
        let (brightness, mix) = slot(effect).map_or((0, 0), |(s, _)| (self.0[s], self.0[s + 1]));
        Lighting { effect, brightness, speed: mix >> 4, colour: mix & 0x0f }
    }

    /// Selects an effect. Its stored brightness, speed and colour stay as they are.
    pub fn with_effect(&self, effect: u8) -> Result<Config, Error> {
        if !EFFECTS.iter().any(|e| e.code == effect) {
            return Err(Error::Invalid("unknown effect"));
        }
        let mut b = self.0;
        b[EFFECT] = effect;
        b[PER_KEY_ACTIVE] = (effect == PER_KEY) as u8;
        Ok(Config(b))
    }

    /// Selects an effect and sets its brightness, speed and colour. Only changed values are range-checked,
    /// so whatever the keyboard already stores (even outside the vendor app's ranges) is kept as it is.
    pub fn with_lighting(&self, l: Lighting) -> Result<Config, Error> {
        let mut next = self.with_effect(l.effect)?;
        let stored = next.lighting();
        let ok = |new: u8, old: u8, max: u8| new == old || new <= max;
        if !ok(l.brightness, stored.brightness, 9) || !ok(l.speed, stored.speed, 4) || !ok(l.colour, stored.colour, 7) {
            return Err(Error::Invalid("lighting value out of range"));
        }
        if let Some((s, fx)) = slot(l.effect) {
            let b = &mut next.0;
            if fx.brightness {
                b[s] = l.brightness;
            }
            if fx.speed {
                b[s + 1] = (b[s + 1] & 0x0f) | (l.speed << 4);
            }
            if fx.colour {
                b[s + 1] = (b[s + 1] & 0xf0) | l.colour;
            }
        }
        Ok(next)
    }

    /// The per-key colour group shown in Per-key mode (`0x0b` holds `0x20 + group`, OemDrv 0x49bbf0).
    pub fn colour_group(&self) -> u8 {
        self.0[COLOUR_GROUP].saturating_sub(0x20)
    }

    pub fn modes(&self) -> Modes {
        Modes { system: self.0[SYSTEM_MODE], knob: self.0[KNOB_MODE] }
    }

    /// Only changed values are range-checked, as in `with_lighting`.
    pub fn with_modes(&self, m: Modes) -> Result<Config, Error> {
        let stored = self.modes();
        if (m.system != stored.system && m.system > 2) || (m.knob != stored.knob && m.knob > 1) {
            return Err(Error::Invalid("unknown mode"));
        }
        let mut b = self.0;
        b[SYSTEM_MODE] = m.system;
        b[KNOB_MODE] = m.knob;
        Ok(Config(b))
    }

    pub fn performance(&self) -> Performance {
        Performance { debounce: self.0[DEBOUNCE], sleep: self.0[SLEEP] }
    }

    /// Only changed values are range-checked, as in `with_lighting`.
    pub fn with_performance(&self, p: Performance) -> Result<Config, Error> {
        let stored = self.performance();
        let debounce_ok = p.debounce == stored.debounce || (1..=4).contains(&p.debounce);
        let sleep_ok = p.sleep == stored.sleep || SLEEP_STEPS.contains(&p.sleep);
        if !debounce_ok || !sleep_ok {
            return Err(Error::Invalid("performance value out of range"));
        }
        let mut b = self.0;
        b[DEBOUNCE] = p.debounce;
        b[SLEEP] = p.sleep;
        Ok(Config(b))
    }
}

/// One key-map entry (4 bytes; byte 0 is the type).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeyAction {
    None,
    Key { modifiers: u8, usage: u8 },
    /// Button 1 left, 2 right, 3 middle, 4 forward, 5 back, 8 scroll up, 9 scroll down; `repeat` clicks
    /// `interval` ms apart (double click is `1, 2, 50`).
    Mouse { button: u8, repeat: u8, interval: u8 },
    Media { usage: u16 },
    /// Plays macro `index`. Mode 1 repeats `repeat` times, 2 while the key is held, 4 until any key is pressed.
    Macro { mode: u8, repeat: u8, index: u8 },
    System { code: u8 },
    Lighting { action: u8, value: u8 },
    Fn,
    Fn2,
    Other { bytes: [u8; 4] },
}

impl KeyAction {
    pub fn from_bytes(b: [u8; 4]) -> KeyAction {
        match b {
            [0, 0, 0, 0] => KeyAction::None,
            [0x00, modifiers, 0, usage] => KeyAction::Key { modifiers, usage },
            [0x01, button, repeat, interval] => KeyAction::Mouse { button, repeat, interval },
            [0x02, 0, hi, lo] => KeyAction::Media { usage: u16::from_be_bytes([hi, lo]) },
            [0x03, mode, repeat, index] => KeyAction::Macro { mode, repeat, index },
            [0x07, 0, 0, code] => KeyAction::System { code },
            [0x08, action, value, 0] => KeyAction::Lighting { action, value },
            [0x0d, 0, 0, 0] => KeyAction::Fn,
            [0x0d, 1, 0, 0] => KeyAction::Fn2,
            bytes => KeyAction::Other { bytes },
        }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        match self {
            KeyAction::None => [0; 4],
            KeyAction::Key { modifiers, usage } => [0x00, modifiers, 0, usage],
            KeyAction::Mouse { button, repeat, interval } => [0x01, button, repeat, interval],
            KeyAction::Macro { mode, repeat, index } => [0x03, mode, repeat, index],
            KeyAction::Media { usage } => {
                let [hi, lo] = usage.to_be_bytes();
                [0x02, 0, hi, lo]
            }
            KeyAction::System { code } => [0x07, 0, 0, code],
            KeyAction::Lighting { action, value } => [0x08, action, value, 0],
            KeyAction::Fn => [0x0d, 0, 0, 0],
            KeyAction::Fn2 => [0x0d, 1, 0, 0],
            KeyAction::Other { bytes } => bytes,
        }
    }
}

/// Keys per layer; slot 127 holds the layer marker `00 00 5A A5`.
pub const KEYS: usize = 127;
/// Keys with a light (one byte per key in each colour plane).
pub const LIT_KEYS: usize = 126;
/// Size of the macro memory (vendor default `MacroBufferSize`, OemDrv 0x40fe4c).
pub const MACRO_BYTES: usize = 4096;

/// One 512-byte key-map layer (commands 0x03 / 0x83): 0 default, 1 Fn, 2 FN2, 3 tap.
#[derive(Clone)]
pub struct Layer([u8; 512]);

impl Layer {
    pub fn parse(bytes: &[u8]) -> Result<Layer, Error> {
        let layer: [u8; 512] = bytes.try_into().map_err(|_| Error::WrongLength)?;
        if layer[508..] != [0x00, 0x00, 0x5a, 0xa5] {
            return Err(Error::Invalid("not an F75 key layer"));
        }
        Ok(Layer(layer))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn actions(&self) -> Vec<KeyAction> {
        self.0.chunks(4).take(KEYS).map(|e| KeyAction::from_bytes(e.try_into().unwrap())).collect()
    }

    pub fn with_action(&self, index: usize, action: KeyAction) -> Result<Layer, Error> {
        if index >= KEYS {
            return Err(Error::Invalid("no key at that index"));
        }
        let mut l = self.0;
        l[index * 4..index * 4 + 4].copy_from_slice(&action.to_bytes());
        Ok(Layer(l))
    }
}

pub type Rgb = [u8; 3];

/// A per-key colour group (commands 0x06 / 0x86, sub = group): red, green and blue planes of
/// `LIT_KEYS` bytes each, indexed by key-matrix slot. Shown when the effect is Per-key.
pub struct KeyColours([u8; 3 * LIT_KEYS]);

impl KeyColours {
    pub fn parse(bytes: &[u8]) -> Result<KeyColours, Error> {
        Ok(KeyColours(bytes.try_into().map_err(|_| Error::WrongLength)?))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn colours(&self) -> Vec<Rgb> {
        (0..LIT_KEYS).map(|k| [self.0[k], self.0[LIT_KEYS + k], self.0[2 * LIT_KEYS + k]]).collect()
    }

    pub fn with_colours(&self, keys: &[usize], rgb: Rgb) -> Result<KeyColours, Error> {
        let mut b = self.0;
        for &k in keys {
            if k >= LIT_KEYS {
                return Err(Error::Invalid("no light at that key"));
            }
            for (plane, value) in rgb.into_iter().enumerate() {
                b[plane * LIT_KEYS + k] = value;
            }
        }
        Ok(KeyColours(b))
    }
}

/// Colours per palette entry; the effect's colour setting 0-6 picks one (7 = colourful).
pub const PALETTE_SLOTS: usize = 7;
/// Every effect's colours from the factory, as read from a real F75: red, green, blue, yellow, magenta,
/// cyan and white.
pub const FACTORY_PALETTE: [Rgb; PALETTE_SLOTS] =
    [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [255, 0, 255], [0, 255, 255], [255, 255, 255]];
const PALETTE_ROW: usize = 3 * PALETTE_SLOTS;
/// Palette rows exist for effect codes 0..=22; the bytes after them are something else, kept as they are.
const PALETTE_ROWS: usize = 23;

/// Effect palettes (commands 0x0A / 0x8A): seven RGB colours per effect, at effect code × 21.
pub struct Palette([u8; 512]);

impl Palette {
    pub fn parse(bytes: &[u8]) -> Result<Palette, Error> {
        let table: [u8; 512] = bytes.try_into().map_err(|_| Error::WrongLength)?;
        if table[0x1fa..0x1fc] != [0x5a, 0xa5] {
            return Err(Error::Invalid("not an F75 palette"));
        }
        Ok(Palette(table))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// The seven colours of an effect; None past the last row (effect 22).
    pub fn colours(&self, effect: u8) -> Option<[Rgb; PALETTE_SLOTS]> {
        let row = (effect as usize).checked_mul(PALETTE_ROW).filter(|_| (effect as usize) < PALETTE_ROWS)?;
        Some(std::array::from_fn(|i| self.0[row + 3 * i..row + 3 * i + 3].try_into().expect("3 bytes")))
    }

    /// Every row, indexed by effect code.
    pub fn rows(&self) -> Vec<[Rgb; PALETTE_SLOTS]> {
        (0..PALETTE_ROWS as u8).filter_map(|e| self.colours(e)).collect()
    }

    /// Changes one colour of an effect that uses a colour setting.
    pub fn with_colour(&self, effect: u8, slot: usize, rgb: Rgb) -> Result<Palette, Error> {
        if slot >= PALETTE_SLOTS || !EFFECTS.iter().any(|e| e.code == effect && e.colour) {
            return Err(Error::Invalid("that effect has no such colour"));
        }
        let mut t = self.0;
        let at = effect as usize * PALETTE_ROW + 3 * slot;
        t[at..at + 3].copy_from_slice(&rgb);
        Ok(Palette(t))
    }
}

/// What a macro step does with its `code` (settled by a real F75's factory macro, see the tests).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    /// A HID keyboard usage.
    Key = 0,
    /// A modifier: HID usages E0-E7.
    Modifier = 1,
    /// A mouse button: 1 left, 2 right, 4 middle.
    Button = 2,
    MoveX = 3,
    MoveY = 4,
    Wheel = 5,
}

impl StepKind {
    fn from_nibble(n: u8) -> Option<StepKind> {
        use StepKind::*;
        [Key, Modifier, Button, MoveX, MoveY, Wheel].into_iter().find(|&k| k as u8 == n)
    }
}

/// One macro step. `up` marks a release; the step waits `delay` ms (at most `MAX_DELAY`) before the next.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub up: bool,
    pub code: u8,
    pub delay: u32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<Step>,
}

/// The vendor app's limit for macro names, in UTF-16 units.
pub const MACRO_NAME_MAX: usize = 30;
/// The firmware writes `5A A5` over these bytes of macro memory (seen by aula-f75-hub), so nothing is stored there.
const MACRO_MARKER: std::ops::Range<usize> = 126..128;
/// Longest pause a step can hold, in ms (20 bits).
pub const MAX_DELAY: u32 = 0xf_ffff;

/// Macro memory (commands 0x05 / 0x85): a little-endian (offset, size) pair per macro, then the bodies
/// back to back, each `[name length in bytes, UTF-16LE name, 4-byte steps]`; the rest is zero.
pub fn encode_macros(macros: &[Macro]) -> Result<Vec<u8>, Error> {
    if macros.len() > u8::MAX as usize {
        return Err(Error::Invalid("too many macros"));
    }
    let base = 4 * macros.len();
    if base > MACRO_MARKER.start {
        return Err(Error::Invalid("too many macros"));
    }
    let mut out = vec![0u8; MACRO_BYTES];
    let mut at = base;
    for (i, m) in macros.iter().enumerate() {
        let name: Vec<u8> = m.name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        if name.len() > 2 * MACRO_NAME_MAX {
            return Err(Error::Invalid("macro names are at most 30 characters"));
        }
        let mut body = vec![name.len() as u8];
        body.extend(name);
        for s in &m.steps {
            if s.delay > MAX_DELAY {
                return Err(Error::Invalid("macro step out of range"));
            }
            let head = (s.kind as u8) << 4 | if s.up { 0x80 } else { 0 } | (s.delay >> 16) as u8;
            body.extend([head, (s.delay >> 8) as u8, s.delay as u8, s.code]);
        }
        if at < MACRO_MARKER.end && at + body.len() > MACRO_MARKER.start {
            at = MACRO_MARKER.end;
        }
        let end = at + body.len();
        if end > MACRO_BYTES {
            return Err(Error::Invalid("the macros don't fit in the keyboard's memory"));
        }
        out[4 * i..4 * i + 2].copy_from_slice(&(at as u16).to_le_bytes());
        out[4 * i + 2..4 * i + 4].copy_from_slice(&(body.len() as u16).to_le_bytes());
        out[at..end].copy_from_slice(&body);
        at = end;
    }
    Ok(out)
}

const DAMAGED: Error = Error::Invalid("the keyboard's macro memory is damaged");

/// A stored name: UTF-16LE as the vendor app writes it (every second byte zero for Latin text), else plain
/// ASCII as in the factory macro.
fn macro_name(bytes: &[u8]) -> String {
    if bytes.len().is_multiple_of(2) && bytes.iter().skip(1).step_by(2).all(|&b| b == 0) {
        String::from_utf16_lossy(&bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Makes a macro recorded in the editor fit the keyboard: modifier keys become modifier steps, the name is
/// cut to 30 characters and pauses to `MAX_DELAY`.
pub fn fit(recorded: Macro) -> Macro {
    let name: Vec<u16> = recorded.name.encode_utf16().take(MACRO_NAME_MAX).collect();
    let steps = recorded
        .steps
        .into_iter()
        .map(|s| Step {
            kind: if s.kind == StepKind::Key && (0xe0..=0xe7).contains(&s.code) { StepKind::Modifier } else { s.kind },
            delay: s.delay.min(MAX_DELAY),
            ..s
        })
        .collect();
    Macro { name: String::from_utf16_lossy(&name), steps }
}

pub fn parse_macros(bytes: &[u8]) -> Result<Vec<Macro>, Error> {
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    if bytes.len() != MACRO_BYTES {
        return Err(Error::WrongLength);
    }
    let base = word(0);
    if base == 0 {
        return Ok(Vec::new());
    }
    if !base.is_multiple_of(4) || base > MACRO_BYTES {
        return Err(DAMAGED);
    }
    (0..base / 4)
        .map(|i| {
            let (at, size) = (word(4 * i), word(4 * i + 2));
            let body = bytes.get(at..at + size).filter(|b| !b.is_empty()).ok_or(DAMAGED)?;
            let name_len = body[0] as usize;
            if 1 + name_len > size || !(size - 1 - name_len).is_multiple_of(4) {
                return Err(DAMAGED);
            }
            let name = &body[1..1 + name_len];
            let steps = body[1 + name_len..]
                .chunks(4)
                .map(|c| {
                    Ok(Step {
                        kind: StepKind::from_nibble((c[0] >> 4) & 0x07).ok_or(DAMAGED)?,
                        up: c[0] & 0x80 != 0,
                        code: c[3],
                        delay: ((c[0] & 0x0f) as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32,
                    })
                })
                .collect::<Result<_, Error>>()?;
            Ok(Macro { name: macro_name(name), steps })
        })
        .collect()
}

/// Puts `new` on key `at` = (layer, key) without renumbering any other key's macro: it takes the lowest
/// number no other key plays (or a new one), slots nobody plays are emptied, and empty slots at the end
/// are dropped. A key that already played a macro keeps how it plays it; any other key plays it once.
/// Returns the changed layer and the macros to store.
pub fn assign_macro(layers: &[Layer], macros: &[Macro], at: (usize, usize), new: Macro) -> Result<(Layer, Vec<Macro>), Error> {
    let (l, k) = at;
    let target = layers.get(l).ok_or(Error::Invalid("no such layer"))?;
    let (mode, repeat) = match target.actions().get(k) {
        Some(KeyAction::Macro { mode, repeat, .. }) => (*mode, *repeat),
        Some(_) => (1, 1),
        None => return Err(Error::Invalid("no key at that index")),
    };
    let used: std::collections::HashSet<usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(li, layer)| layer.actions().into_iter().enumerate().map(move |(ki, a)| ((li, ki), a)))
        .filter_map(|(key, a)| match a {
            KeyAction::Macro { index, .. } if key != at => Some(index as usize),
            _ => None,
        })
        .collect();
    let slot = (0..macros.len()).find(|i| !used.contains(i)).unwrap_or(macros.len());
    let index = u8::try_from(slot).map_err(|_| Error::Invalid("too many macros"))?;
    let empty = Macro { name: String::new(), steps: Vec::new() };
    let mut kept: Vec<Macro> = (0..macros.len().max(slot + 1))
        .map(|i| match i {
            _ if i == slot => new.clone(),
            _ if used.contains(&i) => macros[i].clone(),
            _ => empty.clone(),
        })
        .collect();
    while kept.len() > slot + 1 && !used.contains(&(kept.len() - 1)) {
        kept.pop();
    }
    Ok((target.with_action(k, KeyAction::Macro { mode, repeat, index })?, kept))
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

/// Battery block (command 0x87, sub 0x02): `[level, status]`.
pub fn battery(data: &[u8]) -> Battery {
    let status = data[1];
    Battery { level: data[0], charging: status & 0xf0 != 0 && status & 0x0f == 0 }
}

/// Data from a GET_FEATURE reply, after checking it echoes the request (command, sub and chunk).
pub fn reply_data(request: &[u8; PACKET_LEN], reply: &[u8]) -> Result<Vec<u8>, Error> {
    let end = 8 + u16::from_le_bytes([request[6], request[7]]) as usize;
    if reply.len() < end || reply[..3] != request[..3] || reply[5] != request[5] {
        return Err(Error::UnexpectedReply);
    }
    Ok(reply[8..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(p: &[u8]) -> &[u8] {
        &p[..8]
    }

    // Headers below were echoed back by a real F75 (2026-11-20).
    #[test]
    fn read_requests_match_what_the_keyboard_echoes() {
        assert_eq!(header(&read_requests(Block::Config)[0]), [0x06, 0x84, 0, 0, 1, 0, 0x80, 0x00]);
        assert_eq!(header(&read_requests(Block::Layer(1))[0]), [0x06, 0x83, 1, 0, 1, 0, 0x00, 0x02]);
        assert_eq!(header(&read_requests(Block::Battery)[0]), [0x06, 0x87, 2, 0, 1, 0, 0x02, 0x00]);
        assert!(read_requests(Block::Config)[0][8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn large_blocks_travel_in_512_byte_chunks() {
        // Synthetic data; the chunked header is the vendor's CDevG5KB::AccessData (OemDrv 0x4993c0).
        let reads = read_requests(Block::Macros);
        assert_eq!(reads.len(), 8);
        assert_eq!(header(&reads[0]), [0x06, 0x85, 0, 0, 8, 0, 0x00, 0x02]);
        assert_eq!(header(&reads[7]), [0x06, 0x85, 0, 0, 8, 7, 0x00, 0x02]);
        let data: Vec<u8> = (0..4096).map(|i| (i / 512) as u8).collect();
        let writes = write_requests(Block::Macros, &data).unwrap();
        assert_eq!(header(&writes[3]), [0x06, 0x05, 0, 0, 8, 3, 0x00, 0x02]);
        assert!(writes[3][8..].iter().all(|&b| b == 3));
        assert_eq!(header(&read_requests(Block::Colours(1))[0]), [0x06, 0x86, 1, 0, 1, 0, 0x7a, 0x01]);
        assert_eq!(header(&read_requests(Block::Palette)[0]), [0x06, 0x8a, 0, 0, 1, 0, 0x00, 0x02]);
    }

    #[test]
    fn reply_data_checks_the_chunk_it_answers() {
        // Synthetic: a reply that echoes chunk 2 must not be taken as chunk 3.
        let requests = read_requests(Block::Macros);
        let mut reply = requests[2].to_vec();
        reply[8] = 0xab;
        assert_eq!(reply_data(&requests[2], &reply).unwrap()[0], 0xab);
        assert_eq!(reply_data(&requests[3], &reply), Err(Error::UnexpectedReply));
    }

    const CONFIG: &[u8] = include_bytes!("../fixtures/config.bin");

    #[test]
    fn write_request_carries_the_block_after_the_header() {
        let p = write_requests(Block::Config, CONFIG).unwrap()[0];
        assert_eq!(header(&p), [0x06, 0x04, 0, 0, 1, 0, 0x80, 0x00]);
        assert_eq!(&p[8..136], CONFIG);
        assert!(p[136..].iter().all(|&b| b == 0));
    }

    #[test]
    fn battery_cannot_be_written_and_lengths_must_match() {
        assert_eq!(write_requests(Block::Battery, &[0, 0]), Err(Error::NotWritable));
        assert_eq!(write_requests(Block::Config, &CONFIG[..127]), Err(Error::WrongLength));
    }

    #[test]
    fn reply_data_returns_the_payload_of_a_matching_reply() {
        // Real reply to the battery request (cable, charging).
        let reply = [0x06, 0x87, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x10];
        assert_eq!(reply_data(&read_requests(Block::Battery)[0], &reply), Ok(vec![0x00, 0x10]));
    }

    #[test]
    fn reply_data_rejects_a_reply_to_another_request() {
        let mut reply = read_requests(Block::Layer(0))[0].to_vec();
        assert_eq!(reply_data(&read_requests(Block::Config)[0], &reply), Err(Error::UnexpectedReply));
        reply.truncate(8);
        assert_eq!(reply_data(&read_requests(Block::Layer(0))[0], &reply), Err(Error::UnexpectedReply));
    }

    fn config() -> Config {
        Config::parse(CONFIG).unwrap()
    }

    /// (offset, before, after) for every byte that differs.
    fn changes(a: &[u8], b: &[u8]) -> Vec<(usize, u8, u8)> {
        (0..a.len()).filter(|&i| a[i] != b[i]).map(|i| (i, a[i], b[i])).collect()
    }

    #[test]
    fn reads_lighting_and_performance_from_a_real_config_block() {
        let c = config();
        assert_eq!(c.lighting(), Lighting { effect: 11, brightness: 9, speed: 3, colour: 7 });
        assert_eq!(c.performance(), Performance { debounce: 2, sleep: 2 });
    }

    #[test]
    fn rejects_blocks_that_are_not_an_f75_config() {
        assert!(Config::parse(&CONFIG[..100]).is_err());
        let mut no_trailer = CONFIG.to_vec();
        no_trailer[0x7f] = 0;
        assert!(Config::parse(&no_trailer).is_err());
    }

    const CONFIG_IN_USE: &[u8] = include_bytes!("../fixtures/config-in-use.bin");

    #[test]
    fn reads_a_config_the_firmware_changed_and_keeps_what_it_stored() {
        // A real F75 in daily use (2026-11-23): its firmware had set 0x38 from FF to 81. The vendor app writes
        // FF FF at 0x38-0x39 (OemDrv filler 0x49bbf0) but never reads them, and aula-f75-hub's F75 had 81 F0 there.
        let c = Config::parse(CONFIG_IN_USE).unwrap();
        assert_eq!(c.lighting().effect, 0); // Off
        assert_eq!(c.modes(), Modes { system: 2, knob: 1 });
        let lit = c.with_effect(1).unwrap();
        assert_eq!(changes(CONFIG_IN_USE, lit.bytes()), [(0x0a, 0x00, 0x01)]);
    }

    #[test]
    fn setting_lighting_changes_only_the_effect_byte_and_its_slot() {
        let c = config();
        let breathing_blue = c.with_lighting(Lighting { effect: 2, brightness: 4, speed: 1, colour: 2 }).unwrap();
        assert_eq!(changes(c.bytes(), breathing_blue.bytes()), [(0x0a, 0x0b, 0x02), (0x3c, 0x09, 0x04), (0x3d, 0x37, 0x12)]);
    }

    #[test]
    fn effects_without_a_colour_keep_the_stored_colour_bits() {
        let c = config();
        let rainbow = c.with_lighting(Lighting { effect: 3, brightness: 5, speed: 2, colour: 4 }).unwrap();
        assert_eq!(changes(c.bytes(), rainbow.bytes()), [(0x0a, 0x0b, 0x03), (0x3e, 0x09, 0x05), (0x3f, 0x37, 0x27)]);
    }

    #[test]
    fn rejects_out_of_range_lighting() {
        let c = config();
        let ok = Lighting { effect: 2, brightness: 4, speed: 1, colour: 2 };
        assert!(c.with_lighting(Lighting { brightness: 10, ..ok }).is_err());
        assert!(c.with_lighting(Lighting { speed: 5, ..ok }).is_err());
        assert!(c.with_lighting(Lighting { colour: 8, ..ok }).is_err());
        assert!(c.with_lighting(Lighting { effect: 0x40, ..ok }).is_err());
    }

    #[test]
    fn choosing_an_effect_keeps_its_stored_settings() {
        let c = config();
        let breathing = c.with_effect(2).unwrap();
        assert_eq!(changes(c.bytes(), breathing.bytes()), [(0x0a, 0x0b, 0x02)]);
        assert_eq!(breathing.lighting(), Lighting { effect: 2, brightness: 9, speed: 3, colour: 7 });
        assert!(c.with_effect(0x40).is_err());
    }

    #[test]
    fn values_already_on_the_keyboard_are_accepted_as_they_are() {
        // Effect 11's slot (0x4e) with an out-of-range colour nibble, and an unlisted sleep value.
        let mut odd = CONFIG.to_vec();
        odd[0x4f] = 0x39;
        odd[0x18] = 5;
        let odd = Config::parse(&odd).unwrap();
        let dimmer = odd.with_lighting(Lighting { effect: 11, brightness: 4, speed: 3, colour: 9 }).unwrap();
        assert_eq!(changes(odd.bytes(), dimmer.bytes()), [(0x4e, 0x09, 0x04)]);
        let debounce = odd.with_performance(Performance { debounce: 3, sleep: 5 }).unwrap();
        assert_eq!(changes(odd.bytes(), debounce.bytes()), [(0x03, 0x02, 0x03)]);
    }

    #[test]
    fn reads_and_sets_the_system_and_knob_modes() {
        // Byte 0x1b: 0 Windows (Fn+W), 1 Android (Fn+Q), 2 Mac (Fn+E). Byte 0x1a: knob 0 volume, 1 lighting
        // (a long press on the knob toggles it). Both tested on a real F75 by aula-f75-hub.
        let c = config();
        assert_eq!(c.modes(), Modes { system: 0, knob: 0 });
        let mac = c.with_modes(Modes { system: 2, knob: 1 }).unwrap();
        assert_eq!(changes(c.bytes(), mac.bytes()), [(0x1a, 0, 1), (0x1b, 0, 2)]);
        assert!(c.with_modes(Modes { system: 3, knob: 0 }).is_err());
        assert!(c.with_modes(Modes { system: 0, knob: 2 }).is_err());
    }

    #[test]
    fn setting_performance_changes_debounce_and_sleep_only() {
        let c = config();
        let p = c.with_performance(Performance { debounce: 3, sleep: 20 }).unwrap();
        assert_eq!(changes(c.bytes(), p.bytes()), [(0x03, 0x02, 0x03), (0x18, 0x02, 0x14)]);
        assert!(c.with_performance(Performance { debounce: 3, sleep: 5 }).is_err());
        assert!(c.with_performance(Performance { debounce: 0, sleep: 2 }).is_err());
    }

    const LAYER0: &[u8] = include_bytes!("../fixtures/layer0.bin");
    const LAYER1: &[u8] = include_bytes!("../fixtures/layer1.bin");
    const LAYER2: &[u8] = include_bytes!("../fixtures/layer2.bin");
    const LAYER3: &[u8] = include_bytes!("../fixtures/layer3.bin");

    #[test]
    fn decodes_the_entry_types_seen_on_the_keyboard() {
        let cases = [
            ([0x00, 0x00, 0x00, 0x29], KeyAction::Key { modifiers: 0, usage: 0x29 }),
            ([0x00, 0x04, 0x00, 0x2b], KeyAction::Key { modifiers: 0x04, usage: 0x2b }),
            ([0x00, 0x02, 0x00, 0x00], KeyAction::Key { modifiers: 0x02, usage: 0 }),
            ([0x02, 0x00, 0x00, 0xcd], KeyAction::Media { usage: 0xcd }),
            ([0x02, 0x00, 0x02, 0x23], KeyAction::Media { usage: 0x0223 }),
            ([0x07, 0x00, 0x00, 0x18], KeyAction::System { code: 0x18 }),
            ([0x08, 0x03, 0x01, 0x00], KeyAction::Lighting { action: 3, value: 1 }),
            ([0x0d, 0x00, 0x00, 0x00], KeyAction::Fn),
            ([0x00, 0x00, 0x00, 0x00], KeyAction::None),
            // Synthetic: any type byte the app doesn't know must survive untouched.
            ([0x05, 0x01, 0x02, 0x03], KeyAction::Other { bytes: [0x05, 0x01, 0x02, 0x03] }),
        ];
        for (bytes, action) in cases {
            assert_eq!(KeyAction::from_bytes(bytes), action, "{bytes:02x?}");
        }
    }

    #[test]
    fn encodes_mouse_macro_and_fn2_keys_as_the_vendor_app_does() {
        // Synthetic: constants from the vendor's keyinfo_to_hardware_code (OemDrv 0x49a920), little-endian.
        let cases = [
            ([0x01, 0x01, 0x01, 0x00], KeyAction::Mouse { button: 1, repeat: 1, interval: 0 }), // left button
            ([0x01, 0x02, 0x01, 0x00], KeyAction::Mouse { button: 2, repeat: 1, interval: 0 }), // right button
            ([0x01, 0x01, 0x02, 0x32], KeyAction::Mouse { button: 1, repeat: 2, interval: 50 }), // double click
            ([0x01, 0x09, 0x00, 0x00], KeyAction::Mouse { button: 9, repeat: 0, interval: 0 }), // scroll down
            ([0x03, 0x01, 0x03, 0x00], KeyAction::Macro { mode: 1, repeat: 3, index: 0 }), // macro 0, 3 times
            ([0x03, 0x02, 0x01, 0x05], KeyAction::Macro { mode: 2, repeat: 1, index: 5 }),
            ([0x0d, 0x01, 0x00, 0x00], KeyAction::Fn2),
        ];
        for (bytes, action) in cases {
            assert_eq!(KeyAction::from_bytes(bytes), action, "{bytes:02x?}");
            assert_eq!(action.to_bytes(), bytes, "{action:?}");
        }
    }

    #[test]
    fn every_entry_on_the_real_layers_round_trips() {
        for layer in [LAYER0, LAYER1, LAYER2, LAYER3] {
            for e in layer.chunks(4).take(KEYS) {
                let e: [u8; 4] = e.try_into().unwrap();
                assert_eq!(KeyAction::from_bytes(e).to_bytes(), e, "{e:02x?}");
            }
        }
    }

    #[test]
    fn layers_expose_actions_by_matrix_index() {
        let fn_layer = Layer::parse(LAYER1).unwrap().actions();
        assert_eq!(fn_layer.len(), KEYS);
        assert_eq!(fn_layer[12], KeyAction::Media { usage: 0x0223 }); // Fn+F1
        assert_eq!(fn_layer[24], KeyAction::Key { modifiers: 0x04, usage: 0x2b }); // Fn+F3 = Alt+Tab
    }

    #[test]
    fn editing_a_key_changes_only_its_entry_and_never_the_marker() {
        let layer = Layer::parse(LAYER0).unwrap();
        let caps_on_esc = layer.with_action(0, KeyAction::Key { modifiers: 0, usage: 0x39 }).unwrap();
        assert_eq!(&caps_on_esc.bytes()[..4], [0x00, 0x00, 0x00, 0x39]);
        assert_eq!(&caps_on_esc.bytes()[4..], &LAYER0[4..]);
        assert!(layer.with_action(KEYS, KeyAction::None).is_err());
        let mut no_marker = LAYER0.to_vec();
        no_marker[511] = 0;
        assert!(Layer::parse(&no_marker).is_err());
    }

    const COLOURS: &[u8] = include_bytes!("../fixtures/colours.bin");

    #[test]
    fn per_key_colours_are_red_green_and_blue_planes_indexed_by_key() {
        // Factory colour group on a real F75: Esc, WASD and the arrows lit blue, every other key off.
        let group = KeyColours::parse(COLOURS).unwrap();
        let colours = group.colours();
        assert_eq!(colours.len(), LIT_KEYS);
        assert_eq!((colours[0], colours[14], colours[89]), ([0, 0, 255], [0, 0, 255], [0, 0, 255]));
        assert_eq!(colours[1], [0, 0, 0]);
        // Planes at 0, 126 and 252 (KB.ini GameRGBIndex=0,1,2; written by OemDrv 0x49c020).
        let q = group.with_colours(&[8], [255, 16, 1]).unwrap();
        assert_eq!(changes(COLOURS, q.bytes()), [(8, 0, 255), (134, 0, 16), (260, 0, 1)]);
        assert!(group.with_colours(&[LIT_KEYS], [1, 1, 1]).is_err());
        assert!(KeyColours::parse(&COLOURS[..377]).is_err());
    }

    const PALETTE: &[u8] = include_bytes!("../fixtures/palette.bin");

    #[test]
    fn effect_palettes_hold_seven_rgb_colours_per_effect() {
        let p = Palette::parse(PALETTE).unwrap();
        // A real F75 from the factory: every effect has the same seven colours.
        assert!(p.rows().iter().all(|row| *row == FACTORY_PALETTE));
        assert_eq!(p.colours(2), Some(FACTORY_PALETTE));
        assert_eq!(p.colours(23), None); // the rows end at effect 22
        // Effect code × 21 + slot × 3, in R G B order (KB.ini RGBIndex=0,1,2; written by OemDrv 0x49c020).
        let teal = p.with_colour(2, 5, [0, 128, 128]).unwrap();
        assert_eq!(changes(PALETTE, teal.bytes()), [(58, 255, 128), (59, 255, 128)]);
        assert_eq!(teal.colours(2).unwrap()[5], [0, 128, 128]);
        assert!(p.with_colour(2, 7, [0, 0, 0]).is_err()); // 7 is "colourful", not a slot
        assert!(p.with_colour(3, 0, [0, 0, 0]).is_err()); // Spectrum picks its own colours
        let mut no_trailer = PALETTE.to_vec();
        no_trailer[0x1fb] = 0;
        assert!(Palette::parse(&no_trailer).is_err());
    }

    #[test]
    fn config_names_the_colour_group_per_key_mode_shows() {
        assert_eq!(config().colour_group(), 0); // 0x0b = 0x20 + group
    }

    fn key(code: u8, up: bool, delay: u32) -> Step {
        Step { kind: StepKind::Key, up, code, delay }
    }

    #[test]
    fn macro_memory_is_an_offset_table_then_named_bodies() {
        // Synthetic, laid out as the vendor's FillMatrix (OemDrv 0x49b3d0) and StMacro_To_HdMacro (0x49b000): a
        // little-endian (offset, size) pair per macro, then each body as [name length, UTF-16LE name, steps].
        // A step is [kind << 4 | 0x80 when released | delay bits 16-19, delay bits 8-15, delay bits 0-7, code].
        let a = Macro { name: "A".into(), steps: vec![key(0x04, false, 10), key(0x04, true, 0x12345)] };
        let bytes = encode_macros(std::slice::from_ref(&a)).unwrap();
        assert_eq!(bytes.len(), MACRO_BYTES);
        let expected = [0x04, 0x00, 0x0b, 0x00, 0x02, 0x41, 0x00, 0x00, 0x00, 0x0a, 0x04, 0x81, 0x23, 0x45, 0x04];
        assert_eq!(bytes[..15], expected);
        assert!(bytes[15..].iter().all(|&b| b == 0));
        assert_eq!(parse_macros(&bytes).unwrap(), [a]);
    }

    #[test]
    fn macros_follow_each_other_and_empty_memory_has_none() {
        assert_eq!(parse_macros(&[0; MACRO_BYTES]).unwrap(), []); // a real F75's macro memory, as shipped
        // Synthetic, as above.
        let hi = Macro { name: "Hi".into(), steps: vec![key(0x0b, false, 5)] };
        let click = Macro { name: "Z".into(), steps: vec![Step { kind: StepKind::Button, up: false, code: 1, delay: 0 }] };
        let bytes = encode_macros(&[hi.clone(), click.clone()]).unwrap();
        assert_eq!(bytes[..8], [0x08, 0x00, 0x09, 0x00, 0x11, 0x00, 0x07, 0x00]);
        assert_eq!(bytes[8..24], [0x04, 0x48, 0x00, 0x69, 0x00, 0x00, 0x00, 0x05, 0x0b, 0x02, 0x5a, 0x00, 0x20, 0x00, 0x00, 0x01]);
        assert_eq!(parse_macros(&bytes).unwrap(), [hi, click]);
    }

    #[test]
    fn rejects_macros_that_do_not_fit_and_damaged_memory() {
        // Synthetic: limits from the vendor app (30-character names, 20-bit delays, 4 KB of memory).
        let named = |name: &str| Macro { name: name.into(), steps: vec![] };
        assert!(encode_macros(&[named(&"x".repeat(31))]).is_err());
        assert!(encode_macros(&[Macro { name: "slow".into(), steps: vec![key(4, false, 0x100000)] }]).is_err());
        assert!(encode_macros(&[Macro { name: "long".into(), steps: vec![key(4, false, 0); 1100] }]).is_err());
        let mut damaged = encode_macros(&[named("ok")]).unwrap();
        damaged[2] = 0xff; // size runs past the memory
        assert!(parse_macros(&damaged).is_err());
        let mut unknown_step = encode_macros(&[Macro { name: String::new(), steps: vec![key(4, false, 0)] }]).unwrap();
        unknown_step[5] = 0x70; // step kind 7
        assert!(parse_macros(&unknown_step).is_err());
    }

    const MACROS: &[u8] = include_bytes!("../fixtures/macros.bin");
    const FUNCTIONS: &[u8] = include_bytes!("../fixtures/layer2-functions.bin");

    /// The macros in `fixtures/macros.bin`: written by this app to a real F75 and read back from it.
    fn recorded() -> Vec<Macro> {
        let step = |kind, up, code, delay| Step { kind, up, code, delay };
        vec![
            Macro {
                name: "Hi".into(),
                steps: vec![
                    step(StepKind::Modifier, false, 0xe1, 30),
                    key(0x0b, false, 80),
                    key(0x0b, true, 20),
                    step(StepKind::Modifier, true, 0xe1, 150),
                    key(0x0c, false, 75),
                    key(0x0c, true, 0),
                ],
            },
            Macro {
                name: "Click".into(),
                steps: vec![
                    step(StepKind::Button, false, 1, 40),
                    step(StepKind::Button, true, 1, 0x12345),
                    step(StepKind::Wheel, false, 0xff, 10),
                ],
            },
        ]
    }

    #[test]
    fn reads_macros_as_a_real_f75_stores_them() {
        assert_eq!(parse_macros(MACROS).unwrap(), recorded());
        assert_eq!(encode_macros(&recorded()).unwrap(), MACROS);
    }

    const FACTORY_MACRO: &[u8] = include_bytes!("../fixtures/factory-macro.bin");

    #[test]
    fn reads_the_factory_macro_of_a_real_f75() {
        // Block 0 of a stock F75's macro memory, read by aula-f75-hub (MIT; its backup/cmd_43.hex): macro "M1"
        // types Ctrl+`. It settles the step kinds (0 key, 1 modifier) and shows that names can be plain ASCII.
        let mut memory = FACTORY_MACRO.to_vec();
        memory.resize(MACRO_BYTES, 0);
        let step = |kind, up, code| Step { kind, up, code, delay: 10 };
        let m1 = Macro {
            name: "M1".into(),
            steps: vec![
                step(StepKind::Modifier, false, 0xe0),
                step(StepKind::Key, false, 0x35),
                step(StepKind::Modifier, true, 0xe0),
                step(StepKind::Key, true, 0x35),
            ],
        };
        assert_eq!(parse_macros(&memory).unwrap(), [m1]);
    }

    #[test]
    fn recorded_macros_are_fitted_to_the_keyboard() {
        let recorded = Macro {
            name: "x".repeat(40),
            steps: vec![
                Step { kind: StepKind::Key, up: false, code: 0xe1, delay: 0x20_0000 }, // Shift, as the editor records it
                Step { kind: StepKind::Key, up: false, code: 0x04, delay: 5 },
            ],
        };
        let fitted = fit(recorded);
        assert_eq!(fitted.name, "x".repeat(MACRO_NAME_MAX));
        assert_eq!(
            fitted.steps,
            [
                Step { kind: StepKind::Modifier, up: false, code: 0xe1, delay: MAX_DELAY },
                Step { kind: StepKind::Key, up: false, code: 0x04, delay: 5 },
            ]
        );
    }

    #[test]
    fn no_macro_lies_across_the_bytes_the_firmware_overwrites() {
        // Synthetic: the firmware writes 5A A5 over bytes 126-127 of macro memory (aula-f75-hub, seen on
        // hardware), so a macro that would cross them starts at 128.
        let m = |n| Macro { name: String::new(), steps: vec![key(4, false, 0); n] };
        // Two macros: the table takes 8 bytes, the first 113 (to 121), the second 17 would cross 126.
        let bytes = encode_macros(&[m(28), m(4)]).unwrap();
        assert_eq!(bytes[4..6], [128, 0]);
        assert_eq!(parse_macros(&bytes).unwrap(), [m(28), m(4)]);
        let many: Vec<Macro> = (0..32).map(|_| m(0)).collect();
        assert!(encode_macros(&many).is_err()); // a 128-byte table would cross them itself
    }

    #[test]
    fn reads_mouse_macro_and_fn2_keys_as_a_real_f75_stores_them() {
        // Written to the FN2 layer of a real F75 and read back.
        let fn2 = Layer::parse(FUNCTIONS).unwrap().actions();
        assert_eq!(fn2[0], KeyAction::Mouse { button: 1, repeat: 1, interval: 0 });
        assert_eq!(fn2[1], KeyAction::Mouse { button: 2, repeat: 1, interval: 0 });
        assert_eq!(fn2[7], KeyAction::Mouse { button: 1, repeat: 2, interval: 50 });
        assert_eq!(fn2[8], KeyAction::Mouse { button: 8, repeat: 0, interval: 0 });
        assert_eq!(fn2[13], KeyAction::Macro { mode: 1, repeat: 3, index: 0 });
        assert_eq!(fn2[14], KeyAction::Macro { mode: 2, repeat: 1, index: 1 });
        assert_eq!(fn2[19], KeyAction::Fn2);
        for e in FUNCTIONS.chunks(4).take(KEYS) {
            let e: [u8; 4] = e.try_into().unwrap();
            assert_eq!(KeyAction::from_bytes(e).to_bytes(), e, "{e:02x?}");
        }
    }

    fn played(index: u8, mode: u8) -> KeyAction {
        KeyAction::Macro { mode, repeat: 1, index }
    }

    #[test]
    fn a_new_macro_takes_a_free_number_and_other_keys_keep_theirs() {
        let m = |name: &str| Macro { name: name.into(), steps: vec![key(4, false, 0)] };
        // Synthetic: key 1 plays macro 1 ("b"), key 2 and Fn+key 3 play macro 0 ("a"); macro 2 ("c") is unused.
        let layer0 = Layer::parse(LAYER0).unwrap().with_action(1, played(1, 1)).unwrap().with_action(2, played(0, 4)).unwrap();
        let layer1 = Layer::parse(LAYER1).unwrap().with_action(3, played(0, 1)).unwrap();
        let (fn_layer, macros) = assign_macro(&[layer0, layer1.clone()], &[m("a"), m("b"), m("c")], (1, 7), m("new")).unwrap();
        assert_eq!(macros, [m("a"), m("b"), m("new")]);
        // Fn+1 (Bluetooth channel 1, `07 00 00 05`) becomes macro 2, played once; nothing else changes.
        assert_eq!(changes(layer1.bytes(), fn_layer.bytes()), [(28, 0x07, 0x03), (29, 0, 0x01), (30, 0, 0x01), (31, 0x05, 0x02)]);
    }

    #[test]
    fn re_recording_a_key_reuses_its_number_and_keeps_how_it_plays() {
        let m = |name: &str| Macro { name: name.into(), steps: vec![] };
        let layer = Layer::parse(LAYER0).unwrap().with_action(5, played(0, 4)).unwrap();
        let (layer, macros) = assign_macro(&[layer], &[m("old")], (0, 5), m("new")).unwrap();
        assert_eq!(macros, [m("new")]);
        assert_eq!(layer.actions()[5], played(0, 4));
    }

    #[test]
    fn unused_macros_are_emptied_in_place_and_dropped_from_the_end() {
        let m = |name: &str| Macro { name: name.into(), steps: vec![key(4, false, 0)] };
        let empty = Macro { name: String::new(), steps: vec![] };
        // Synthetic: only key 9 plays a macro; the others are left over from keys that were changed.
        let plays_c = Layer::parse(LAYER0).unwrap().with_action(9, played(2, 1)).unwrap();
        let (layer, macros) = assign_macro(&[plays_c], &[m("a"), m("b"), m("c")], (0, 10), m("new")).unwrap();
        assert_eq!(macros, [m("new"), empty, m("c")]);
        assert_eq!((layer.actions()[9], layer.actions()[10]), (played(2, 1), played(0, 1)));
        let plays_a = Layer::parse(LAYER0).unwrap().with_action(9, played(0, 1)).unwrap();
        let (_, macros) = assign_macro(&[plays_a], &[m("a"), m("b"), m("c")], (0, 10), m("new")).unwrap();
        assert_eq!(macros, [m("a"), m("new")]);
    }

    #[test]
    fn battery_follows_the_vendor_charging_rule() {
        assert_eq!(battery(&[0x00, 0x10]), Battery { level: 0, charging: true }); // real, on cable
        // Synthetic: the charging rule from the vendor's ReadPower (OemDrv 0x49a5c0).
        assert_eq!(battery(&[0x55, 0x00]), Battery { level: 85, charging: false });
        assert_eq!(battery(&[0x64, 0x11]), Battery { level: 100, charging: false });
    }
}
