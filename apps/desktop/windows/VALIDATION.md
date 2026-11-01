# Windows validation — issue #7

Date: 2026-10-30–2026-11-01 (Asia/Shanghai)
Branch: `ticket-7-windows`  
Runtime: optimized `cargo build --release -p aimemorandum-windows` on Windows, GNU Rust 1.98.1.

This log separates automated/native probes from interactive acceptance. An unexecuted check is **not** a pass. The issue must remain open while any required acceptance check below is pending.

## Evidence collected

| Area | Evidence | Result |
| --- | --- | --- |
| Automated suite | Final `cargo test --workspace` passed 89 tests; `cargo clippy --workspace --all-targets -- -D warnings` also passed. Six opt-in external/hardware probes remained ignored by the ordinary suite. | Passed after the final fixes. |
| Single instance | A real second `--toggle-panel` process exited with code 0 while the first process remained running. Integration tests also prove duplicate pipe ownership and a real `TogglePanel` frame crossing the pipe. | Passed. |
| WinRT OCR | Two ignored probes used the installed user-profile recognizer on `target/validation/ocr-en.png`; output contained `AIMemorandum OCR 123`. `windows_ocr_populates_a_dropped_item_through_the_product_controller` also proved OCR text persisted in `Item.body_md`, the attachment persisted, and original bytes stayed unchanged. The forward-slash absolute-path case passed. | Adapter and product-controller flow passed on this machine. |
| xcap capture | The ignored real-desktop probe captured one 2560×1600 monitor on 2026-10-31. On 2026-11-01 it enumerated two physical monitors, `0,0 2560×1600` and `-1920,271 1920×1080`, then captured and decoded the complete 4480×1600 virtual desktop. No elevation or consent prompt was needed. | Single- and multi-monitor capture passed. |
| Hotkeys | `native_hotkeys_register_and_report_a_real_conflict --ignored` registered the native bindings; a second manager reported `HotKey already registered` for Ctrl+Alt+M. | Native registration and conflict reporting passed. |
| Tray content | `native_tray_accepts_twelve_unicode_entries --ignored` created the real tray icon and updated exactly 12 entries; the long entry contained exactly 36 Unicode scalar values, including Chinese and an emoji. | Native menu construction/update passed. |
| 输入面板（CapturePanel） | A native window-state probe observed the optimized product's visible `快速记录` window. A DPI-aware probe observed the always-on-top screenshot overlay at physical bounds 0,0 2560×1600, exactly matching the real monitor. | Single-monitor window creation and physical geometry passed. |
| Real Chinese IME | With the installed Microsoft Pinyin profile, typing `nihao` displayed composition text `ni'hao`. Composition Enter changed the composition without creating a database item. A fresh composition followed by Space committed `你好`, still with zero items; the following Enter cleared the draft and saved item 56e236b4-c07f-4b8e-b873-fdc15315d8c1 with title/body `你好`. | Composition Enter did not submit; committed-text Enter submitted. |
| Focus, draft, and hotkey action | `draftcheck` was entered in the real editor. Ctrl+Alt+M hid the product window and returned foreground to DeepSeek Harness; the next Ctrl+Alt+M showed `快速记录`, restored focus to ControlType.Edit, and preserved `draftcheck`. The database item count did not change. | Hide/show, focus restoration, draft preservation, and actual global-hotkey action passed. |
| Native PNG/JPEG drops | Files were dragged from a real Explorer window onto the product. Each drop created a source=drop item with the correct image/png or image/jpeg attachment. Persisted original.png and original.jpg were byte-identical to their Explorer sources. | Real OLE drop passed for PNG and JPEG. |
| Real clipboard | `Set-Clipboard` text followed by `--save-clipboard` created item 4b1739f3-9810-4da1-b803-ac8f8db49d79 with title/body `clipboard-check-0731`. A real 640×160 clipboard bitmap followed by the same command created an image clipboard item whose persisted PNG decoded at exactly 640×160. | Text and image clipboard product flows passed. |
| Complete screenshot UI flow | Actual Ctrl+Alt+S opened the overlay; a real pointer drag from (120,350) to (850,720) saved screenshot item d37a2bae-36ba-4db0-93ff-bf8d93efd20d. At 150% scaling the persisted PNG decoded at 1095×555 physical pixels, had nonblank RGB channel variance, and OCR body text from the captured desktop. | Real pointer selection, physical-pixel capture, persistence, and saved-image checks passed on one monitor. |
| Real multi-monitor topology and overlay | Win32 `EnumDisplayMonitors`/`GetDpiForMonitor` measured primary DISPLAY1 at 0,0 2560×1600, 144 DPI (150%), and secondary DISPLAY6 at -1920,271 1920×1080, 96 DPI (100%). A DPI-aware native window probe observed the product overlay at physical -1920,0 4480×1600, exactly covering the virtual desktop. | Mixed DPI, negative origin, and physical overlay bounds passed on real hardware. |
| Mixed-DPI cross-screen selection | Actual Ctrl+Alt+S and a real pointer drag from (-1700,400) on the 96-DPI secondary monitor to (700,1000) on the 144-DPI primary saved item 35767ef0-b072-456e-93cc-b56b9fd00341 and PNG attachment ada24450-34e0-450c-a34c-296b717e772c. The PNG decoded at exactly 2400×600; the capture and persisted attachment were byte-identical. Amber and cyan marker regions landed at output rectangles (150,80)-(800,340) and (1780,80)-(2380,340), exactly matching their physical window positions. OCR contained both `LEFT NEGATIVE 96 DPI` and `PRIMARY 144 DPI`. | Negative-origin input, cross-screen tiling, physical-pixel dimensions, both monitors' content, and persistence passed. |
| Tray click completion identity | A real tray right-click opened #32768; native HMENU/item rectangles were used to click the intended entry at physical (2074,1100). Exactly item 56e236b4-c07f-4b8e-b873-fdc15315d8c1 changed to completed at 2026-10-31T02:32:09Z; all other records stayed unchanged and the popup closed. Separate keyboard selections completed only 0cf3205d-d97c-41ee-8024-e7d8d127bf67 and 4b1739f3-9810-4da1-b803-ac8f8db49d79. | Direct mouse-click and keyboard selection identity passed. |
| Memory: tray only | Final GUI-subsystem optimized process (PID 6640), including immediate hidden-shell/store initialization: WorkingSet64 **21,856,256 bytes** (20.8 MiB), PrivateMemorySize64 **4,325,376 bytes** (4.1 MiB). | Below 60 MiB and 100 MiB limits. |
| Memory: 输入面板（CapturePanel） | Final GUI-subsystem optimized process (PID 6640): WorkingSet64 **24,915,968 bytes** (23.8 MiB), PrivateMemorySize64 **6,078,464 bytes** (5.8 MiB). | Below 80 MiB and 100 MiB limits. |
| Memory: screenshot overlay | Final GUI-subsystem optimized two-monitor topmost overlay using the product's default `winit-software` renderer (PID 6640): WorkingSet64 **89,509,888 bytes** (85.4 MiB), PrivateMemorySize64 **70,651,904 bytes** (67.4 MiB). | Below 100 MiB. |

