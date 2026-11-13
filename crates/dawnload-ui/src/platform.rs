//! What each OS does its own way: opening a file, showing it in the file manager, notifications,
//! the system's animation setting, the app's identity (Windows' AUMID, Linux's app ID and
//! desktop entry), and whether there is a tray to close to.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The app's identity everywhere one is needed: the Wayland app ID, the desktop entry's name,
/// Windows' AppUserModelID. Reverse DNS of where the project lives (ADR 0010).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const APP_ID: &str = "io.github.dawnwebs.Dawnload";

pub const REPO_URL: &str = "https://github.com/dawnwebs/Dawnload";
pub const LICENCES_URL: &str = "https://github.com/dawnwebs/Dawnload/blob/main/LICENSE";

const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");

/// Per-OS words: the modifier key, what "show in the file manager" is called, and what the
/// person's machine is called in sentences.
pub fn words() -> (&'static str, &'static str, &'static str) {
    if cfg!(target_os = "macos") { ("⌘", "Show in Finder", "this Mac") } else { ("Ctrl", "Show in Folder", "this computer") }
}

fn spawn(mut command: Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        command.creation_flags(0x0800_0000);
    }
    if let Err(e) = command.spawn() {
        eprintln!("dawnload-app: couldn't run {command:?}: {e}");
    }
}

/// Opens a file with its default app, or a folder in the file manager.
pub fn open_path(path: &Path) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{HSTRING, w};
        let target = HSTRING::from(path.as_os_str());
        unsafe {
            ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        spawn(command);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        spawn(command);
    }
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{HSTRING, w};
        unsafe {
            ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL);
        }
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        command.arg(url);
        spawn(command);
    }
}

/// Shows a file selected in the file manager (Explorer, Finder, or the desktop's own through
/// org.freedesktop.FileManager1), or opens a folder.
pub fn reveal(path: &Path) {
    if path.is_dir() {
        return open_path(path);
    }
    if !path.exists() {
        // Gone or not written yet: show the folder it would be in.
        if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
            open_path(parent);
        }
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // explorer parses its own command line: /select,"C:\a b\c.mp4" must not be re-quoted.
        let mut command = Command::new("explorer.exe");
        command.raw_arg(format!("/select,\"{}\"", path.display()));
        spawn(command);
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        spawn(command);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            if linux::show_items(&path).is_err()
                && let Some(parent) = path.parent()
            {
                let mut command = Command::new("xdg-open");
                command.arg(parent);
                spawn(command);
            }
        });
    }
}

/// The system's "reduce motion" / "animation effects" setting.
pub fn system_reduce_motion() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};
        let mut enabled: i32 = 1;
        let ok = unsafe {
            SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut enabled as *mut i32 as *mut _), Default::default())
        };
        ok.is_ok() && enabled == 0
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
            .unwrap_or(false)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "false")
            .unwrap_or(false)
    }
}

/// Is there a tray to close to? Windows and macOS always have one; a Linux desktop needs a
/// StatusNotifier host (KDE, and GNOME with the AppIndicator extension, which Ubuntu ships).
pub fn tray_available() -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::has_status_notifier_host()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        true
    }
}

/// Gives the process its identity before any window opens. Windows: the AppUserModelID, so the
/// taskbar groups Dawnload and toasts can name it. Linux: the app ID, and a desktop entry and
/// icon in the person's own data folder so the dock shows Dawnload's name and icon (packages
/// will install their own; this is for the plain binary).
pub fn init_identity(data_dir: &Path) {
    #[cfg(windows)]
    windows_impl::register(data_dir);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = slint::set_xdg_app_id(APP_ID);
        if std::env::var_os("FLATPAK_ID").is_none() && std::env::var_os("DAWNLOAD_NO_DESKTOP_ENTRY").is_none() {
            linux::install_desktop_entry();
        }
        let _ = data_dir;
    }
    #[cfg(target_os = "macos")]
    let _ = data_dir;
}

/// Sends notifications from a thread of its own, so a slow notification service never holds
/// up the window.
pub struct Notifier {
    tx: std::sync::mpsc::Sender<(String, String)>,
}

impl Notifier {
    pub fn new(data_dir: &Path) -> Notifier {
        let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();
        let icon = icon_file(data_dir);
        std::thread::Builder::new()
            .name("notifications".into())
            .spawn(move || {
                for (title, body) in rx {
                    if let Err(e) = show_notification(&title, &body, icon.as_deref()) {
                        eprintln!("dawnload-app: notification not shown: {e}");
                    }
                }
            })
            .ok();
        Notifier { tx }
    }

    pub fn notify(&self, title: String, body: String) {
        let _ = self.tx.send((title, body));
    }
}

