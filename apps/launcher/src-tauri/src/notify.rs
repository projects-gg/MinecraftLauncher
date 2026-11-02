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
//!
//! CHAT-CALM: a chat's notification carries its conversation as a tag. One conversation has one
//! notification: a new one takes the old one's place, `popup: false` changes the words of the one
//! shown (on screen or in the notification center) in place without popping up, and
//! `desktop_notify_clear` takes it away once the player read the conversation in the launcher.

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
///
/// `tag` (CHAT-CALM, Windows): the conversation it is about. With `popup` it takes the place of the
/// conversation's notification and pops up; without, the one shown says the new words in place
/// (none shown, the player closed it: nothing happens, and that is fine too).
#[tauri::command]
#[specta::specta]
pub async fn desktop_notify(
    app: AppHandle,
    title: String,
    body: Option<String>,
    id: u32,
    tag: Option<String>,
    popup: bool,
) -> bool {
    show(&app, &title, body.as_deref(), id, tag.as_deref(), popup)
}

/// Takes the conversation's notification (`tag`, CHAT-CALM) away from the notification center:
/// the player read it in the launcher. Never fails.
#[tauri::command]
#[specta::specta]
pub async fn desktop_notify_clear(tag: String) {
    #[cfg(windows)]
    if let Err(error) = windows_toast::clear(&tag) {
        tracing::debug!(%error, "notification not taken away");
    }
    #[cfg(not(windows))]
    let _ = tag;
}

