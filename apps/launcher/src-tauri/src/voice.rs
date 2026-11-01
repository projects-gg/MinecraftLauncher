//! The voice window (VOICE, design §6.1 in the task note `voice.md`).
//!
//! Voice chat's engine runs in a window of its own that is never shown: the main page may rest
//! and be suspended during a call (webview_rest) while the call's audio work stays in a small page
//! without React (`apps/launcher/voice.html`). For now only VOICE-0's probe exists, in debug
//! builds: `PROJECTS_VOICE_PROBE=<config file>` (pl_voice::probe::ProbeConfig) opens the window,
//! gives its page the microphone, and the page runs a call against itself (two peer connections:
//! one Opus encode, one decode and playout, like a call with one other person) while this side
//! records what VOICE-0 measures to the config's `out` file: the page's audio and network
//! counters, the WebView2 processes' CPU time and memory (which renderer hosts which page), how
//! long an event takes to reach the hidden page, and Raw Input as push-to-talk would use it
//! (counts only). Release builds never read the variable.

/// Opens the probe's voice window when a debug build is asked to (call once, in setup).
pub fn install(app: &tauri::App) {
    #[cfg(debug_assertions)]
    probe::install(app);
    #[cfg(not(debug_assertions))]
    let _ = app;
}

#[cfg(debug_assertions)]
mod probe {
    use std::borrow::Cow;
    use std::io::Write;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;
    use pl_voice::probe::ProbeConfig;
    use pl_voice::{VOICE_PAGE, VOICE_WINDOW};
    use serde_json::{Value, json};
    use tauri::http::{HeaderValue, Response, header::CONTENT_SECURITY_POLICY};
    use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

    pub(super) struct Probe {
        config: ProbeConfig,
        out: Mutex<std::fs::File>,
        started: Instant,
        gate: Mutex<Gate>,
    }

    /// The page's call starts once both sides are ready: its listener (it said so) and the
    /// microphone's answer (the permission handler is in place).
    #[derive(Default)]
    struct Gate {
        page: bool,
        permission: bool,
        started: bool,
    }

    impl Probe {
        fn ms(&self) -> u64 {
            self.started.elapsed().as_millis() as u64
        }

        pub(super) fn record(&self, mut value: Value) {
            if let Some(object) = value.as_object_mut() {
                object.insert("at".to_owned(), json!(self.ms()));
            }
            let mut out = self.out.lock();
            if let Err(error) = writeln!(out, "{value}") {
                tracing::warn!(%error, "voice probe: a record was lost");
            }
        }

        pub(super) fn ready(&self, app: &AppHandle, page: bool) {
            let go = {
                let mut gate = self.gate.lock();
                if page {
                    gate.page = true;
                } else {
                    gate.permission = true;
                }
                let go = gate.page && gate.permission && !gate.started;
                gate.started |= go;
                go
            };
            if !go {
                return;
            }
            tracing::info!("voice probe: the call starts");
            let payload = json!({
                "page": self.config.page,
                "seconds": self.config.seconds,
                "every": self.config.every,
            });
            if let Err(error) = app.emit_to(VOICE_WINDOW, "voice-probe-go", payload) {
                tracing::warn!(%error, "voice probe: the page was not started");
            }
        }
    }

    pub(super) fn install(app: &tauri::App) {
        let Some(path) = std::env::var_os("PROJECTS_VOICE_PROBE").filter(|path| !path.is_empty())
        else {
            return;
        };
        if let Err(error) = start(app, Path::new(&path)) {
            tracing::warn!(%error, "voice probe not started");
        }
    }

