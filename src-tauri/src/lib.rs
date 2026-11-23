mod device;
mod login;
mod protocol;
mod settings;
mod tray;

use device::{Connection, Keyboard};
use hidapi::HidApi;
use protocol::{
    Battery, Block, Config, Effect, KeyAction, KeyColours, Layer, Lighting, Macro, Modes, Palette, Performance, Rgb,
    EFFECTS, FACTORY_PALETTE, PALETTE_SLOTS, SLEEP_STEPS,
};
use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter, Manager, State};

/// Key layers: 0 default, 1 Fn, 2 FN2 (held through a key set to FN2), 3 Tap.
const LAYERS: [u8; 4] = [0, 1, 2, 3];

#[derive(Default)]
struct Hid {
    keyboard: Option<Keyboard>,
}

type Shared = Mutex<Hid>;

/// Why a command failed. Only a `Device` failure makes the open handle suspect.
enum Failure {
    Device(String),
    Rejected(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Device(e)
    }
}

impl From<protocol::Error> for Failure {
    fn from(e: protocol::Error) -> Self {
        Failure::Rejected(e.to_string())
    }
}

type HidJob = Box<dyn FnOnce(&mut Option<HidApi>) + Send>;

/// Runs `f` with the HID context, always on one thread that lives as long as the app. hidapi ties macOS's
/// device manager to the run loop of the thread that sets it up, and macOS uses that run loop whenever a
/// device connects, so that thread must never exit: a short-lived one crashed the app when the F75
/// reconnected over Bluetooth.
fn with_api<T: Send + 'static>(f: impl FnOnce(&mut HidApi) -> Result<T, String> + Send + 'static) -> Result<T, String> {
    static JOBS: OnceLock<mpsc::Sender<HidJob>> = OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (jobs, inbox) = mpsc::channel::<HidJob>();
        std::thread::Builder::new()
            .name("hid".into())
            .spawn(move || {
                let mut api = None;
                for job in inbox {
                    job(&mut api);
                }
            })
            .expect("start the HID thread");
        jobs
    });
    let (reply, outcome) = mpsc::channel();
    let job: HidJob = Box::new(move |api| {
        if api.is_none() {
            match HidApi::new() {
                Ok(new) => *api = Some(new),
                Err(e) => {
                    let _ = reply.send(Err(format!("can't access USB devices: {e}")));
                    return;
                }
            }
        }
        let _ = reply.send(f(api.as_mut().expect("set up above")));
    });
    const STOPPED: &str = "the HID thread stopped";
    jobs.send(job).map_err(|_| STOPPED.to_string())?;
    outcome.recv().map_err(|_| STOPPED.to_string())?
}

/// Runs `f` against the keyboard, opening it if needed. A device failure drops the handle so the next call reopens it.
fn with_keyboard<T>(state: &Shared, f: impl FnOnce(&Keyboard) -> Result<T, Failure>) -> Result<T, String> {
    let mut hid = state.lock().unwrap_or_else(|e| e.into_inner());
    if hid.keyboard.is_none() {
        hid.keyboard = Some(with_api(|api| {
            api.refresh_devices().map_err(|e| e.to_string())?;
            Keyboard::open(api)
        })?);
    }
    let keyboard = hid.keyboard.as_ref().expect("opened above");
    match f(keyboard) {
        Ok(value) => Ok(value),
        Err(Failure::Rejected(e)) => Err(e),
        Err(Failure::Device(e)) => {
            hid.keyboard = None;
            Err(e)
        }
    }
}

fn config(k: &Keyboard) -> Result<Config, Failure> {
    Ok(Config::parse(&k.read(Block::Config)?)?)
}

fn layer(k: &Keyboard, n: u8) -> Result<Layer, Failure> {
    Ok(Layer::parse(&k.read(Block::Layer(n))?)?)
}

fn layers(k: &Keyboard) -> Result<Vec<Layer>, Failure> {
    LAYERS.iter().map(|&n| layer(k, n)).collect()
}

