use aimemorandum_core::LocalStore;
use aimemorandum_ui::{
    Clock, CoreEnrollment, ScreenshotOverlay, SlintShell, UiController, UiError, UiIntent,
    UiIntentSender,
};
use aimemorandum_windows::{
    data_dir, menu_action, monitor_geometries, parse_args, project_selection,
    virtual_desktop_bounds, CliAction, IpcClient, IpcServer, NativeHotkeys, NativeTray,
    PhysicalPoint, SelectionDrag, TrayAction, WindowsClipboard, WindowsDesktopShell, WindowsOcr,
    WindowsScreenshot,
};
use chrono::{DateTime, Local, Utc};
use global_hotkey::GlobalHotKeyEvent;
use slint::winit_030::{
    winit::event::{ElementState, Ime, MouseButton, WindowEvent},
    EventResult, WinitWindowAccessor,
};
use slint::{
    ComponentHandle, ModelRc, PhysicalPosition, PhysicalSize, SharedString, Timer, TimerMode,
    VecModel,
};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use tray_icon::menu::MenuEvent;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub fn main() {
    if let Err(error) = entry() {
        eprintln!("AIMemorandum: {error}");
        std::process::exit(1);
    }
}

fn entry() -> Result<(), String> {
    // Full-screen OpenGL overlays exceeded the 100 MiB steady-state budget on Windows.
    // Software rendering stayed below it; retain an explicit operator override.
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
    // DPI awareness must precede tray, Slint, and xcap initialization so all native
    // monitor coordinates have the same physical-pixel coordinate space.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(format!(
            "enable per-monitor DPI awareness: {}",
            io::Error::last_os_error()
        ));
    }
    let action = parse_args(std::env::args().skip(1)).map_err(|error| error.to_string())?;
    match IpcServer::bind() {
        Ok(server) => run(server, action).map_err(|error| error.to_string()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let action = if action == CliAction::Launch {
                CliAction::TogglePanel
            } else {
                action
            };
            IpcClient::send(action)
                .map_err(|error| format!("forward command to running instance: {error}"))
        }
        Err(error) => Err(format!("create single-instance pipe: {error}")),
    }
}

fn run(server: IpcServer, initial_action: CliAction) -> Result<(), UiError> {
    let (action_sender, actions) = channel();
    std::thread::Builder::new()
        .name("aimemorandum-ipc".to_owned())
        .spawn(move || loop {
            match server.accept_once() {
                Ok(action) => {
                    if action_sender.send(action).is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("AIMemorandum IPC: {error}"),
            }
        })
        .map_err(|error| UiError::Platform(format!("start IPC listener: {error}")))?;

    let tray = NativeTray::new()?;
    let hotkeys = NativeHotkeys::new()?;
    let runtime = Rc::new(RefCell::new(Runtime {
        shell: None,
        tray,
        hotkeys,
        actions,
        screenshot_bridge_installed: false,
        quitting: false,
    }));
    // Initialize the hidden shell for tray mode too, so stored tasks are projected
    // immediately after --background autostart rather than after another action.
    runtime.borrow_mut().ensure_shell()?;
    if initial_action != CliAction::Background {
        runtime.borrow_mut().handle_action(initial_action)?;
    }

    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), {
        let runtime = runtime.clone();
        move || {
            if let Err(error) = runtime.borrow_mut().poll() {
                eprintln!("AIMemorandum runtime: {error}");
            }
        }
    });
    // Tray mode intentionally has no visible windows between actions.
    slint::run_event_loop_until_quit().map_err(|error| UiError::Platform(error.to_string()))
}

struct Runtime {
    shell: Option<SlintShell>,
    tray: NativeTray,
    hotkeys: NativeHotkeys,
    actions: Receiver<CliAction>,
    screenshot_bridge_installed: bool,
    quitting: bool,
}

