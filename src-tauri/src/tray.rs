//! The menu-bar app: a native menu that reads and writes the keyboard directly. Nothing polls and no web
//! view exists until the user opens the key editor; the menu refreshes when the pointer reaches the icon.
use crate::device::Connection;
use crate::protocol::{Battery, Block, Lighting, Modes, Performance, Rgb, EFFECTS, FACTORY_PALETTE, PALETTE_SLOTS, SLEEP_STEPS};
use crate::{config, with_keyboard, Shared};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

/// What the menu shows, read from the keyboard.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub connection: Connection,
    pub battery: Option<Battery>,
    pub lighting: Option<Lighting>,
    pub performance: Option<Performance>,
    pub modes: Option<Modes>,
    /// The current effect's seven colours.
    pub palette: Option<[Rgb; PALETTE_SLOTS]>,
    pub problem: Option<String>,
    /// Whether Open at Login is on; None where it isn't supported.
    pub login: Option<bool>,
    /// The editor draws the ISO layout.
    pub iso: bool,
}

/// One menu entry, independent of Tauri so the menu's content can be tested.
#[derive(Debug, PartialEq)]
pub enum Entry {
    Info(String),
    Separator,
    Action { id: String, label: String },
    Choice { id: String, label: String, checked: bool },
    Group { label: String, enabled: bool, entries: Vec<Entry> },
    About,
}

/// What a clicked menu item asks for.
#[derive(Debug, PartialEq)]
pub enum Command {
    Effect(u8),
    Brightness(u8),
    Speed(u8),
    Colour(u8),
    Debounce(u8),
    Sleep(u8),
    System(u8),
    Knob(u8),
    Editor,
    Restore,
    Permissions,
    Login,
    Layout(bool),
    Quit,
}

const SPEEDS: [&str; 5] = ["Slowest", "Slow", "Medium", "Fast", "Fastest"];
/// Names of the factory palette (`FACTORY_PALETTE`), then colour setting 7.
const COLOURS: [&str; 8] = ["Red", "Green", "Blue", "Yellow", "Magenta", "Cyan", "White", "Colourful"];

/// A colour's name, or its hex value once it was changed in the editor.
fn colour_label(slot: u8, palette: Option<&[Rgb; PALETTE_SLOTS]>) -> String {
    match palette.and_then(|p| p.get(slot as usize)) {
        Some(&[r, g, b]) if [r, g, b] != FACTORY_PALETTE[slot as usize] => format!("Custom #{r:02X}{g:02X}{b:02X}"),
        _ => COLOURS[slot as usize].to_string(),
    }
}

/// Sleep steps are 30 s each; 0 means never.
fn sleep_label(step: u8) -> String {
    match step {
        0 => "Never".into(),
        s if s % 2 == 0 => format!("{} min", s / 2),
        s => format!("{}.5 min", s / 2),
    }
}

fn status(v: &View) -> String {
    match v.connection {
        Connection::Usb => {
            let level = v.battery.map(|b| b.level).filter(|l| (1..=100).contains(l));
            let battery = match (v.battery, level) {
                (Some(b), Some(l)) if b.charging => format!(" · Charging · {l}%"),
                (Some(b), None) if b.charging => " · Charging".into(),
                (_, Some(l)) => format!(" · Battery {l}%"),
                _ => String::new(),
            };
            format!("F75 · USB-C{battery}")
        }
        Connection::Wireless => "F75 · wireless · plug in USB-C to change settings".into(),
        Connection::None => "F75 not connected · plug in USB-C".into(),
    }
}

fn choices(prefix: &str, items: impl IntoIterator<Item = (u8, String)>, current: Option<u8>) -> Vec<Entry> {
    items
        .into_iter()
        .map(|(n, label)| Entry::Choice { id: format!("{prefix}:{n}"), label, checked: current == Some(n) })
        .collect()
}

fn group(label: &str, enabled: bool, entries: Vec<Entry>) -> Entry {
    Entry::Group { label: label.into(), enabled, entries }
}

fn action(id: &str, label: &str) -> Entry {
    Entry::Action { id: id.into(), label: label.into() }
}

