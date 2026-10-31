# AIMemorandum Linux desktop

Independent Linux executable built on the shared `aimemorandum-core` and
`aimemorandum-ui` crates.

## Build

Install Rust, pkg-config, D-Bus, PipeWire/SPA development headers, X11/XCB,
Wayland, fontconfig and OpenSSL development packages. On Ubuntu:

```sh
sudo apt install build-essential pkg-config libdbus-1-dev \
  libpipewire-0.3-dev libspa-0.2-dev libssl-dev \
  libfontconfig1-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev \
  libgbm-dev libxkbcommon-dev libx11-dev libx11-xcb-dev libxcb1-dev libxcb-randr0-dev \
  libxcb-shm0-dev libxfixes-dev libxrandr-dev
cargo build --release -p aimemorandum-linux
```

The executable is `target/release/aimemorandum`. Install it together with
`packaging/aimemorandum.desktop` and `packaging/aimemorandum.svg` using the
usual `/usr/local/bin`, `/usr/local/share/applications` and
`/usr/local/share/icons/hicolor/scalable/apps` destinations.

## Session integration

- X11 uses `xcap` and `global-hotkey` (`Ctrl+Alt+M/S/V/L`).
- Wayland uses the XDG ScreenCast portal and one PipeWire static frame. A
  Linux-only router waits for native-window readiness, then requests fullscreen
  on the captured winit `MonitorHandle`; selection is disabled while it waits.
  A unique logical-geometry match (one logical pixel tolerance) selects that
  handle. Native output geometry is converted with the capture-derived
  physical/logical scale, not winit's integer Wayland output scale.
  Missing/ambiguous matches, missing portal output metadata, or invalid scaling
  fail closed instead of displaying an overlay on an arbitrary output.
  Cropping continues to use the same captured monitor origin and scale. KDE
  layer-shell is not integrated, and real compositor/output behavior still
  requires the multi-monitor acceptance matrix below.
- The tray is a StatusNotifierItem. GNOME requires an AppIndicator/KStatusNotifier
  extension; KDE and XFCE provide a compatible host directly.
- Autostart and data follow XDG. The local socket accepts only the same UID and
  is removed on clean shutdown. A second process forwards its command to the
  primary instance and exits.

## CLI

```text
aimemorandum [--toggle-panel|--screenshot|--save-clipboard|--items|--background]
```

Without an action the primary window opens. `--background` is used by the XDG
autostart entry. Hotkey registration or tray-host failure does not disable the
CLI/socket route.

## Tests and real acceptance

```sh
cargo test --workspace
cargo build --release -p aimemorandum-linux
```

Portable geometry, command, tray-model and screenshot-staging tests do not prove
desktop integration. Run the issue #8 acceptance matrix on real X11 (preferably
XFCE) and GNOME/KDE Wayland hosts: Chinese IME and marked-text draft preservation,
PNG/JPEG drops, portal authorization, multi-monitor scale/crop pixel comparison,
12 tray entries with 36-character Unicode titles and item identity, all four
shortcuts including conflicts, CLI forwarding/fallback, and steady-state RSS
(tray <=60 MiB, recording window <=80 MiB, every steady state <100 MiB). OCR is
outside this ticket.