The default renderer is deliberately `winit-software` unless the operator sets `SLINT_BACKEND`. The femtovg/OpenGL runtime was also sampled and exceeded the budget (WorkingSet64 241,258,496 bytes; PrivateMemorySize64 371,798,016 bytes), so it is not the product default.

Interactive acceptance used an isolated APPDATA directory under `target/validation/interactive/appdata`, not the user's real database. Tray diagnosis temporarily added event-only logging; it was removed before the final workspace test, Clippy, and release-build pass. The multi-monitor acceptance itself required no capture behavior change.

## Final acceptance status

On 2026-11-01 the final hardware checks were executed with two physical monitors at different DPI settings. Real overlay geometry, negative-origin pointer input, a cross-screen selection, both monitors' saved-image content, and database persistence all passed as detailed above. No required acceptance check remains unexecuted.

The multi-monitor session used isolated APPDATA under `target/validation/multi-monitor`, not the user's real database. The final hardware acceptance required no production-code change.

Autostart has automated command coverage, a regression test proving the REG_SZ buffer includes its NUL terminator, and an artifact test proving the background executable uses the Windows GUI subsystem. No persistent machine configuration was changed.

## Reproducible opt-in probes

Run from `apps/desktop`:

```sh
cargo test -p aimemorandum-windows --test ocr -- --ignored --nocapture
cargo test -p aimemorandum-windows --test screenshot xcap_captures_the_real_virtual_desktop_to_png -- --ignored --nocapture
cargo test -p aimemorandum-windows --test hotkeys native_hotkeys_register_and_report_a_real_conflict -- --ignored --nocapture
cargo test -p aimemorandum-windows --test tray native_tray_accepts_twelve_unicode_entries -- --ignored --nocapture
```

The OCR test requires `AIMEMORANDUM_OCR_FIXTURE` to point at a real image. The screenshot probe writes a real desktop capture under `%APPDATA%/AIMemorandum/captures`; treat that file as sensitive and do not commit it. Hardware probes are ignored in the normal suite so unsupported or locked-down environments do not produce false passes.
