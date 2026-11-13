//! Push-to-talk and the voice shortcuts through Raw Input (design §6.3, VOICE-0's decision): a
//! hidden message window on a thread of its own gets keyboard (and, for a mouse button, mouse)
//! reports with `RIDEV_INPUTSINK`, also while the game is in front; nothing is taken from the
//! game, nothing is polled, and only the chosen keys are looked at (`pl_voice::input::Shortcuts`).
//! A combination ("Ctrl + Shift + M", VOICE-KEYS) asks Windows which modifiers are held only when
//! its key goes down. It runs only while the player is in a room and a shortcut needs it
//! (`pl_voice::input`'s rule).
//!
//! A process has one Raw Input target per device kind, and tao registered its own for device
//! events: the watch reads what was registered before and puts it back when it stops.

use std::sync::atomic::{AtomicBool, Ordering};

use pl_voice::Voice;
use pl_voice::settings::VoiceKey;
use pl_voice::view::VoiceShortcut;

/// SQOL: a launcher text field has the keyboard (`voice_typing`).
static TYPING: AtomicBool = AtomicBool::new(false);

/// SQOL: a launcher text field took the keyboard (`on`) or let it go: meanwhile single-key voice
/// shortcuts do nothing, as long as the launcher is the window in front (a page that forgets to
/// say it let go never keeps the shortcuts from the game).
pub fn set_typing(on: bool) {
    TYPING.store(on, Ordering::SeqCst);
}

