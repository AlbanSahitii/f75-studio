//! Open at Login through macOS's own login-item service, so the app is listed (and can be switched off)
//! under System Settings › General › Login Items.
#[cfg(target_os = "macos")]
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Open at Login is offered on macOS only, for now.
pub const SUPPORTED: bool = cfg!(target_os = "macos");

#[cfg(target_os = "macos")]
pub fn enabled() -> bool {
    // SAFETY: read-only ServiceManagement query about this app's own login item.
    unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
}

#[cfg(target_os = "macos")]
pub fn set(on: bool) -> Result<(), String> {
    // SAFETY: registers or removes this app's own login item; macOS reports failures as NSError.
    let service = unsafe { SMAppService::mainAppService() };
    let result = unsafe { if on { service.registerAndReturnError() } else { service.unregisterAndReturnError() } };
    result.map_err(|e| format!("macOS refused the login item: {e:?}"))
}

#[cfg(not(target_os = "macos"))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn set(_on: bool) -> Result<(), String> {
    Err("Open at Login isn't supported on this platform yet".into())
}
