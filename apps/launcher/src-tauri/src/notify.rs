//! System notifications (Settings → Genel "Masaüstü bildirimleri"). The page decides when one is
//! worth it (`src/lib/attention.ts`); this shows it.
//!
//! Windows: a toast under this process's own AppUserModelID (`pl_desktop::toast`), which every
//! start names for Windows with the product name and the launcher's picture. So it reads
//! "Projects Launcher" (a debug build: "Projects Launcher (debug <instance>)") whatever folder the
//! exe runs from and whether or not a Start menu shortcut exists; the notification plugin sent from
//! development folders as Windows PowerShell (NOTIF1). A click brings the launcher forward the way
//! Windows allows (`shell::focus_main_window`: the taskbar button flashes when it refuses) and
//! tells the page which of its notifications it was, so the page opens what it was about.
//! Elsewhere the notification plugin shows it, without clicks.

use tauri::AppHandle;

/// Windows: the process's windows and notifications take the identity's AppUserModelID, the one
/// the installer's shortcuts carry, whatever started the launcher (the Start menu, the login item,
/// a `projects://` link, an update). Before any window exists.
pub fn adopt_app_id() {
    #[cfg(windows)]
    windows_toast::adopt_app_id();
}

/// Windows: names this process's AppUserModelID for notifications (the picture file and the
/// per-user registry values), off the main thread.
pub fn register(app: &AppHandle) {
    #[cfg(windows)]
    {
        let product = app
            .config()
            .product_name
            .clone()
            .unwrap_or_else(|| "Projects Launcher".to_owned());
        let name = crate::identity::current().label(&product);
        tauri::async_runtime::spawn_blocking(move || windows_toast::register(&name));
    }
    #[cfg(not(windows))]
    let _ = app;
}

/// Shows a system notification; true when the system took it. On Windows a click comes back to
/// the page as the `desktop-notification-click` event with `id`. False when the player turned the
/// launcher's notifications off in Windows, so the page keeps its own toast.
#[tauri::command]
#[specta::specta]
pub async fn desktop_notify(app: AppHandle, title: String, body: Option<String>, id: u32) -> bool {
    show(&app, &title, body.as_deref(), id)
}

#[cfg(windows)]
fn show(app: &AppHandle, title: &str, body: Option<&str>, id: u32) -> bool {
    match windows_toast::show(app, title, body, id) {
        Ok(shown) => shown,
        Err(error) => {
            tracing::warn!(%error, "notification not shown");
            false
        }
    }
}

#[cfg(not(windows))]
fn show(app: &AppHandle, title: &str, body: Option<&str>, _id: u32) -> bool {
    use tauri_plugin_notification::NotificationExt;

    let mut notification = app.notification().builder().title(title);
    if let Some(body) = body {
        notification = notification.body(body);
    }
    notification.show().is_ok()
}

#[cfg(windows)]
mod windows_toast {
    use std::path::{Path, PathBuf};

    use pl_desktop::toast::{APP_IDS_KEY, ICON_FILE, toast_xml};
    use tauri::{AppHandle, Emitter};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        NotificationSetting, ToastNotification, ToastNotificationManager,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_CREATE_SUB_KEY, KEY_SET_VALUE, REG_OPEN_CREATE_OPTIONS,
        REG_OPTION_NON_VOLATILE, REG_OPTION_VOLATILE, REG_SAM_FLAGS, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::{HSTRING, IInspectable, PCWSTR};

    /// The page's event for a click on one of its notifications, with the number it gave it.
    const CLICK_EVENT: &str = "desktop-notification-click";

    /// The picture on the launcher's notifications and in Windows' notification settings.
    const ICON: &[u8] = include_bytes!("../icons/128x128.png");