/// The whole menu for a given keyboard state.
pub fn entries(v: &View) -> Vec<Entry> {
    let l = v.lighting;
    let p = v.performance;
    let effect = l.and_then(|l| EFFECTS.iter().find(|e| e.code == l.effect));
    let mut es = vec![Entry::Info(status(v))];
    if v.battery.is_some_and(|b| b.charging && !(1..=100).contains(&b.level)) {
        es.push(Entry::Info("The F75 doesn't send its battery level over the cable".into()));
    }
    if let Some(problem) = &v.problem {
        es.push(Entry::Info(problem.clone()));
        if problem.contains("Input Monitoring") {
            es.push(action("permissions", "Open Input Monitoring Settings…"));
        }
    }
    es.push(Entry::Separator);
    let fx = EFFECTS.iter().map(|e| (e.code, e.name.to_string()));
    es.push(group("Effect", l.is_some(), choices("fx", fx, l.map(|l| l.effect))));
    let levels = (0..=9).map(|b| (b, format!("{}%", (b as u32 + 1) * 10)));
    es.push(group("Brightness", effect.is_some_and(|e| e.brightness), choices("br", levels, l.map(|l| l.brightness))));
    let speeds = (0..5u8).map(|s| (s, SPEEDS[s as usize].to_string()));
    es.push(group("Speed", effect.is_some_and(|e| e.speed), choices("sp", speeds, l.map(|l| l.speed))));
    let colours = (0..8u8).map(|c| (c, colour_label(c, v.palette.as_ref())));
    es.push(group("Colour", effect.is_some_and(|e| e.colour), choices("co", colours, l.map(|l| l.colour))));
    es.push(Entry::Separator);
    let debounce = (1..=4u8).map(|d| (d, d.to_string()));
    es.push(group("Debounce", p.is_some(), choices("db", debounce, p.map(|p| p.debounce))));
    let sleep = SLEEP_STEPS.iter().map(|&s| (s, sleep_label(s)));
    es.push(group("Sleep", p.is_some(), choices("sl", sleep, p.map(|p| p.sleep))));
    let m = v.modes;
    let systems = [(2, "Mac (Fn+E)"), (0, "Windows (Fn+W)"), (1, "Android (Fn+Q)")].map(|(n, l)| (n, l.to_string()));
    es.push(group("System", m.is_some(), choices("os", systems, m.map(|m| m.system))));
    let knob = [(0, "Volume and mute"), (1, "Brightness and effects")].map(|(n, l)| (n, l.to_string()));
    es.push(group("Knob", m.is_some(), choices("kn", knob, m.map(|m| m.knob))));
    es.push(Entry::Separator);
    es.push(action("editor", "Keys, Macros and Colours…"));
    let connected = v.connection == Connection::Usb && l.is_some();
    es.push(group("Restore Original Settings", connected, vec![action("restore", "Restore Now")]));
    let mut prefs = Vec::new();
    if let Some(on) = v.login {
        prefs.push(Entry::Choice { id: "login".into(), label: "Open at Login".into(), checked: on });
    }
    let layouts = vec![
        Entry::Choice { id: "layout:ansi".into(), label: "ANSI (US)".into(), checked: !v.iso },
        Entry::Choice { id: "layout:iso".into(), label: "ISO".into(), checked: v.iso },
    ];
    prefs.push(group("Keyboard Layout", true, layouts));
    es.push(group("Settings", true, prefs));
    es.push(Entry::Separator);
    es.push(Entry::About);
    es.push(action("quit", "Quit F75 Studio"));
    es
}

pub fn command(id: &str) -> Option<Command> {
    match id {
        "editor" => return Some(Command::Editor),
        "restore" => return Some(Command::Restore),
        "permissions" => return Some(Command::Permissions),
        "login" => return Some(Command::Login),
        "layout:ansi" => return Some(Command::Layout(false)),
        "layout:iso" => return Some(Command::Layout(true)),
        "quit" => return Some(Command::Quit),
        _ => {}
    }
    let (kind, n) = id.split_once(':')?;
    let n: u8 = n.parse().ok()?;
    Some(match kind {
        "fx" => Command::Effect(n),
        "br" => Command::Brightness(n),
        "sp" => Command::Speed(n),
        "co" => Command::Colour(n),
        "db" => Command::Debounce(n),
        "sl" => Command::Sleep(n),
        "os" => Command::System(n),
        "kn" => Command::Knob(n),
        _ => return None,
    })
}

// --- Tauri glue ---------------------------------------------------------------------------------

