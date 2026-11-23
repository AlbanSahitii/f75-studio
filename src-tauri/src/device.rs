//! The only module that touches hardware: finds the F75's vendor interface and runs protocol requests.
use crate::protocol::{self, Block, PACKET_LEN, REPORT_ID};
use hidapi::{HidApi, HidDevice, HidResult};
use std::{thread::sleep, time::Duration};

const WIRED: (u16, u16) = (0x258a, 0x010c);
const WIRELESS: [(u16, u16); 2] = [(0x3554, 0xfa09), (0x3554, 0xfa07)];
const VENDOR_PAGE: u16 = 0xff00;
/// The vendor app waits 20 ms around each request and retries failed transfers 3 times, 70 ms apart.
const GAP: Duration = Duration::from_millis(20);
const RETRY: Duration = Duration::from_millis(70);

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Usb,
    Wireless,
    None,
}

pub fn connection(api: &HidApi) -> Connection {
    let ids: Vec<_> = api.device_list().map(|d| (d.vendor_id(), d.product_id())).collect();
    if ids.contains(&WIRED) {
        Connection::Usb
    } else if ids.iter().any(|id| WIRELESS.contains(id)) {
        Connection::Wireless
    } else {
        Connection::None
    }
}

pub struct Keyboard {
    dev: HidDevice,
}

fn retry<T>(mut f: impl FnMut() -> HidResult<T>) -> Result<T, String> {
    let mut last = None;
    for _ in 0..3 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => last = Some(e),
        }
        sleep(RETRY);
    }
    Err(format!("keyboard did not respond: {}", last.unwrap()))
}

impl Keyboard {
    /// Opens the wired vendor interface. Each OS lists it differently, so keep the first that answers.
    pub fn open(api: &HidApi) -> Result<Keyboard, String> {
        let mut why = "Connect your F75 with USB-C to customize it.".to_string();
        for d in api.device_list().filter(|d| (d.vendor_id(), d.product_id()) == WIRED && d.usage_page() == VENDOR_PAGE) {
            match d.open_device(api) {
                Ok(dev) => {
                    let keyboard = Keyboard { dev };
                    match keyboard.read(Block::Config) {
                        Ok(_) => return Ok(keyboard),
                        Err(e) => why = e,
                    }
                }
                // kIOReturnNotPermitted: macOS wants Input Monitoring for devices that include a keyboard.
                Err(e) if e.to_string().contains("E00002E2") || e.to_string().contains("not permitted") => {
                    why = "macOS is blocking access to the keyboard. Allow F75 Studio in System Settings › \
                           Privacy & Security › Input Monitoring, then reopen the app. If it's already on, \
                           remove it with − and add it again."
                        .into()
                }
                Err(e) => why = e.to_string(),
            }
        }
        Err(why)
    }

    /// Reads a block chunk by chunk: send the request header, wait, then fetch the reply.
    pub fn read(&self, block: Block) -> Result<Vec<u8>, String> {
        let mut data = Vec::with_capacity(block.len());
        for request in protocol::read_requests(block) {
            sleep(GAP);
            retry(|| self.dev.send_feature_report(&request))?;
            sleep(GAP);
            let mut reply = [0u8; PACKET_LEN];
            reply[0] = REPORT_ID;
            let n = retry(|| self.dev.get_feature_report(&mut reply))?;
            data.extend(protocol::reply_data(&request, &reply[..n]).map_err(|e| e.to_string())?);
        }
        Ok(data)
    }

    /// Writes a block, reads it back, and fails unless the keyboard stored exactly what was sent.
    pub fn write(&self, block: Block, data: &[u8]) -> Result<(), String> {
        for packet in protocol::write_requests(block, data).map_err(|e| e.to_string())? {
            sleep(GAP);
            retry(|| self.dev.send_feature_report(&packet))?;
        }
        sleep(Duration::from_millis(60));
        if self.read(block)? != data {
            return Err("the keyboard did not store the change".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Config, KeyColours, Layer, Palette};
    use std::sync::{Mutex, MutexGuard};

    /// Hardware tests share one keyboard, so they take turns.
    static TURN: Mutex<()> = Mutex::new(());

    fn keyboard() -> (MutexGuard<'static, ()>, Keyboard) {
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        (turn, crate::with_api(|api| Keyboard::open(api)).expect("no F75 on USB-C"))
    }

    /// Puts a block's original bytes back when dropped, even when a check failed first.
    struct Restore<'a> {
        k: &'a Keyboard,
        block: Block,
        original: Vec<u8>,
    }

    impl<'a> Restore<'a> {
        fn read(k: &'a Keyboard, block: Block) -> Restore<'a> {
            Restore { k, block, original: k.read(block).expect("read the original") }
        }
    }

    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            if let Err(e) = self.k.write(self.block, &self.original) {
                eprintln!("COULDN'T RESTORE {:?}: {e}", self.block);
            }
        }
    }

    /// Read-only check against a real F75 on USB-C: `cargo test -- --ignored reads_a_connected_keyboard`.
    #[test]
    #[ignore]
    fn reads_a_connected_keyboard() {
        let (_turn, k) = keyboard();
        let config = Config::parse(&k.read(Block::Config).unwrap()).unwrap();
        for layer in 0..4 {
            Layer::parse(&k.read(Block::Layer(layer)).unwrap()).unwrap();
        }
        protocol::battery(&k.read(Block::Battery).unwrap());
        KeyColours::parse(&k.read(Block::Colours(config.colour_group())).unwrap()).unwrap();
        Palette::parse(&k.read(Block::Palette).unwrap()).unwrap();
        protocol::parse_macros(&k.read(Block::Macros).unwrap()).unwrap();
    }

    /// Writes test values to a connected F75, checks each was stored, and puts the originals back. It changes
    /// the keyboard, so it only runs on request: `F75_WRITE_TEST=1 cargo test -- --ignored writes_and_restores`.
    #[test]
    #[ignore]
    fn writes_and_restores_colours_palette_macros_and_keys() {
        if std::env::var_os("F75_WRITE_TEST").is_none() {
            eprintln!("skipped: set F75_WRITE_TEST=1 to let this test write to the keyboard");
            return;
        }
        let (_turn, k) = keyboard();
        // Byte 3 is the red of the "Off" effect's second colour, and of the fourth key's per-key colour.
        for block in [Block::Palette, Block::Colours(0)] {
            let restore = Restore::read(&k, block);
            let mut changed = restore.original.clone();
            changed[3] ^= 0x01;
            k.write(block, &changed).unwrap();
        }
        // The keyboard hands back exactly the fixture bytes, which were captured from it this way. Layer 2
        // is FN2, which only a key set to FN2 opens.
        let _macros = Restore::read(&k, Block::Macros);
        let _fn2 = Restore::read(&k, Block::Layer(2));
        k.write(Block::Macros, include_bytes!("../fixtures/macros.bin")).unwrap();
        k.write(Block::Layer(2), include_bytes!("../fixtures/layer2-functions.bin")).unwrap();
    }
}