/// A voice chat notification (VOICE, `crate::voice`): a ring with "Katıl" and "Reddet" and the
/// call sound (`actions`, their labels and arguments), or a plain one; `tag` (a ring's room) lets
/// [`voice_clear`] take it away. A press comes back through `on_action` (`""` for the body). True
/// when Windows took it.
pub fn voice_show(
    tag: &str,
    title: &str,
    body: Option<&str>,
    actions: Option<[(&str, &str); 2]>,
    on_action: Box<dyn Fn(String) + Send + Sync>,
) -> bool {
    #[cfg(windows)]
    {
        match windows_toast::show_voice(tag, title, body, actions, on_action) {
            Ok(shown) => shown,
            Err(error) => {
                tracing::warn!(%error, "voice notification not shown");
                false
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (tag, title, body, actions, on_action);
        false
    }
}

/// Takes a voice notification away (a ring that ended). Never fails.
pub fn voice_clear(tag: &str) {
    #[cfg(windows)]
    if let Err(error) = windows_toast::clear_voice(tag) {
        tracing::debug!(%error, "voice notification not taken away");
    }
    #[cfg(not(windows))]
    let _ = tag;
}

#[cfg(windows)]
fn show(
    app: &AppHandle,
    title: &str,
    body: Option<&str>,
    id: u32,
    tag: Option<&str>,
    popup: bool,
) -> bool {
    match windows_toast::show(app, title, body, id, tag, popup) {
        Ok(shown) => shown,
        Err(error) => {
            tracing::warn!(%error, "notification not shown");
            false
        }
    }
}

#[cfg(not(windows))]
fn show(
    app: &AppHandle,
    title: &str,
    body: Option<&str>,
    _id: u32,
    tag: Option<&str>,
    popup: bool,
) -> bool {
    use tauri_plugin_notification::NotificationExt;

    // Without tags elsewhere, a change in place would be a new notification: none.
    if tag.is_some() && !popup {
        return true;
    }
    let mut notification = app.notification().builder().title(title);
    if let Some(body) = body {
        notification = notification.body(body);
    }
    notification.show().is_ok()
}

#[cfg(windows)]
mod windows_toast {
    use std::path::{Path, PathBuf};

    use pl_desktop::toast::{
        APP_IDS_KEY, BODY_CHARS, BOUND_TOAST_XML, CHAT_GROUP, ICON_FILE, TITLE_CHARS, toast_tag,
        toast_value, toast_xml,
    };
    use tauri::{AppHandle, Emitter};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        NotificationData, NotificationSetting, NotificationUpdateResult, ToastNotification,
        ToastNotificationManager,
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
        tag: Option<&str>,
        popup: bool,
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
        let toast = match tag {
            None => {
                let xml = XmlDocument::new()?;
                xml.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
                ToastNotification::CreateToastNotification(&xml)?
            }
            Some(key) => {
                // One notification per conversation, its words bound to its data (CHAT-CALM).
                let tag = HSTRING::from(toast_tag(key));
                let group = HSTRING::from(CHAT_GROUP);
                let data = NotificationData::new()?;
                let values = data.Values()?;
                values.Insert(
                    &HSTRING::from("title"),
                    &HSTRING::from(toast_value(title, TITLE_CHARS)),
                )?;
                values.Insert(
                    &HSTRING::from("body"),
                    &HSTRING::from(toast_value(body.unwrap_or_default(), BODY_CHARS)),
                )?;
                if !popup {
                    // The one shown says the new words where it is; closed or gone: nothing until
                    // the next pop-up.
                    let changed = notifier.UpdateWithTagAndGroup(&data, &tag, &group)?;
                    if changed != NotificationUpdateResult::Succeeded {
                        tracing::debug!("chat notification closed: not changed");
                    }
                    return Ok(true);
                }
                let xml = XmlDocument::new()?;
                xml.LoadXml(&HSTRING::from(BOUND_TOAST_XML))?;
                let toast = ToastNotification::CreateToastNotification(&xml)?;
                toast.SetTag(&tag)?;
                toast.SetGroup(&group)?;
                toast.SetData(&data)?;
                toast
            }
        };
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

    /// Voice chat's notifications (VOICE) are one group; a ring's tag is its room.
    const VOICE_GROUP: &str = "voice";

    /// A voice chat notification: a ring (`actions`: "Katıl" and "Reddet", with the looping call
    /// sound until it is answered or taken away) or a plain one. A button's press comes back
    /// through `on_action` with its argument (`join`, `decline`), a click on the body with `""`.
    pub fn show_voice(
        tag: &str,
        title: &str,
        body: Option<&str>,
        actions: Option<[(&str, &str); 2]>,
        on_action: Box<dyn Fn(String) + Send + Sync>,
    ) -> windows::core::Result<bool> {
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id()))?;
        if notifier
            .Setting()
            .is_ok_and(|setting| setting != NotificationSetting::Enabled)
        {
            return Ok(false);
        }
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(voice_xml(title, body, actions)))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        toast.SetTag(&HSTRING::from(toast_tag(tag)))?;
        toast.SetGroup(&HSTRING::from(VOICE_GROUP))?;
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, args| {
                let argument = args
                    .as_ref()
                    .and_then(|args| {
                        windows::core::Interface::cast::<
                            windows::UI::Notifications::ToastActivatedEventArgs,
                        >(args)
                        .ok()
                    })
                    .and_then(|args| args.Arguments().ok())
                    .map(|text| text.to_string_lossy())
                    .unwrap_or_default();
                on_action(argument);
                Ok(())
            },
        ))?;
        notifier.Show(&toast)?;
        Ok(true)
    }

    /// Takes a voice notification away (a ring that ended).
    pub fn clear_voice(tag: &str) -> windows::core::Result<()> {
        ToastNotificationManager::History()?.RemoveGroupedTagWithId(
            &HSTRING::from(toast_tag(tag)),
            &HSTRING::from(VOICE_GROUP),
            &HSTRING::from(app_id()),
        )
    }

    fn voice_xml(title: &str, body: Option<&str>, actions: Option<[(&str, &str); 2]>) -> String {
        let mut xml = String::new();
        if actions.is_some() {
            // Stays on screen with the call sound until answered (Windows' incoming call).
            xml.push_str(r#"<toast scenario="incomingCall" launch="">"#);
        } else {
            xml.push_str(r#"<toast launch="">"#);
        }
        xml.push_str(r#"<visual><binding template="ToastGeneric"><text>"#);
        xml.push_str(&escape(&toast_value(title, TITLE_CHARS)));
        xml.push_str("</text>");
        if let Some(body) = body {
            xml.push_str("<text>");
            xml.push_str(&escape(&toast_value(body, BODY_CHARS)));
            xml.push_str("</text>");
        }
        xml.push_str("</binding></visual>");
        if let Some(actions) = actions {
            xml.push_str("<actions>");
            for (label, argument) in actions {
                xml.push_str(r#"<action activationType="foreground" content=""#);
                xml.push_str(&escape(label));
                xml.push_str(r#"" arguments=""#);
                xml.push_str(&escape(argument));
                xml.push_str(r#""/>"#);
            }
            xml.push_str("</actions>");
            xml.push_str(
                r#"<audio src="ms-winsoundevent:Notification.Looping.Call" loop="true"/>"#,
            );
        }
        xml.push_str("</toast>");
        xml
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// Takes a conversation's notification away from the notification center (CHAT-CALM).
    pub fn clear(key: &str) -> windows::core::Result<()> {
        ToastNotificationManager::History()?.RemoveGroupedTagWithId(
            &HSTRING::from(toast_tag(key)),
            &HSTRING::from(CHAT_GROUP),
            &HSTRING::from(app_id()),
        )
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