fn palette(k: &Keyboard) -> Result<Palette, Failure> {
    Ok(Palette::parse(&k.read(Block::Palette)?)?)
}

/// The colour group Per-key mode shows, and its colours.
fn key_colours(k: &Keyboard, config: &Config) -> Result<(u8, KeyColours), Failure> {
    let group = config.colour_group();
    Ok((group, KeyColours::parse(&k.read(Block::Colours(group))?)?))
}

/// Writes the unchanged config again. The vendor app does this after palette and colour writes, so the
/// lights pick up the new colours.
fn relight(k: &Keyboard, config: &Config) -> Result<(), Failure> {
    Ok(k.write(Block::Config, config.bytes())?)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    connection: Connection,
    battery: Option<Battery>,
}

#[tauri::command(async)]
fn status(state: State<Shared>) -> Status {
    let connection = with_api(|api| {
        let _ = api.refresh_devices();
        Ok(device::connection(api))
    })
    .unwrap_or(Connection::None);
    if connection != Connection::Usb {
        state.lock().unwrap_or_else(|e| e.into_inner()).keyboard = None;
    }
    let battery = match connection {
        Connection::Usb => with_keyboard(&state, |k| Ok(protocol::battery(&k.read(Block::Battery)?))).ok(),
        _ => None,
    };
    Status { connection, battery }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    lighting: Lighting,
    performance: Performance,
    modes: Modes,
    layers: Vec<Vec<KeyAction>>,
    effects: &'static [Effect],
    sleep_steps: &'static [u8],
    /// Seven colours per effect, indexed by effect code; empty when the keyboard didn't give them.
    palette: Vec<[Rgb; PALETTE_SLOTS]>,
    /// The colours the palette's names ("Red", …) describe.
    factory_palette: [Rgb; PALETTE_SLOTS],
    /// Per-key colours (shown by the Per-key effect), by key-matrix slot; empty when unavailable.
    key_colours: Vec<Rgb>,
    macros: Vec<Macro>,
}

/// Everything F75 Studio reads from the keyboard. Colours and macros are optional, so a keyboard that
/// can't give them still gets lighting and keys.
struct Blocks {
    config: Config,
    layers: Vec<Layer>,
    palette: Option<Palette>,
    colours: Option<(u8, KeyColours)>,
    macros: Option<Vec<u8>>,
}

fn read_all(k: &Keyboard) -> Result<Blocks, Failure> {
    let config = config(k)?;
    Ok(Blocks {
        layers: layers(k)?,
        palette: palette(k).ok(),
        colours: key_colours(k, &config).ok(),
        macros: k.read(Block::Macros).ok(),
        config,
    })
}

impl Blocks {
    fn backup(&self) -> Backup {
        let (colour_group, colours) = self.colours.as_ref().map_or((0, Vec::new()), |(g, c)| (*g, c.bytes().to_vec()));
        Backup {
            config: self.config.bytes().to_vec(),
            layers: self.layers.iter().map(|l| l.bytes().to_vec()).collect(),
            palette: self.palette.as_ref().map_or_else(Vec::new, |p| p.bytes().to_vec()),
            colour_group,
            colours,
            macros: self.macros.clone().unwrap_or_default(),
        }
    }
}

/// Everything F75 Studio can change, as first read. Fields added after the first release default to
/// empty and are filled in from the keyboard the next time it is read.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Backup {
    config: Vec<u8>,
    layers: Vec<Vec<u8>>,
    palette: Vec<u8>,
    colour_group: u8,
    colours: Vec<u8>,
    macros: Vec<u8>,
}

impl Backup {
    /// Whether every block the app can change is saved.
    fn complete(&self) -> bool {
        self.layers.len() == LAYERS.len() && !self.palette.is_empty() && !self.colours.is_empty() && !self.macros.is_empty()
    }
}