const TRAY: &str = "f75";
static LAST_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let unknown = View {
        connection: Connection::None,
        battery: None,
        lighting: None,
        performance: None,
        modes: None,
        palette: None,
        problem: None,
        login: crate::login::SUPPORTED.then(crate::login::enabled),
        iso: crate::settings::load(app).iso,
    };
    TrayIconBuilder::with_id(TRAY)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("F75 Studio")
        .menu(&menu(app, &entries(&unknown))?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| perform(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Enter { .. } = event {
                refresh(tray.app_handle(), false);
            }
        })
        .build(app)?;
    refresh(app, true);
    Ok(())
}

fn menu(app: &AppHandle, entries: &[Entry]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for e in entries {
        menu.append(item(app, e)?.as_ref())?;
    }
    Ok(menu)
}

fn item(app: &AppHandle, e: &Entry) -> tauri::Result<Box<dyn IsMenuItem<Wry>>> {
    Ok(match e {
        Entry::Info(text) => Box::new(MenuItem::new(app, text, false, None::<&str>)?),
        Entry::Separator => Box::new(PredefinedMenuItem::separator(app)?),
        Entry::About => Box::new(PredefinedMenuItem::about(app, Some("About F75 Studio"), None)?),
        Entry::Action { id, label } => Box::new(MenuItem::with_id(app, id, label, true, None::<&str>)?),
        Entry::Choice { id, label, checked } => Box::new(CheckMenuItem::with_id(app, id, label, true, *checked, None::<&str>)?),
        Entry::Group { label, enabled, entries } => {
            let sub = Submenu::new(app, label, *enabled)?;
            for e in entries {
                sub.append(item(app, e)?.as_ref())?;
            }
            Box::new(sub)
        }
    })
}

/// Reads the keyboard on a worker thread and rebuilds the menu. Hovering refreshes at most every 2 s.
fn refresh(app: &AppHandle, force: bool) {
    {
        let mut last = LAST_REFRESH.lock().unwrap_or_else(|e| e.into_inner());
        if !force && last.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return;
        }
        *last = Some(Instant::now());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let view = read(&app);
        if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY), menu(&app, &entries(&view))) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}

fn read(app: &AppHandle) -> View {
    let status = crate::status(app.state());
    let mut view = View {
        connection: status.connection,
        battery: status.battery,
        lighting: None,
        performance: None,
        modes: None,
        palette: None,
        problem: None,
        login: crate::login::SUPPORTED.then(crate::login::enabled),
        iso: crate::settings::load(app).iso,
    };
    if status.connection == Connection::Usb {
        match with_keyboard(&app.state::<Shared>(), |k| {
            let c = config(k)?;
            // Only names custom colours; the menu works without it.
            let colours = crate::palette(k).ok().and_then(|p| p.colours(c.lighting().effect));
            Ok((c.lighting(), c.performance(), c.modes(), colours))
        }) {
            Ok((l, p, m, colours)) => (view.lighting, view.performance, view.modes, view.palette) = (Some(l), Some(p), Some(m), colours),
            Err(e) => view.problem = Some(e),
        }
    }
    view.problem = view.problem.or_else(|| LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).take());
    view
}

fn perform(app: &AppHandle, id: &str) {
    let Some(cmd) = command(id) else { return };
    match cmd {
        Command::Quit => app.exit(0),
        Command::Editor => open_editor(app),
        Command::Permissions => crate::open_privacy_settings(),
        cmd => {
            let app = app.clone();
            std::thread::spawn(move || {
                let state = app.state::<Shared>();
                let changes_keyboard = !matches!(cmd, Command::Restore | Command::Login | Command::Layout(_));
                let saved = if changes_keyboard { crate::ensure_original(&app, &state) } else { Ok(()) };
                let result = saved.and_then(|()| match cmd {
                    Command::Effect(e) => crate::set_effect(app.state(), e).map(drop),
                    Command::Brightness(b) => change_lighting(&state, |l| Lighting { brightness: b, ..l }),
                    Command::Speed(s) => change_lighting(&state, |l| Lighting { speed: s, ..l }),
                    Command::Colour(c) => change_lighting(&state, |l| Lighting { colour: c, ..l }),
                    Command::Debounce(d) => change_performance(&state, |p| Performance { debounce: d, ..p }),
                    Command::Sleep(s) => change_performance(&state, |p| Performance { sleep: s, ..p }),
                    Command::System(n) => crate::set_modes(app.state(), Modes { system: n, ..current_modes(&state)? }).map(drop),
                    Command::Knob(n) => crate::set_modes(app.state(), Modes { knob: n, ..current_modes(&state)? }).map(drop),
                    Command::Restore => crate::restore_original(app.clone(), app.state()),
                    Command::Login => crate::login::set(!crate::login::enabled()),
                    Command::Layout(iso) => crate::apply_layout(&app, iso),
                    Command::Editor | Command::Permissions | Command::Quit => Ok(()),
                });
                if let Err(e) = result {
                    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("Couldn't save: {e}"));
                }
                refresh(&app, true);
            });
        }
    }
}