/// Milliseconds since the computer was last touched (keyboard, mouse, pen; the inactivity rule).
pub fn idle_ms() -> u64 {
    #[cfg(windows)]
    {
        platform::idle_ms()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Discord's voice keys on this computer (VOICE-KEYS): `%APPDATA%`'s Discord folders, each key as
/// this keyboard's layout has it. Reads only when the page asks (`voice_discord_offer`).
pub fn discord_keys() -> Option<pl_voice::discord::VoiceDiscordOffer> {
    #[cfg(windows)]
    {
        let roaming = std::env::var_os("APPDATA")?;
        pl_voice::discord::read(std::path::Path::new(&roaming), &platform::vk_key)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// A running watch; [`Watch::stop`] ends it (and puts the earlier registrations back).
pub struct Watch {
    #[cfg(windows)]
    window: isize,
    #[cfg(windows)]
    thread: std::thread::JoinHandle<()>,
}

impl Watch {
    /// Starts watching `keys` for `voice`; None when Raw Input could not be had (the shortcuts
    /// then do nothing, the call goes on).
    pub fn start(keys: &[(VoiceShortcut, VoiceKey)], voice: Voice) -> Option<Self> {
        #[cfg(windows)]
        {
            platform::start(keys, voice).map(|(window, thread)| Self { window, thread })
        }
        #[cfg(not(windows))]
        {
            let _ = (keys, voice);
            None
        }
    }

    /// Ends the watch and waits until the earlier registrations are back, so a new watch never
    /// races the old one's restore.
    pub fn stop(self) {
        #[cfg(windows)]
        {
            platform::stop(self.window);
            if self.thread.thread().id() != std::thread::current().id() {
                let _ = self.thread.join();
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;
    use std::sync::mpsc;
    use std::time::Duration;

    use pl_voice::Voice;
    use pl_voice::input::{Held, KeyChange, Shortcuts};
    use pl_voice::keys::ScanCode;
    use pl_voice::settings::VoiceKey;
    use pl_voice::view::VoiceShortcut;
    use windows::Win32::Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyboardLayout, GetLastInputInfo, LASTINPUTINFO, MAPVK_VK_TO_CHAR,
        MAPVK_VK_TO_VSC_EX, MapVirtualKeyExW, VIRTUAL_KEY, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
        VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    };
    use windows::Win32::UI::Input::{
        GetRawInputData, GetRegisteredRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
        RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
        RegisterRawInputDevices,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
        GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE, MSG, PostMessageW, PostQuitMessage,
        RegisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_INPUT, WNDCLASSW,
    };
    use windows::core::w;

    /// HID usages (generic desktop page): mouse and keyboard.
    const USAGE_PAGE: u16 = 1;
    const USAGE_MOUSE: u16 = 2;
    const USAGE_KEYBOARD: u16 = 6;

    thread_local! {
        /// The watched keys and who hears their changes, on the input thread.
        static WATCH: RefCell<Option<(Shortcuts, Voice)>> = const { RefCell::new(None) };
    }

    /// SQOL: the player types in a launcher text field now: it said so, and a window of the
    /// launcher is the one in front. SQOL-O: or a running game's overlay says a text field of the
    /// game has the keyboard (the game's window in front; it stops saying so when it is not).
    fn typing() -> bool {
        if crate::overlay::game_typing() {
            return true;
        }
        if !super::TYPING.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
        let mut pid = 0u32;
        // SAFETY: plain queries of the window in front and its process.
        let _ = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        pid == std::process::id()
    }

    /// The modifiers held now, from Windows' own key state (asked only when a combination's key
    /// goes down: a modifier pressed before the watch began, or let go on another desktop, still
    /// reads right). AltGr is Right Alt with a Left Ctrl Windows adds.
    fn held() -> Held {
        // SAFETY: a plain query of one key's state.
        let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0;
        Held {
            ctrl: down(VK_LCONTROL) || down(VK_RCONTROL),
            shift: down(VK_LSHIFT) || down(VK_RSHIFT),
            alt: down(VK_LMENU),
            alt_gr: down(VK_RMENU),
        }
    }

    /// A virtual-key code as the player's keyboard has it (the layout of the window in front, the
    /// launcher's while its page asks): the key's scan code, and the character on it, upper-cased
    /// (a Turkish layout's "i" is "İ"); empty for keys that write none.
    pub(super) fn vk_key(vk: u16) -> Option<(ScanCode, String)> {
        // SAFETY: plain queries of a window's thread and its keyboard layout.
        let layout = unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            GetKeyboardLayout(thread)
        };
        // SAFETY: as above.
        let scan = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_VSC_EX, Some(layout)) };
        let make = u16::try_from(scan & 0xFF).ok().filter(|make| *make != 0)?;
        let extended = match scan >> 8 {
            0 => false,
            0xE0 => true,
            // E1 (Pause) and anything else: not a key a shortcut uses.
            _ => return None,
        };
        // SAFETY: as above; dead keys carry the top bit.
        let written = unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_CHAR, Some(layout)) };
        let turkish = (layout.0 as usize & 0x3FF) == 0x1F;
        let label = char::from_u32(written & 0x7FFF_FFFF)
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .map(|c| {
                if turkish && c == 'i' {
                    "İ".to_owned()
                } else {
                    c.to_uppercase().collect()
                }
            })
            .unwrap_or_default();
        Some((ScanCode { make, extended }, label))
    }

    pub(super) fn idle_ms() -> u64 {
        let mut info = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: plain Win32 calls with a struct sized for them.
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return 0;
        }
        u64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime))
    }

    pub(super) fn start(
        keys: &[(VoiceShortcut, VoiceKey)],
        voice: Voice,
    ) -> Option<(isize, std::thread::JoinHandle<()>)> {
        let shortcuts = Shortcuts::new(keys);
        if shortcuts.is_empty() {
            return None;
        }
        let (ready, started) = mpsc::channel();
        let thread = match std::thread::Builder::new()
            .name("voice-keys".to_owned())
            .spawn(move || run(shortcuts, voice, ready))
        {
            Ok(thread) => thread,
            Err(error) => {
                tracing::warn!(%error, "voice: no thread for the shortcuts");
                return None;
            }
        };
        match started.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(window)) => Some((window, thread)),
            Ok(Err(error)) => {
                tracing::warn!(%error, "voice: Raw Input not registered");
                None
            }
            Err(_) => None,
        }
    }

    pub(super) fn stop(window: isize) {
        if window != 0 {
            // SAFETY: posting to our own message window; a window gone already just fails.
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(window as *mut std::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }

    fn run(shortcuts: Shortcuts, voice: Voice, ready: mpsc::Sender<Result<isize, String>>) {
        let wants_mouse = shortcuts.wants_mouse();
        let window = match make_window() {
            Ok(window) => window,
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                return;
            }
        };
        let before = registered();
        let mut usages = vec![USAGE_KEYBOARD];
        if wants_mouse {
            usages.push(USAGE_MOUSE);
        }
        let ours: Vec<RAWINPUTDEVICE> = usages
            .iter()
            .map(|&usage| RAWINPUTDEVICE {
                usUsagePage: USAGE_PAGE,
                usUsage: usage,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: window,
            })
            .collect();
        // SAFETY: registers this thread's message window for the listed device kinds.
        if let Err(error) =
            unsafe { RegisterRawInputDevices(&ours, size_of::<RAWINPUTDEVICE>() as u32) }
        {
            let _ = ready.send(Err(error.to_string()));
            // SAFETY: our own window, on its thread.
            let _ = unsafe { DestroyWindow(window) };
            return;
        }
        WATCH.with(|watch| *watch.borrow_mut() = Some((shortcuts, voice)));
        // Nobody waits any more (the start took too long): put everything back and go, so no
        // registration outlives its watch.
        let orphan = ready.send(Ok(window.0 as isize)).is_err();
        let mut message = MSG::default();
        if orphan {
            // SAFETY: ends this thread's message loop before it starts.
            unsafe { PostQuitMessage(0) };
        }
        // SAFETY: this thread's message loop.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            unsafe { DispatchMessageW(&message) };
        }
        // Held keys let go (push-to-talk must not stay open).
        let held = WATCH.with(|watch| {
            watch
                .borrow_mut()
                .take()
                .map(|(mut shortcuts, voice)| (shortcuts.release_all(), voice))
        });
        if let Some((changes, voice)) = held {
            for change in changes {
                voice.key(change);
            }
        }
        // Put back what was there: the earlier target of each kind, or none.
        let restore: Vec<RAWINPUTDEVICE> = usages
            .iter()
            .map(|&usage| {
                before
                    .iter()
                    .find(|device| device.usUsagePage == USAGE_PAGE && device.usUsage == usage)
                    .copied()
                    .unwrap_or(RAWINPUTDEVICE {
                        usUsagePage: USAGE_PAGE,
                        usUsage: usage,
                        dwFlags: RIDEV_REMOVE,
                        hwndTarget: HWND::default(),
                    })
            })
            .collect();
        // SAFETY: re-registers what this process had before the watch began.
        if let Err(error) =
            unsafe { RegisterRawInputDevices(&restore, size_of::<RAWINPUTDEVICE>() as u32) }
        {
            tracing::warn!(%error, "voice: the earlier Raw Input registration was not put back");
        }
        // SAFETY: our own window, on its thread.
        let _ = unsafe { DestroyWindow(window) };
    }

    fn make_window() -> windows::core::Result<HWND> {
        // SAFETY: plain Win32 calls; the class name is a static string.
        let instance = unsafe { GetModuleHandleW(None) }?;
        let class = w!("ProjectsVoiceKeys");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&window_class) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(windows::core::Error::from_thread());
        }
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )
        }
    }

    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_INPUT => {
                let mut input = RAWINPUT::default();
                let mut size = size_of::<RAWINPUT>() as u32;
                // SAFETY: lParam of WM_INPUT is the report's handle; the buffer holds a keyboard
                // or mouse report (the only kinds registered).
                let read = unsafe {
                    GetRawInputData(
                        HRAWINPUT(lparam.0 as *mut std::ffi::c_void),
                        RID_INPUT,
                        Some((&raw mut input).cast()),
                        &mut size,
                        size_of::<RAWINPUTHEADER>() as u32,
                    )
                };
                if read != u32::MAX && read > 0 {
                    let changes: Vec<(KeyChange, Voice)> = WATCH.with(|watch| {
                        let mut watch = watch.borrow_mut();
                        let Some((shortcuts, voice)) = watch.as_mut() else {
                            return Vec::new();
                        };
                        let kind = input.header.dwType;
                        let changes = if kind == RIM_TYPEKEYBOARD.0 {
                            // SAFETY: a keyboard report.
                            let key = unsafe { input.data.keyboard };
                            shortcuts.keyboard_typing(key.MakeCode, key.Flags, held, typing())
                        } else if kind == RIM_TYPEMOUSE.0 {
                            // SAFETY: a mouse report.
                            let flags =
                                unsafe { input.data.mouse.Anonymous.Anonymous.usButtonFlags };
                            shortcuts.mouse(flags, held)
                        } else {
                            Vec::new()
                        };
                        changes
                            .into_iter()
                            .map(|change| (change, voice.clone()))
                            .collect()
                    });
                    // Outside the borrow: the machine may stop this very watch.
                    for (change, voice) in changes {
                        voice.key(change);
                    }
                }
            }
            WM_CLOSE => {
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            _ => {}
        }
        // SAFETY: the default handling of a message to our own window.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    fn registered() -> Vec<RAWINPUTDEVICE> {
        let size = size_of::<RAWINPUTDEVICE>() as u32;
        let mut count = 0u32;
        // SAFETY: the first call asks for the count only.
        unsafe { GetRegisteredRawInputDevices(None, &mut count, size) };
        let mut devices = vec![RAWINPUTDEVICE::default(); count as usize];
        // SAFETY: a buffer of `count` entries.
        let got =
            unsafe { GetRegisteredRawInputDevices(Some(devices.as_mut_ptr()), &mut count, size) };
        devices.truncate(if got == u32::MAX { 0 } else { got as usize });
        devices
    }
}