    fn app_id() -> &'static str {
        &crate::identity::current().identifier
    }

    pub fn adopt_app_id() {
        // SAFETY: a plain Win32 call with a string that outlives it.
        if let Err(error) =
            unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id())) }
        {
            tracing::warn!(%error, "the process kept the AppUserModelID Windows gave it");
        }
    }

    pub fn register(name: &str) {
        let icon = save_icon();
        match write_registration(name, icon.as_deref()) {
            Ok(()) => tracing::debug!(app_id = app_id(), name, "notifications named for Windows"),
            Err(error) => tracing::warn!(%error, "notifications not named for Windows"),
        }
    }

    /// The picture next to the launcher's other per-machine files (`<local data>/<identifier>/`,
    /// as the shortcut pictures), written when missing or different.
    fn save_icon() -> Option<PathBuf> {
        let path = dirs::data_local_dir()?.join(app_id()).join(ICON_FILE);
        if std::fs::read(&path).is_ok_and(|bytes| bytes == ICON) {
            return Some(path);
        }
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, ICON));
        match written {
            Ok(()) => Some(path),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "notification picture not saved");
                None
            }
        }
    }

    /// `HKCU\Software\Classes\AppUserModelId\<app id>` with `DisplayName` and `IconUri`. The key is
    /// volatile: Windows drops it at sign-out and every start writes it again, so nothing stays
    /// behind once the launcher is uninstalled and no installer step is needed. The parent stays a
    /// normal key, because under a volatile key Windows allows only volatile ones and other apps
    /// name their IDs there too.
    fn write_registration(name: &str, icon: Option<&Path>) -> windows::core::Result<()> {
        let parent = Key::create(
            HKEY_CURRENT_USER,
            APP_IDS_KEY,
            REG_OPTION_NON_VOLATILE,
            KEY_CREATE_SUB_KEY,
        )?;
        let key = Key::create(parent.0, app_id(), REG_OPTION_VOLATILE, KEY_SET_VALUE)?;
        key.set("DisplayName", name)?;
        if let Some(icon) = icon {
            key.set("IconUri", &icon.to_string_lossy())?;
        }
        Ok(())
    }

    /// An open registry key, closed when dropped.
    struct Key(HKEY);

    impl Key {
        fn create(
            parent: HKEY,
            path: &str,
            options: REG_OPEN_CREATE_OPTIONS,
            access: REG_SAM_FLAGS,
        ) -> windows::core::Result<Self> {
            let mut key = HKEY::default();
            // SAFETY: `key` receives the opened key; the strings outlive the call.
            unsafe {
                RegCreateKeyExW(
                    parent,
                    &HSTRING::from(path),
                    None,
                    PCWSTR::null(),
                    options,
                    access,
                    None,
                    &mut key,
                    None,
                )
            }
            .ok()?;
            Ok(Self(key))
        }

        fn set(&self, name: &str, value: &str) -> windows::core::Result<()> {
            let data: Vec<u8> = value
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            // SAFETY: a key this guard opened; `data` is a NUL-terminated UTF-16 string.
            unsafe {
                RegSetValueExW(
                    self.0,
                    &HSTRING::from(name),
                    None,
                    REG_SZ,
                    Some(data.as_slice()),
                )
            }
            .ok()
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: closes the key this guard opened, once.
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }

    pub fn show(
        app: &AppHandle,
        title: &str,
        body: Option<&str>,
        id: u32,
    ) -> windows::core::Result<bool> {
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id()))?;
        // Turned off for the launcher in Windows' settings: the page shows its own toast instead.
        // Do Not Disturb is not this: Windows keeps those notifications for later by itself.
        if notifier
            .Setting()
            .is_ok_and(|setting| setting != NotificationSetting::Enabled)
        {
            return Ok(false);
        }
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        let handle = app.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, _| {
                clicked(&handle, id);
                Ok(())
            },
        ))?;
        notifier.Show(&toast)?;
        Ok(true)
    }

    /// A click on a notification (on screen; Windows reports none once it moved to the
    /// notification center): the launcher comes forward where Windows allows it, the page wakes and
    /// opens what the notification was about.
    fn clicked(app: &AppHandle, id: u32) {
        tracing::debug!(id, "notification clicked");
        crate::shell::focus_main_window(app);
        crate::webview_rest::wake(app, "notification click");
        let _ = app.emit(CLICK_EVENT, id);
    }
}
