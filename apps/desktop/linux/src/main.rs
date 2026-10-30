#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("aimemorandum-linux only supports Linux");
}

#[cfg(target_os = "linux")]
mod linux {
    use aimemorandum_core::LocalStore;
    use aimemorandum_linux::{
        autostart_path, data_dir, detect_session, send_command, socket_path, start_hotkeys,
        CliAction, FrameProvider, IpcError, LinuxClipboard, LinuxDesktopShell, PortalFrameProvider,
        PrimaryInstance, ScreenshotAdapter, SessionKind, SystemClock, TrayService,
        X11FrameProvider,
    };
    use aimemorandum_ui::{CoreEnrollment, ShellEvent, SlintShell, UiController};
    use std::{
        error::Error,
        os::unix::net::UnixStream,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread::JoinHandle,
    };

    pub fn run() -> Result<(), Box<dyn Error>> {
        let action = aimemorandum_linux::parse_args(std::env::args_os().skip(1))?;
        let socket = socket_path()?;
        match PrimaryInstance::acquire(&socket) {
            Ok(primary) => run_primary(primary, action),
            Err(IpcError::AlreadyRunning) => {
                if action != CliAction::Background {
                    send_command(&socket, action)?;
                }
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn run_primary(
        primary: PrimaryInstance,
        initial_action: CliAction,
    ) -> Result<(), Box<dyn Error>> {
        let session = detect_session()?;
        let data = data_dir()?;
        std::fs::create_dir_all(&data)?;
        let provider: Box<dyn FrameProvider> = match session {
            SessionKind::X11 => Box::new(X11FrameProvider),
            SessionKind::Wayland => Box::new(PortalFrameProvider::new(
                data.join("screencast-restore-token"),
            )),
        };
        let screenshot = ScreenshotAdapter::new(provider, data.join("captures"));
        let clipboard = LinuxClipboard::new(data.join("captures"));
        let shell = LinuxDesktopShell::new(autostart_path()?, std::env::current_exe()?);
        let store = LocalStore::open(&data)?;
        let controller = UiController::new(
            store,
            screenshot,
            clipboard,
            CoreEnrollment,
            shell,
            SystemClock,
        )?;
        let ui = SlintShell::new(controller)?;
        let (events, receiver) = mpsc::channel::<ShellEvent>();
        let _hotkeys = match start_hotkeys(session, events.clone()) {
            Ok(service) => Some(service),
            Err(error) => {
                eprintln!("全局快捷键注册失败：{error}；已保留 CLI/socket 回退");
                None
            }
        };
        let tray = match TrayService::start(events.clone()) {
            Ok(service) => Some(service),
            Err(error) => {
                eprintln!("StatusNotifierItem 启动失败：{error}；GNOME 请启用 AppIndicator 扩展");
                None
            }
        };
        if let Some(event) = shell_event(initial_action) {
            let _ = events.send(event);
        }
        let _ipc = spawn_ipc(primary, events.clone());
        let show_capture = initial_action != CliAction::Background;
        ui.run_with_events(receiver, show_capture, move |view| {
            if let Some(tray) = tray.as_ref() {
                tray.publish(view);
            }
        })?;
        Ok(())
    }

    struct IpcService {
        stop: Arc<AtomicBool>,
        path: PathBuf,
        thread: JoinHandle<()>,
    }

    impl Drop for IpcService {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            let _ = UnixStream::connect(&self.path);
            let _ = self.thread.join();
        }
    }

    fn spawn_ipc(primary: PrimaryInstance, events: mpsc::Sender<ShellEvent>) -> IpcService {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let path = primary.path().to_owned();
        let thread = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                let result = primary.serve_next(|action| {
                    if let Some(event) = shell_event(action) {
                        events.send(event).map_err(|error| error.to_string())?;
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    eprintln!("IPC 请求失败：{error}");
                }
            }
        });
        IpcService { stop, path, thread }
    }

    fn shell_event(action: CliAction) -> Option<ShellEvent> {
        match action {
            CliAction::Launch => Some(ShellEvent::ShowCapture),
            CliAction::TogglePanel => Some(ShellEvent::ToggleCapture),
            CliAction::Screenshot => Some(ShellEvent::Screenshot),
            CliAction::SaveClipboard => Some(ShellEvent::SaveClipboard),
            CliAction::ShowItems => Some(ShellEvent::ShowItems),
            CliAction::Background => None,
        }
    }
}