impl Runtime {
    fn poll(&mut self) -> Result<(), UiError> {
        if let Some(view) = self.shell.as_mut().and_then(SlintShell::poll) {
            self.tray
                .set_entries(&view.tray.entries, view.tray.open_count)?;
        }
        let mut events = Vec::new();
        while let Ok(action) = self.actions.try_recv() {
            events.push(TrayAction::Command(action));
        }
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if let Some(action) = self.hotkeys.bindings().action_for(event.id, event.state) {
                events.push(TrayAction::Command(action));
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(action) = menu_action(&event.id.0) {
                events.push(action);
            }
        }
        for event in events {
            match event {
                TrayAction::Command(action) => self.handle_action(action)?,
                TrayAction::CompleteItem(id) => {
                    self.ensure_shell()?
                        .sender()
                        .send(UiIntent::CompleteItem(id))?;
                }
                TrayAction::Quit => {
                    if !self.quitting {
                        self.quitting = true;
                        slint::quit_event_loop()
                            .map_err(|error| UiError::Platform(error.to_string()))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn ensure_shell(&mut self) -> Result<&mut SlintShell, UiError> {
        if self.shell.is_none() {
            let store = LocalStore::open(data_dir().map_err(UiError::Io)?)
                .map_err(|error| UiError::Platform(error.to_string()))?;
            let controller = UiController::new(
                store,
                WindowsScreenshot::new().map_err(UiError::Io)?,
                WindowsClipboard::new().map_err(UiError::Io)?,
                CoreEnrollment,
                WindowsDesktopShell::new()?,
                WindowsClock,
            )?
            .with_text_recognizer(WindowsOcr::user_profile());
            let shell = SlintShell::new(controller)?;
            let capture = shell.capture.as_weak();
            shell
                .capture
                .window()
                .on_winit_window_event(move |_, event| {
                    let Some(capture) = capture.upgrade() else {
                        return EventResult::Propagate;
                    };
                    match event {
                        WindowEvent::DroppedFile(path) => {
                            // Winit emits one event per file; preserve each native drop as a capture.
                            let path = SharedString::from(path.to_string_lossy().as_ref());
                            capture.invoke_paths_dropped(ModelRc::new(VecModel::from(vec![path])));
                        }
                        WindowEvent::Ime(Ime::Preedit(text, _)) => {
                            capture.set_ime_marked(!text.is_empty());
                        }
                        WindowEvent::Ime(Ime::Commit(_) | Ime::Disabled) => {
                            capture.set_ime_marked(false);
                        }
                        _ => {}
                    }
                    // Slint must still render preedit, commit text, and process ordinary keys.
                    EventResult::Propagate
                });
            self.shell = Some(shell);
        }
        Ok(self.shell.as_mut().expect("shell was just initialized"))
    }

    fn handle_action(&mut self, action: CliAction) -> Result<(), UiError> {
        match action {
            CliAction::Background => Ok(()),
            CliAction::Launch | CliAction::TogglePanel => {
                let shell = self.ensure_shell()?;
                if shell.capture.window().is_visible() {
                    shell.capture.hide().map_err(platform)?;
                } else {
                    shell.capture.show().map_err(platform)?;
                    shell.sender().send(UiIntent::Refresh)?;
                }
                Ok(())
            }
            CliAction::ShowItems => {
                let shell = self.ensure_shell()?;
                shell.items.show().map_err(platform)?;
                shell.sender().send(UiIntent::Refresh)
            }
            CliAction::SaveClipboard => self
                .ensure_shell()?
                .sender()
                .send(UiIntent::CaptureClipboard),
            CliAction::Screenshot => {
                let install_bridge = !self.screenshot_bridge_installed;
                let shell = self.ensure_shell()?;
                // Reset any previous native drag before reusing the overlay.
                shell.screenshot.invoke_cancelled();
                let monitors = monitor_geometries()?;
                let bounds = virtual_desktop_bounds(&monitors)?;
                let _ = shell.capture.hide();
                let _ = shell.items.hide();
                let _ = shell.settings.hide();
                let _ = shell.reminder.hide();
                shell.screenshot.show().map_err(platform)?;
                shell
                    .screenshot
                    .window()
                    .set_position(PhysicalPosition::new(bounds.x, bounds.y));
                shell
                    .screenshot
                    .window()
                    .set_size(PhysicalSize::new(bounds.width, bounds.height));
                if install_bridge {
                    if !shell.screenshot.window().has_winit_window() {
                        return Err(UiError::Platform(
                            "screenshot overlay requires the winit backend".to_owned(),
                        ));
                    }
                    install_screenshot_bridge(shell);
                    self.screenshot_bridge_installed = true;
                }
                Ok(())
            }
        }
    }
}

fn install_screenshot_bridge(shell: &SlintShell) {
    let drag = Rc::new(RefCell::new(SelectionDrag::default()));
    let screenshot = shell.screenshot.as_weak();
    let sender = shell.sender().clone();
    shell.screenshot.window().on_winit_window_event({
        let drag = drag.clone();
        let sender = sender.clone();
        move |window, event| {
            let Some(screenshot) = screenshot.upgrade() else {
                return EventResult::Propagate;
            };
            match event {
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state: ElementState::Pressed,
                    ..
                } => {
                    let Some(point) = cursor_position() else {
                        cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
                        return EventResult::PreventDefault;
                    };
                    drag.borrow_mut().begin(point);
                    update_screenshot_projection(
                        &screenshot,
                        window,
                        &drag.borrow(),
                        window.scale_factor() as f64,
                    );
                    let _ = sender.send(UiIntent::ScreenshotStarted {
                        x: point.x as f64,
                        y: point.y as f64,
                    });
                    EventResult::PreventDefault
                }
                WindowEvent::CursorMoved { .. } if drag.borrow().selection().is_some() => {
                    let Some(point) = cursor_position() else {
                        cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
                        return EventResult::PreventDefault;
                    };
                    drag.borrow_mut().update(point);
                    update_screenshot_projection(
                        &screenshot,
                        window,
                        &drag.borrow(),
                        window.scale_factor() as f64,
                    );
                    let _ = sender.send(UiIntent::ScreenshotMoved {
                        x: point.x as f64,
                        y: point.y as f64,
                    });
                    EventResult::PreventDefault
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state: ElementState::Released,
                    ..
                } if drag.borrow().selection().is_some() => {
                    let Some(point) = cursor_position() else {
                        cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
                        return EventResult::PreventDefault;
                    };
                    if drag.borrow_mut().finish(point).is_some() {
                        clear_screenshot_selection(&screenshot);
                        let _ = screenshot.hide();
                        // The overlay must be hidden before the worker captures the desktop.
                        let _ = sender.send(UiIntent::ScreenshotFinished {
                            x: point.x as f64,
                            y: point.y as f64,
                        });
                    }
                    EventResult::PreventDefault
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    update_screenshot_projection(
                        &screenshot,
                        window,
                        &drag.borrow(),
                        *scale_factor,
                    );
                    EventResult::Propagate
                }
                WindowEvent::Focused(false) if drag.borrow().selection().is_some() => {
                    cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
                    EventResult::Propagate
                }
                WindowEvent::CloseRequested => {
                    cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
                    EventResult::PreventDefault
                }
                _ => EventResult::Propagate,
            }
        }
    });
    let cancelled = shell.screenshot.as_weak();
    shell.screenshot.on_cancelled(move || {
        if let Some(screenshot) = cancelled.upgrade() {
            cancel_screenshot(&screenshot, &mut drag.borrow_mut(), &sender);
        }
    });
}

fn cursor_position() -> Option<PhysicalPoint> {
    let mut point = windows_sys::Win32::Foundation::POINT::default();
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(PhysicalPoint {
        x: i64::from(point.x),
        y: i64::from(point.y),
    })
}

fn update_screenshot_projection(
    screenshot: &ScreenshotOverlay,
    window: &slint::Window,
    drag: &SelectionDrag,
    scale: f64,
) {
    let position = window.position();
    let origin = PhysicalPoint {
        x: i64::from(position.x),
        y: i64::from(position.y),
    };
    if let Some(projected) = drag
        .selection()
        .and_then(|selection| project_selection(selection, origin, scale))
    {
        screenshot.set_selection_x(projected.x as f32);
        screenshot.set_selection_y(projected.y as f32);
        screenshot.set_selection_width(projected.width as f32);
        screenshot.set_selection_height(projected.height as f32);
        screenshot.set_selecting(true);
    }
}

fn clear_screenshot_selection(screenshot: &ScreenshotOverlay) {
    screenshot.set_selecting(false);
    screenshot.set_selection_width(0.0);
    screenshot.set_selection_height(0.0);
}

fn cancel_screenshot(
    screenshot: &ScreenshotOverlay,
    drag: &mut SelectionDrag,
    sender: &UiIntentSender,
) {
    drag.cancel();
    clear_screenshot_selection(screenshot);
    let _ = screenshot.hide();
    let _ = sender.send(UiIntent::ScreenshotCancelled);
}

struct WindowsClock;

impl Clock for WindowsClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn timezone(&self) -> String {
        Local::now().offset().to_string()
    }
}

fn platform(error: slint::PlatformError) -> UiError {
    UiError::Platform(error.to_string())
}