fn change_lighting(state: &Shared, f: impl FnOnce(Lighting) -> Lighting) -> Result<(), String> {
    with_keyboard(state, |k| {
        let c = config(k)?;
        let next = c.with_lighting(f(c.lighting()))?;
        k.write(Block::Config, next.bytes())?;
        Ok(())
    })
}

fn current_modes(state: &Shared) -> Result<Modes, String> {
    with_keyboard(state, |k| Ok(config(k)?.modes()))
}

fn change_performance(state: &Shared, f: impl FnOnce(Performance) -> Performance) -> Result<(), String> {
    with_keyboard(state, |k| {
        let c = config(k)?;
        let next = c.with_performance(f(c.performance()))?;
        k.write(Block::Config, next.bytes())?;
        Ok(())
    })
}

/// Whether the user opened the app (Finder, Launchpad, Spotlight, `open`), rather than macOS starting it at
/// login: a login launch carries the "launched as login item" flag, or comes from launchd with no open
/// event at all. Only meaningful while the app finishes launching.
#[cfg(target_os = "macos")]
pub fn opened_by_user() -> bool {
    use objc2::{msg_send, rc::Retained};
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager};
    const OPEN_APPLICATION: u32 = u32::from_be_bytes(*b"oapp");
    const PROPERTY_DATA: u32 = u32::from_be_bytes(*b"prdt");
    const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");
    // SAFETY: read-only queries of the Apple event being handled, on the main thread. `eventID` returns a
    // FourCharCode and `paramDescriptorForKeyword:` takes one and returns a descriptor or nil.
    unsafe {
        let Some(event) = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent() else { return false };
        let id: u32 = msg_send![&*event, eventID];
        let property: Option<Retained<NSAppleEventDescriptor>> = msg_send![&*event, paramDescriptorForKeyword: PROPERTY_DATA];
        id == OPEN_APPLICATION && !property.is_some_and(|d| d.enumCodeValue() == LAUNCHED_AS_LOGIN_ITEM)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn opened_by_user() -> bool {
    true
}

/// The key editor is the only web view, created on demand and freed when its window closes.
pub fn open_editor(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("editor") {
        let _ = window.set_focus();
        return;
    }
    let builder = WebviewWindowBuilder::new(app, "editor", WebviewUrl::default())
        .title("F75 Studio")
        .inner_size(1180.0, 860.0)
        .min_inner_size(980.0, 700.0);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    if let Ok(window) = builder.build() {
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Config;

    /// The menu for the real keyboard's settings (fixture), on the cable and charging.
    fn connected() -> View {
        let c = Config::parse(include_bytes!("../fixtures/config.bin")).unwrap();
        View {
            connection: Connection::Usb,
            battery: Some(Battery { level: 0, charging: true }),
            lighting: Some(c.lighting()),
            performance: Some(c.performance()),
            modes: Some(c.modes()),
            palette: Some(FACTORY_PALETTE),
            problem: None,
            login: Some(true),
            iso: false,
        }
    }

    fn group<'a>(es: &'a [Entry], name: &str) -> (&'a bool, &'a [Entry]) {
        es.iter()
            .find_map(|e| match e {
                Entry::Group { label, enabled, entries } if label == name => Some((enabled, entries.as_slice())),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {name} group"))
    }

    fn checked(es: &[Entry]) -> Vec<(&str, &str)> {
        es.iter()
            .filter_map(|e| match e {
                Entry::Choice { id, label, checked: true } => Some((id.as_str(), label.as_str())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn menu_shows_the_keyboard_state_with_the_current_choices_checked() {
        let es = entries(&connected());
        assert_eq!(es[0], Entry::Info("F75 · USB-C · Charging".into()));
        assert_eq!(es[1], Entry::Info("The F75 doesn't send its battery level over the cable".into()));
        assert_eq!(checked(group(&es, "Effect").1), [("fx:11", "Neon stream")]);
        assert_eq!(checked(group(&es, "Brightness").1), [("br:9", "100%")]);
        assert_eq!(checked(group(&es, "Speed").1), [("sp:3", "Fast")]);
        assert_eq!(checked(group(&es, "Colour").1), [("co:7", "Colourful")]);
        assert_eq!(checked(group(&es, "Debounce").1), [("db:2", "2")]);
        assert_eq!(checked(group(&es, "Sleep").1), [("sl:2", "1 min")]);
        assert!(es.contains(&Entry::Action { id: "editor".into(), label: "Keys, Macros and Colours…".into() }));
        let settings = group(&es, "Settings").1;
        assert_eq!(checked(settings), [("login", "Open at Login")]);
        assert_eq!(checked(group(settings, "Keyboard Layout").1), [("layout:ansi", "ANSI (US)")]);
        assert_eq!(es[es.len() - 2], Entry::About);
        assert_eq!(es.last(), Some(&Entry::Action { id: "quit".into(), label: "Quit F75 Studio".into() }));
    }

    #[test]
    fn settings_an_effect_does_not_use_are_disabled() {
        let mut off = connected();
        off.lighting = Some(Lighting { effect: 0, brightness: 0, speed: 0, colour: 0 });
        let es = entries(&off);
        assert!(*group(&es, "Effect").0);
        for name in ["Brightness", "Speed", "Colour"] {
            assert!(!*group(&es, name).0, "{name} should be disabled for Off");
        }
    }

    #[test]
    fn without_a_wired_keyboard_only_the_status_and_editor_work() {
        let es = entries(&View {
            connection: Connection::None,
            battery: None,
            lighting: None,
            performance: None,
            modes: None,
            palette: None,
            problem: None,
            login: Some(false),
            iso: true,
        });
        assert_eq!(es[0], Entry::Info("F75 not connected · plug in USB-C".into()));
        for name in ["Effect", "Brightness", "Speed", "Colour", "Debounce", "Sleep", "System", "Knob", "Restore Original Settings"] {
            assert!(!*group(&es, name).0, "{name} should be disabled");
        }
        let settings = group(&es, "Settings");
        assert!(*settings.0, "settings don't need the keyboard");
        assert_eq!(checked(group(settings.1, "Keyboard Layout").1), [("layout:iso", "ISO")]);
    }

    #[test]
    fn colours_changed_in_the_editor_are_shown_as_custom() {
        let mut v = connected();
        v.lighting = Some(Lighting { effect: 2, brightness: 9, speed: 3, colour: 2 });
        let mut palette = FACTORY_PALETTE;
        palette[2] = [0, 128, 128];
        v.palette = Some(palette);
        let es = entries(&v);
        let colours = group(&es, "Colour").1;
        assert_eq!(checked(colours), [("co:2", "Custom #008080")]);
        assert!(colours.contains(&Entry::Choice { id: "co:0".into(), label: "Red".into(), checked: false }));
    }

    #[test]
    fn the_system_and_knob_modes_are_one_click_away() {
        let es = entries(&connected());
        assert_eq!(checked(group(&es, "System").1), [("os:0", "Windows (Fn+W)")]);
        assert_eq!(group(&es, "System").1[0], Entry::Choice { id: "os:2".into(), label: "Mac (Fn+E)".into(), checked: false });
        assert_eq!(checked(group(&es, "Knob").1), [("kn:0", "Volume and mute")]);
        assert_eq!(command("os:2"), Some(Command::System(2)));
        assert_eq!(command("kn:1"), Some(Command::Knob(1)));
    }

    #[test]
    fn menu_ids_parse_back_into_commands() {
        assert_eq!(command("fx:11"), Some(Command::Effect(11)));
        assert_eq!(command("sl:20"), Some(Command::Sleep(20)));
        assert_eq!(command("editor"), Some(Command::Editor));
        assert_eq!(command("login"), Some(Command::Login));
        assert_eq!(command("layout:iso"), Some(Command::Layout(true)));
        assert_eq!(command("br:x"), None);
        assert_eq!(command("nope"), None);
    }
}