fn backup_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("original-settings.json"))
}

fn saved_backup(app: &AppHandle) -> Option<Backup> {
    serde_json::from_slice(&fs::read(backup_path(app).ok()?).ok()?).ok()
}

/// Keeps the first settings this app ever read. A missing or damaged file is (re)written, and parts an
/// older backup lacks are added from `now`; nothing already saved is replaced. Writes are atomic.
fn keep_original(app: &AppHandle, now: Backup) -> Result<(), String> {
    let next = match saved_backup(app) {
        None => now,
        Some(b) if b.complete() => return Ok(()),
        Some(mut b) => {
            let had = b.layers.len();
            b.layers.extend(now.layers.into_iter().skip(had));
            let fill = |saved: &mut Vec<u8>, read: Vec<u8>| {
                if saved.is_empty() {
                    *saved = read;
                }
            };
            fill(&mut b.palette, now.palette);
            fill(&mut b.macros, now.macros);
            if b.colours.is_empty() {
                (b.colour_group, b.colours) = (now.colour_group, now.colours);
            }
            b
        }
    };
    let path = backup_path(app)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(&next).expect("plain data serializes"))
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| format!("couldn't save your original settings: {e}"))
}

/// Saves the original settings before the first change, whether it comes from the menu or the editor.
fn ensure_original(app: &AppHandle, state: &Shared) -> Result<(), String> {
    if saved_backup(app).is_some_and(|b| b.complete()) {
        return Ok(());
    }
    with_keyboard(state, |k| keep_original(app, read_all(k)?.backup()).map_err(Failure::Rejected))
}

/// Reads everything the UI shows.
#[tauri::command(async)]
fn snapshot(app: AppHandle, state: State<Shared>) -> Result<Snapshot, String> {
    with_keyboard(&state, |k| {
        let b = read_all(k)?;
        keep_original(&app, b.backup()).map_err(Failure::Rejected)?;
        Ok(Snapshot {
            lighting: b.config.lighting(),
            performance: b.config.performance(),
            modes: b.config.modes(),
            layers: b.layers.iter().map(Layer::actions).collect(),
            effects: &EFFECTS,
            sleep_steps: &SLEEP_STEPS,
            palette: b.palette.as_ref().map_or_else(Vec::new, Palette::rows),
            factory_palette: FACTORY_PALETTE,
            key_colours: b.colours.as_ref().map_or_else(Vec::new, |(_, c)| c.colours()),
            // Damaged macro memory only hides macro names; keys keep working.
            macros: b.macros.as_deref().and_then(|m| protocol::parse_macros(m).ok()).unwrap_or_default(),
        })
    })
}

#[tauri::command(async)]
fn set_effect(state: State<Shared>, effect: u8) -> Result<Lighting, String> {
    with_keyboard(&state, |k| {
        let next = config(k)?.with_effect(effect)?;
        k.write(Block::Config, next.bytes())?;
        Ok(next.lighting())
    })
}

#[tauri::command(async)]
fn set_lighting(state: State<Shared>, lighting: Lighting) -> Result<Lighting, String> {
    with_keyboard(&state, |k| {
        let next = config(k)?.with_lighting(lighting)?;
        k.write(Block::Config, next.bytes())?;
        Ok(next.lighting())
    })
}

#[tauri::command(async)]
fn set_performance(state: State<Shared>, performance: Performance) -> Result<Performance, String> {
    with_keyboard(&state, |k| {
        let next = config(k)?.with_performance(performance)?;
        k.write(Block::Config, next.bytes())?;
        Ok(next.performance())
    })
}

/// Mac or Windows mode, and what the knob does: the same switches as Fn+E / Fn+W and a long press on the knob.
#[tauri::command(async)]
fn set_modes(state: State<Shared>, modes: Modes) -> Result<Modes, String> {
    with_keyboard(&state, |k| {
        let next = config(k)?.with_modes(modes)?;
        k.write(Block::Config, next.bytes())?;
        Ok(next.modes())
    })
}

