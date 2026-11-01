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


## Local deb and AppImage packages

From the repository root run `bash scripts/build-linux-packages.sh --deb --appimage`. Outputs go to `apps/desktop/dist`; use `--skip-build` for an existing release binary. Packaging needs bash/ar/tar, not dpkg-deb or mksquashfs. Provide appimagetool through `APPIMAGETOOL` or PATH. The AppImage uses AppRun and a checked-in 256×256 `aimemorandum.png` at the AppDir root for appimagetool (no build-time converter); both formats include desktop metadata, SVG, LICENSE, and THIRD_PARTY_NOTICES.md.

For isolated deb verification, extract `data.tar.gz` with ar/tar into a temporary directory, optionally run `usr/bin/aimemorandum`, then remove that directory. For AppImage, use `--appimage-extract` and optionally the extracted AppRun. No system installation is required. Exact commands and unverified boundaries are in `docs/RUN.md`.

## Session integration

- X11 uses `xcap` and `global-hotkey` (`Ctrl+Alt+M/S/V/L`). A conflicting
  binding is reported without disabling the other shortcuts or CLI forwarding.
- The default Slint renderer is software to stay within the resident-memory
  budget on ordinary desktops. Set `SLINT_BACKEND` explicitly to override it.
- Wayland uses the XDG ScreenCast portal and one PipeWire static frame. A
  Linux-only router waits for native-window readiness, requests fullscreen on
  the captured winit `MonitorHandle`, and waits for the compositor to confirm
  that output and its physical size before enabling selection. Recapturing a
  different output reroutes even when intermediate UI updates are coalesced.
  A unique logical-geometry match (one logical pixel tolerance) selects that
  handle. Winit's integer output scale converts its logical origin; the
  capture-derived physical/logical scale converts its physical dimensions.
  Keeping those domains separate handles fractional scaling at nonzero origins.
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