    fn start(app: &tauri::App, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let config = ProbeConfig::parse(&text)?;
        let out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.out)
            .map_err(|error| format!("{}: {error}", config.out.display()))?;
        let probe = Arc::new(Probe {
            config,
            out: Mutex::new(out),
            started: Instant::now(),
            gate: Mutex::default(),
        });
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis() as u64)
            .unwrap_or_default();
        probe.record(json!({
            "k": "start",
            "wall": wall,
            "pid": std::process::id(),
            "config": probe.config,
        }));

        let handle = app.handle().clone();
        let p = probe.clone();
        app.listen_any("voice-probe-ready", move |_| p.ready(&handle, true));
        let p = probe.clone();
        app.listen_any("voice-probe-report", move |event| {
            let report = serde_json::from_str::<Value>(event.payload()).unwrap_or(Value::Null);
            p.record(json!({ "k": "page", "r": report }));
        });
        let p = probe.clone();
        app.listen_any("voice-probe-pong", move |event| {
            if let Ok(sent) = serde_json::from_str::<u64>(event.payload()) {
                p.record(json!({ "k": "pong", "rttMs": p.ms().saturating_sub(sent) }));
            }
        });
        let p = probe.clone();
        app.listen_any("voice-probe-done", move |_| {
            p.record(json!({ "k": "done" }))
        });

        let window =
            WebviewWindowBuilder::new(app, VOICE_WINDOW, WebviewUrl::App(VOICE_PAGE.into()))
                .title("Projects Launcher")
                .visible(false)
                .skip_taskbar(true)
                .focused(false)
                .focusable(false)
                .decorations(false)
                .inner_size(320.0, 240.0)
                // Only the launcher's own pages, and WebAssembly for the noise suppressor in this
                // window's policy alone (pl_voice::csp).
                .on_navigation(|url| pl_voice::permission::is_app_origin(url.as_str()))
                .on_web_resource_request(|_request, response| allow_wasm(response))
                .build()
                .map_err(|error| error.to_string())?;
        #[cfg(windows)]
        platform::prepare(&window, probe.clone())?;
        #[cfg(not(windows))]
        {
            let _ = window;
            probe.ready(app.handle(), false);
        }
        run(app.handle().clone(), probe);
        Ok(())
    }

    fn allow_wasm(response: &mut Response<Cow<'static, [u8]>>) {
        let Some(policy) = response.headers_mut().get_mut(CONTENT_SECURITY_POLICY) else {
            return;
        };
        let Ok(text) = policy.to_str() else {
            return;
        };
        if let Ok(value) = HeaderValue::from_str(&pl_voice::csp::allow_wasm(text)) {
            *policy = value;
        }
    }

    /// Every `every` seconds: a ping to the page and a sample of the processes, until a little
    /// after the page's call ended; then the window closes.
    fn run(app: AppHandle, probe: Arc<Probe>) {
        tauri::async_runtime::spawn(async move {
            let every = Duration::from_secs(probe.config.every);
            let until = Duration::from_secs(probe.config.seconds) + every + Duration::from_secs(5);
            let begin = tokio::time::Instant::now();
            let mut next = every;
            while next <= until {
                tokio::time::sleep_until(begin + next).await;
                next += every;
                if let Err(error) = app.emit_to(VOICE_WINDOW, "voice-probe-ping", probe.ms()) {
                    tracing::debug!(%error, "voice probe: ping not sent");
                }
                #[cfg(windows)]
                {
                    let p = probe.clone();
                    let _ = app.run_on_main_thread(move || platform::sample(&p));
                }
            }
            #[cfg(windows)]
            platform::stop_input();
            probe.record(json!({ "k": "end" }));
            if let Some(window) = app.get_webview_window(VOICE_WINDOW) {
                let _ = window.close();
            }
        });
    }

    #[cfg(windows)]
    mod platform {
        use std::cell::RefCell;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

        use pl_voice::VOICE_WINDOW;
        use pl_voice::input::KeyWatch;
        use pl_voice::permission::{Answer, Ask};
        use pl_voice::probe::InputConfig;
        use serde_json::{Value, json};
        use tauri::{Manager, WebviewWindow};
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
            COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_KIND,
            COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU,
            COREWEBVIEW2_PROCESS_KIND_RENDERER, COREWEBVIEW2_PROCESS_KIND_UTILITY, ICoreWebView2,
            ICoreWebView2Environment, ICoreWebView2Environment13,
            ICoreWebView2PermissionRequestedEventArgs3, ICoreWebView2ProcessExtendedInfo,
        };
        use webview2_com::{
            GetProcessExtendedInfosCompletedHandler, PermissionRequestedEventHandler,
            ProcessFailedEventHandler, take_pwstr,
        };
        use windows::Win32::Foundation::{
            CloseHandle, FILETIME, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
        };
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentThreadId, GetProcessTimes, GetThreadTimes, OpenProcess,
            OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, THREAD_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::Input::{
            GetRawInputData, GetRegisteredRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
            RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEKEYBOARD,
            RIM_TYPEMOUSE, RegisterRawInputDevices,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            HWND_MESSAGE, MSG, PostMessageW, PostQuitMessage, RI_KEY_BREAK, RegisterClassW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_INPUT, WNDCLASSW,
        };
        use windows::core::{BOOL, Interface, PWSTR, w};

        use super::Probe;

        const MB: f64 = 1024.0 * 1024.0;
        /// HID usages (generic desktop page): mouse and keyboard.
        const USAGE_PAGE: u16 = 1;
        const USAGE_MOUSE: u16 = 2;
        const USAGE_KEYBOARD: u16 = 6;

        thread_local! {
            /// The WebView2 environment, on the main thread: it lists its processes.
            static ENVIRONMENT: RefCell<Option<ICoreWebView2Environment>> =
                const { RefCell::new(None) };
            /// The watched key, on the input thread.
            static WATCH: RefCell<Option<KeyWatch>> = const { RefCell::new(None) };
        }

        /// What the input thread saw, for the sampler.
        static INPUT_EVENTS: AtomicU64 = AtomicU64::new(0);
        static INPUT_PRESSES: AtomicU64 = AtomicU64::new(0);
        static INPUT_THREAD: AtomicIsize = AtomicIsize::new(0);
        static INPUT_WINDOW: AtomicIsize = AtomicIsize::new(0);

        /// The page counts as visible (so Chromium slows nothing down) although its window never
        /// shows; the microphone is answered here; then the page may start.
        pub(super) fn prepare(window: &WebviewWindow, probe: Arc<Probe>) -> Result<(), String> {
            let app = window.app_handle().clone();
            window
                .with_webview(move |webview| {
                    let controller = webview.controller();
                    ENVIRONMENT.with(|environment| {
                        *environment.borrow_mut() = Some(webview.environment());
                    });
                    if let Err(error) = unsafe { controller.SetIsVisible(true) } {
                        tracing::warn!(%error, "voice window: page not visible");
                    }
                    let core = match unsafe { controller.CoreWebView2() } {
                        Ok(core) => core,
                        Err(error) => {
                            tracing::warn!(%error, "voice window: no WebView2");
                            return;
                        }
                    };
                    if let Err(error) = answer_permissions(&core, probe.clone()) {
                        tracing::warn!(%error, "voice window: microphone unanswered");
                        return;
                    }
                    watch_failures(&core, probe.clone());
                    if let Some(input) = probe.config.input {
                        start_input(probe.clone(), input);
                    }
                    probe.ready(&app, false);
                })
                .map_err(|error| error.to_string())
        }

        fn answer_permissions(
            core: &ICoreWebView2,
            probe: Arc<Probe>,
        ) -> windows::core::Result<()> {
            let handler = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                unsafe { args.PermissionKind(&mut kind)? };
                let mut uri = PWSTR::null();
                unsafe { args.Uri(&mut uri)? };
                let uri = take_pwstr(uri);
                let ask = if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                    Ask::Microphone
                } else {
                    Ask::Other
                };
                let answer = pl_voice::permission::decide(VOICE_WINDOW, &uri, ask);
                let state = match answer {
                    Answer::Allow => COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                    Answer::Deny => COREWEBVIEW2_PERMISSION_STATE_DENY,
                };
                unsafe { args.SetState(state)? };
                // Never kept in the profile all the launcher's windows share.
                if let Ok(args) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                    unsafe { args.SetSavesInProfile(false)? };
                }
                probe.record(json!({
                    "k": "permission",
                    "kind": kind.0,
                    // WebView2 names the asking origin, not the page.
                    "origin": pl_voice::permission::origin(&uri),
                    "allowed": answer == Answer::Allow,
                }));
                Ok(())
            }));
            let mut token = 0;
            unsafe { core.add_PermissionRequested(&handler, &mut token) }
        }

        fn watch_failures(core: &ICoreWebView2, probe: Arc<Probe>) {
            let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                if let Some(args) = args {
                    unsafe { args.ProcessFailedKind(&mut kind)? };
                }
                probe.record(
                    json!({ "k": "error", "text": format!("process failed ({})", kind.0) }),
                );
                Ok(())
            }));
            let mut token = 0;
            if let Err(error) = unsafe { core.add_ProcessFailed(&handler, &mut token) } {
                tracing::debug!(%error, "voice window: process failures not watched");
            }
        }

        /// Records the environment's processes: kind, the pages each hosts, CPU time, memory;
        /// with the launcher's own process and the input thread's counts. Main thread.
        pub(super) fn sample(probe: &Arc<Probe>) {
            let Some(environment) = ENVIRONMENT.with(|environment| environment.borrow().clone())
            else {
                return;
            };
            let Ok(environment) = environment.cast::<ICoreWebView2Environment13>() else {
                probe.record(json!({ "k": "error", "text": "no process list (WebView2 too old)" }));
                return;
            };
            let p = probe.clone();
            let handler =
                GetProcessExtendedInfosCompletedHandler::create(Box::new(move |result, infos| {
                    result?;
                    let mut list = Vec::new();
                    if let Some(infos) = infos {
                        let mut count = 0;
                        unsafe { infos.Count(&mut count)? };
                        for index in 0..count {
                            if let Ok(info) = unsafe { infos.GetValueAtIndex(index) } {
                                list.push(process(&info));
                            }
                        }
                    }
                    let own = numbers(unsafe { GetCurrentProcess() });
                    p.record(json!({
                        "k": "procs",
                        "list": list,
                        "self": {
                            "pid": std::process::id(),
                            "cpuMs": own.map(|own| own.0),
                            "wsMb": own.map(|own| own.1),
                            "privMb": own.map(|own| own.2),
                        },
                    }));
                    Ok(())
                }));
            if let Err(error) = unsafe { environment.GetProcessExtendedInfos(&handler) } {
                probe.record(json!({ "k": "error", "text": format!("process list: {error}") }));
            }
            if probe.config.input.is_some() {
                probe.record(input_numbers());
            }
        }

        fn process(info: &ICoreWebView2ProcessExtendedInfo) -> Value {
            let mut pid = 0;
            let mut kind = COREWEBVIEW2_PROCESS_KIND::default();
            if let Ok(process) = unsafe { info.ProcessInfo() } {
                let _ = unsafe { process.ProcessId(&mut pid) };
                let _ = unsafe { process.Kind(&mut kind) };
            }
            let mut pages = Vec::new();
            if let Ok(frames) = unsafe { info.AssociatedFrameInfos() }
                && let Ok(frames) = unsafe { frames.GetIterator() }
            {
                let mut more = BOOL::default();
                let _ = unsafe { frames.HasCurrent(&mut more) };
                while more.as_bool() {
                    if let Ok(frame) = unsafe { frames.GetCurrent() } {
                        let mut source = PWSTR::null();
                        if unsafe { frame.Source(&mut source) }.is_ok() {
                            pages.push(pl_voice::probe::page_of(&take_pwstr(source)));
                        }
                    }
                    if unsafe { frames.MoveNext(&mut more) }.is_err() {
                        break;
                    }
                }
            }
            let kind = match kind {
                COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
                COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
                COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
                COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
                _ => "other",
            };
            let numbers = u32::try_from(pid).ok().and_then(|pid| {
                let handle =
                    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
                let out = numbers(handle);
                let _ = unsafe { CloseHandle(handle) };
                out
            });
            json!({
                "pid": pid,
                "kind": kind,
                "pages": pages,
                "cpuMs": numbers.map(|n| n.0),
                "wsMb": numbers.map(|n| n.1),
                "privMb": numbers.map(|n| n.2),
            })
        }

        /// CPU time (ms), working set and private memory (MB) of a process.
        fn numbers(handle: HANDLE) -> Option<(f64, f64, f64)> {
            let mut times = [FILETIME::default(); 4];
            let [created, exited, kernel, user] = &mut times;
            unsafe { GetProcessTimes(handle, created, exited, kernel, user) }.ok()?;
            let cpu = (ticks(*kernel) + ticks(*user)) as f64 / 10_000.0;
            let mut memory = PROCESS_MEMORY_COUNTERS_EX {
                cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ..Default::default()
            };
            unsafe {
                GetProcessMemoryInfo(
                    handle,
                    (&raw mut memory).cast::<PROCESS_MEMORY_COUNTERS>(),
                    memory.cb,
                )
            }
            .ok()?;
            Some((
                cpu,
                memory.WorkingSetSize as f64 / MB,
                memory.PrivateUsage as f64 / MB,
            ))
        }

        fn ticks(time: FILETIME) -> u64 {
            (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
        }

        /// Raw Input as push-to-talk would use it: a hidden message window on a thread of its own
        /// gets every keyboard (and mouse) report, also while another program is in front; only
        /// the watched key's presses are kept. A process has one Raw Input target per device
        /// kind, so what was registered before (tao's, for its device events) is put back after.
        fn start_input(probe: Arc<Probe>, config: InputConfig) {
            let spawned = std::thread::Builder::new()
                .name("voice-input".to_owned())
                .spawn(move || {
                    if let Err(error) = run_input(&probe, config) {
                        probe
                            .record(json!({ "k": "error", "text": format!("raw input: {error}") }));
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "voice probe: no input thread");
            }
        }

        fn run_input(probe: &Probe, config: InputConfig) -> windows::core::Result<()> {
            let thread = unsafe {
                OpenThread(
                    THREAD_QUERY_LIMITED_INFORMATION,
                    false,
                    GetCurrentThreadId(),
                )
            }?;
            INPUT_THREAD.store(thread.0 as isize, Ordering::Relaxed);
            WATCH.with(|watch| *watch.borrow_mut() = Some(KeyWatch::new(config.key)));
            let instance = unsafe { GetModuleHandleW(None) }?;
            let class = w!("ProjectsVoiceInput");
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(input_procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            if unsafe { RegisterClassW(&window_class) } == 0 {
                return Err(windows::core::Error::from_thread());
            }
            let window = unsafe {
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
            }?;
            INPUT_WINDOW.store(window.0 as isize, Ordering::Relaxed);
            let before = registered();
            let mut usages = vec![USAGE_KEYBOARD];
            if config.mouse {
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
            unsafe { RegisterRawInputDevices(&ours, size_of::<RAWINPUTDEVICE>() as u32) }?;
            probe.record(json!({
                "k": "input",
                "before": describe(&before),
                "after": describe(&registered()),
                "events": 0,
                "presses": 0,
                "cpuMs": 0.0,
            }));
            let mut message = MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
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
            let restored =
                unsafe { RegisterRawInputDevices(&restore, size_of::<RAWINPUTDEVICE>() as u32) };
            probe.record(json!({
                "k": "note",
                "text": format!("raw input put back: {}", restored.is_ok()),
                "now": describe(&registered()),
            }));
            let _ = unsafe { DestroyWindow(window) };
            INPUT_THREAD.store(0, Ordering::Relaxed);
            let _ = unsafe { CloseHandle(thread) };
            Ok(())
        }

        unsafe extern "system" fn input_procedure(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            match message {
                WM_INPUT => {
                    INPUT_EVENTS.fetch_add(1, Ordering::Relaxed);
                    let mut input = RAWINPUT::default();
                    let mut size = size_of::<RAWINPUT>() as u32;
                    // SAFETY: lParam of WM_INPUT is the report's handle; the buffer holds a
                    // keyboard or mouse report (the only kinds registered).
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
                        WATCH.with(|watch| {
                            let mut watch = watch.borrow_mut();
                            let Some(watch) = watch.as_mut() else {
                                return;
                            };
                            let kind = input.header.dwType;
                            if kind == RIM_TYPEKEYBOARD.0 {
                                // SAFETY: a keyboard report.
                                let key = unsafe { input.data.keyboard };
                                watch.keyboard(key.VKey, u32::from(key.Flags) & RI_KEY_BREAK != 0);
                            } else if kind == RIM_TYPEMOUSE.0 {
                                // SAFETY: a mouse report.
                                let flags =
                                    unsafe { input.data.mouse.Anonymous.Anonymous.usButtonFlags };
                                watch.mouse(flags);
                            }
                            INPUT_PRESSES.store(watch.presses(), Ordering::Relaxed);
                        });
                    }
                }
                WM_CLOSE => {
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
            unsafe { GetRegisteredRawInputDevices(None, &mut count, size) };
            let mut devices = vec![RAWINPUTDEVICE::default(); count as usize];
            let got = unsafe {
                GetRegisteredRawInputDevices(Some(devices.as_mut_ptr()), &mut count, size)
            };
            devices.truncate(if got == u32::MAX { 0 } else { got as usize });
            devices
        }

        fn describe(devices: &[RAWINPUTDEVICE]) -> Value {
            devices
                .iter()
                .map(|device| {
                    json!({
                        "usage": device.usUsage,
                        "flags": device.dwFlags.0,
                        "window": device.hwndTarget.0 as isize,
                    })
                })
                .collect()
        }

        fn input_numbers() -> Value {
            let thread = INPUT_THREAD.load(Ordering::Relaxed);
            let cpu = (thread != 0).then(|| {
                let mut times = [FILETIME::default(); 4];
                let [created, exited, kernel, user] = &mut times;
                let handle = HANDLE(thread as *mut std::ffi::c_void);
                unsafe { GetThreadTimes(handle, created, exited, kernel, user) }
                    .ok()
                    .map(|()| (ticks(*kernel) + ticks(*user)) as f64 / 10_000.0)
            });
            json!({
                "k": "input",
                "events": INPUT_EVENTS.load(Ordering::Relaxed),
                "presses": INPUT_PRESSES.load(Ordering::Relaxed),
                "cpuMs": cpu.flatten(),
            })
        }

        /// Ends the input thread (it puts back the earlier registrations).
        pub(super) fn stop_input() {
            let window = INPUT_WINDOW.swap(0, Ordering::Relaxed);
            if window != 0 {
                let window = HWND(window as *mut std::ffi::c_void);
                let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
        }
    }
}