/// Opens System Settings at Input Monitoring, which macOS requires before an app can talk to a keyboard.
#[tauri::command]
fn open_privacy_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent").spawn();
}

#[tauri::command(async)]
fn set_key(state: State<Shared>, layer: u8, index: usize, action: KeyAction) -> Result<KeyAction, String> {
    if !LAYERS.contains(&layer) {
        return Err("there is no such layer".into());
    }
    with_keyboard(&state, |k| {
        let next = self::layer(k, layer)?.with_action(index, action)?;
        k.write(Block::Layer(layer), next.bytes())?;
        Ok(next.actions()[index])
    })
}

/// Paints keys in the colour group Per-key mode shows. Returns every key's colour.
#[tauri::command(async)]
fn set_key_colours(state: State<Shared>, keys: Vec<usize>, colour: Rgb) -> Result<Vec<Rgb>, String> {
    with_keyboard(&state, |k| {
        let config = config(k)?;
        let (group, colours) = key_colours(k, &config)?;
        let next = colours.with_colours(&keys, colour)?;
        k.write(Block::Colours(group), next.bytes())?;
        relight(k, &config)?;
        Ok(next.colours())
    })
}

/// Changes one of an effect's seven colours. Returns that effect's colours.
#[tauri::command(async)]
fn set_palette_colour(state: State<Shared>, effect: u8, slot: usize, colour: Rgb) -> Result<[Rgb; PALETTE_SLOTS], String> {
    with_keyboard(&state, |k| {
        let next = palette(k)?.with_colour(effect, slot, colour)?;
        k.write(Block::Palette, next.bytes())?;
        relight(k, &config(k)?)?;
        Ok(next.colours(effect).ok_or(protocol::Error::Invalid("that effect has no colours"))?)
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MacroResult {
    /// The key's layer after the change.
    layer: Vec<KeyAction>,
    macros: Vec<Macro>,
}

/// Records a macro onto a key. Other keys keep their macros (see `protocol::assign_macro`), and the macro is
/// fitted to the keyboard first (`protocol::fit`).
#[tauri::command(async)]
fn set_macro(state: State<Shared>, layer: u8, index: usize, recorded: Macro) -> Result<MacroResult, String> {
    let recorded = protocol::fit(recorded);
    with_keyboard(&state, |k| {
        let macros = protocol::parse_macros(&k.read(Block::Macros)?)?;
        let (next, macros) = protocol::assign_macro(&layers(k)?, &macros, (layer as usize, index), recorded)?;
        k.write(Block::Macros, &protocol::encode_macros(&macros)?)?;
        k.write(Block::Layer(layer), next.bytes())?;
        Ok(MacroResult { layer: next.actions(), macros })
    })
}

/// Writes back, byte for byte, the settings captured the first time this app read the keyboard.
#[tauri::command(async)]
fn restore_original(app: AppHandle, state: State<Shared>) -> Result<(), String> {
    let saved = fs::read(backup_path(&app)?).map_err(|_| "No original settings have been saved yet.")?;
    let backup: Backup = serde_json::from_slice(&saved).map_err(|_| "The saved original settings are damaged.")?;
    let config = Config::parse(&backup.config).map_err(|e| e.to_string())?;
    let layers = backup.layers.iter().map(|l| Layer::parse(l)).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    with_keyboard(&state, |k| {
        // Macros before the layers that play them; the config last, so the lights pick up everything.
        if !backup.macros.is_empty() {
            k.write(Block::Macros, &backup.macros)?;
        }
        for (&n, l) in LAYERS.iter().zip(&layers) {
            k.write(Block::Layer(n), l.bytes())?;
        }
        if !backup.palette.is_empty() {
            k.write(Block::Palette, &backup.palette)?;
        }
        if !backup.colours.is_empty() {
            k.write(Block::Colours(backup.colour_group), &backup.colours)?;
        }
        k.write(Block::Config, config.bytes())?;
        Ok(())
    })
}

/// App preferences (not keyboard settings).
#[tauri::command(async)]
fn settings(app: AppHandle) -> settings::Settings {
    settings::load(&app)
}

#[tauri::command(async)]
fn set_layout(app: AppHandle, iso: bool) -> Result<(), String> {
    apply_layout(&app, iso)
}

/// Saves the editor layout and tells an open editor, so the menu and the editor always agree.
fn apply_layout(app: &AppHandle, iso: bool) -> Result<(), String> {
    let next = settings::Settings { iso, ..settings::load(app) };
    settings::save(app, next)?;
    let _ = app.emit("settings-changed", next);
    Ok(())
}

/// A menu-bar app: no Dock icon and no window at launch. The key editor window is created on demand.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            status,
            snapshot,
            set_effect,
            set_lighting,
            set_performance,
            set_modes,
            open_privacy_settings,
            set_key,
            set_key_colours,
            set_palette_colour,
            set_macro,
            restore_original,
            settings,
            set_layout
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle();
            // Open at Login is on by default; once it has been switched on, it is the user's choice.
            let mut prefs = settings::load(handle);
            if login::SUPPORTED && !prefs.login_default_applied && login::set(true).is_ok() {
                prefs.login_default_applied = true;
                let _ = settings::save(handle, prefs);
            }
            tray::create(handle)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Closing the key editor must not quit the menu-bar app; only Quit (an explicit exit code) does.
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            // Opened by hand, the app shows the editor; started at login, it stays in the menu bar.
            tauri::RunEvent::Ready if tray::opened_by_user() => tray::open_editor(app),
            // Opening it again while it runs shows the editor too, as with any other app.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::open_editor(app),
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRunLoopGetCurrent() -> *const std::ffi::c_void;
        fn CFGetRetainCount(cf: *const std::ffi::c_void) -> isize;
    }

    /// hidapi ties its device manager to the run loop of the thread that sets it up, and macOS uses that run
    /// loop whenever a device connects. When it died with a short-lived thread, the app crashed (SIGTRAP) as
    /// soon as the F75 reconnected over Bluetooth while the editor checked the connection.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_hid_context_outlives_the_threads_that_use_it() {
        let run_loop = std::thread::spawn(|| with_api(|_| Ok(unsafe { CFRunLoopGetCurrent() } as usize)).unwrap()).join().unwrap();
        // Touching a dead run loop traps here, as the crash did.
        assert!(unsafe { CFGetRetainCount(run_loop as *const std::ffi::c_void) } > 0);
    }

    /// Regenerates the browser-preview data (src/preview.json) from the real-keyboard fixtures:
    /// `cargo test -- --ignored preview`.
    #[test]
    #[ignore]
    fn preview() {
        let config = Config::parse(include_bytes!("../fixtures/config.bin")).unwrap();
        let layers = [
            &include_bytes!("../fixtures/layer0.bin")[..],
            &include_bytes!("../fixtures/layer1.bin")[..],
            &include_bytes!("../fixtures/layer2.bin")[..],
            &include_bytes!("../fixtures/layer3.bin")[..],
        ];
        let snapshot = Snapshot {
            lighting: config.lighting(),
            performance: config.performance(),
            modes: config.modes(),
            layers: layers.iter().map(|b| Layer::parse(b).unwrap().actions()).collect(),
            effects: &EFFECTS,
            sleep_steps: &SLEEP_STEPS,
            palette: Palette::parse(include_bytes!("../fixtures/palette.bin")).unwrap().rows(),
            factory_palette: FACTORY_PALETTE,
            key_colours: KeyColours::parse(include_bytes!("../fixtures/colours.bin")).unwrap().colours(),
            macros: Vec::new(),
        };
        fs::write("../src/preview.json", serde_json::to_string(&snapshot).unwrap()).unwrap();
    }
}