/// The app icon as a file, for notifications that take a path.
fn icon_file(data_dir: &Path) -> Option<PathBuf> {
    let path = data_dir.join("dawnload-icon.png");
    if std::fs::metadata(&path).map(|m| m.len() as usize != ICON_PNG.len()).unwrap_or(true) {
        std::fs::create_dir_all(data_dir).ok()?;
        std::fs::write(&path, ICON_PNG).ok()?;
    }
    Some(path)
}

#[allow(unused_variables)]
fn show_notification(title: &str, body: &str, icon: Option<&Path>) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_impl::toast(title, body).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        // Until the app is bundled and signed (phase 5), AppleScript's notification is the one
        // that works for a plain binary.
        let script = format!(
            "display notification {} with title {}",
            applescript_string(body),
            applescript_string(&format!("Dawnload · {title}"))
        );
        Command::new("osascript").arg("-e").arg(script).status().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::notify(title, body, icon).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Escapes text for the toast's XML.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

#[cfg(windows)]
mod windows_impl {
    use super::{APP_ID, xml_escape};
    use std::path::Path;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::HSTRING;

    fn reg_set(subkey: &str, name: &str, value: &str) {
        let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(subkey),
                &HSTRING::from(name),
                REG_SZ.0,
                Some(data.as_ptr() as *const _),
                (data.len() * 2) as u32,
            );
        }
    }

    /// The AUMID for this process, and its registration under HKCU so toasts show "Dawnload"
    /// and the icon without a Start menu shortcut (an installer adds one in phase 5).
    pub fn register(data_dir: &Path) {
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_ID));
        }
        let key = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
        reg_set(&key, "DisplayName", "Dawnload");
        if let Some(icon) = super::icon_file(data_dir) {
            reg_set(&key, "IconUri", &icon.display().to_string());
        }
    }

    pub fn toast(title: &str, body: &str) -> windows::core::Result<()> {
        let xml = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml_escape(title),
            xml_escape(body)
        );
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::{APP_ID, ICON_PNG};
    use std::collections::HashMap;
    use std::path::Path;
    use zbus::blocking::Connection;
    use zbus::zvariant::Value;

    pub fn notify(title: &str, body: &str, icon: Option<&Path>) -> zbus::Result<()> {
        let connection = Connection::session()?;
        let mut hints: HashMap<&str, Value> = HashMap::new();
        hints.insert("desktop-entry", Value::from(APP_ID));
        let icon = icon.map(|p| p.display().to_string()).unwrap_or_default();
        connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("Dawnload", 0u32, icon.as_str(), title, body, Vec::<&str>::new(), hints, -1i32),
        )?;
        Ok(())
    }

    pub fn show_items(path: &Path) -> zbus::Result<()> {
        let connection = Connection::session()?;
        let uri = format!("file://{}", percent_path(path));
        connection.call_method(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            Some("org.freedesktop.FileManager1"),
            "ShowItems",
            &(vec![uri.as_str()], ""),
        )?;
        Ok(())
    }

    fn percent_path(path: &Path) -> String {
        let mut out = String::new();
        for byte in path.as_os_str().as_encoded_bytes() {
            match *byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }

    pub fn has_status_notifier_host() -> bool {
        let Ok(connection) = Connection::session() else { return false };
        let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&connection) else { return false };
        ["org.kde.StatusNotifierWatcher", "org.freedesktop.StatusNotifierWatcher"]
            .into_iter()
            .any(|name| name.try_into().ok().and_then(|n| proxy.name_has_owner(n).ok()).unwrap_or(false))
    }

    /// ~/.local/share/applications/<id>.desktop and its icon, rewritten when the binary moves.
    pub fn install_desktop_entry() {
        let Some(data) = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        else {
            return;
        };
        let Ok(exe) = std::env::current_exe() else { return };
        let icon_dir = data.join("icons/hicolor/256x256/apps");
        let icon = icon_dir.join(format!("{APP_ID}.png"));
        if std::fs::metadata(&icon).map(|m| m.len() as usize != ICON_PNG.len()).unwrap_or(true) {
            let _ = std::fs::create_dir_all(&icon_dir);
            let _ = std::fs::write(&icon, ICON_PNG);
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Dawnload\nComment=Paste a link and keep the file\nExec=\"{}\" %U\nIcon={APP_ID}\nTerminal=false\nCategories=Network;FileTransfer;\nStartupWMClass={APP_ID}\n",
            exe.display()
        );
        let apps = data.join("applications");
        let path = apps.join(format!("{APP_ID}.desktop"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(entry.as_str()) {
            let _ = std::fs::create_dir_all(&apps);
            let _ = std::fs::write(&path, entry);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn toast_text_is_escaped() {
        assert_eq!(super::xml_escape("Tom & Jerry <1>"), "Tom &amp; Jerry &lt;1&gt;");
    }
}
