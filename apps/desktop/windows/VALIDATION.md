# Windows validation — issue #7

Date: 2026-10-30–31 (Asia/Shanghai)
Branch: `ticket-7-windows`  
Runtime: optimized `cargo build --release -p aimemorandum-windows` on Windows, GNU Rust 1.98.1.

This log separates automated/native probes from interactive acceptance. An unexecuted check is **not** a pass. The issue must remain open while any required acceptance check below is pending.

## Evidence collected

| Area | Evidence | Result |
| --- | --- | --- |
| Automated suite | Final `cargo test --workspace` passed 87 tests; `cargo clippy --workspace --all-targets -- -D warnings` also passed. Six opt-in external/hardware probes remained ignored by the ordinary suite. | Passed after the final fixes. |
| Single instance | A real second `--toggle-panel` process exited with code 0 while the first process remained running. Integration tests also prove duplicate pipe ownership and a real `TogglePanel` frame crossing the pipe. | Passed. |
| WinRT OCR | Two ignored probes used the installed user-profile recognizer on `target/validation/ocr-en.png`; output contained `AIMemorandum OCR 123`. `windows_ocr_populates_a_dropped_item_through_the_product_controller` also proved OCR text persisted in `Item.body_md`, the attachment persisted, and original bytes stayed unchanged. The forward-slash absolute-path case passed. | Adapter and product-controller flow passed on this machine. |
| xcap capture | `xcap_captures_the_real_virtual_desktop_to_png --ignored` captured one real 2560×1600 monitor and decoded the resulting PNG at the same dimensions. No elevation or consent prompt was needed. | Single-monitor capture passed. |
| Hotkeys | `native_hotkeys_register_and_report_a_real_conflict --ignored` registered the native bindings; a second manager reported `HotKey already registered` for Ctrl+Alt+M. | Native registration and conflict reporting passed. |
| Tray content | `native_tray_accepts_twelve_unicode_entries --ignored` created the real tray icon and updated exactly 12 entries; the long entry contained exactly 36 Unicode scalar values, including Chinese and an emoji. | Native menu construction/update passed. |
| 输入面板（CapturePanel） | A native window-state probe observed the optimized product's visible `快速记录` window. A DPI-aware probe observed the always-on-top screenshot overlay at physical bounds 0,0 2560×1600, exactly matching the real monitor. | Single-monitor window creation and physical geometry passed. |
| Real Chinese IME | With the installed Microsoft Pinyin profile, typing `nihao` displayed composition text `ni'hao`. Composition Enter changed the composition without creating a database item. A fresh composition followed by Space committed `你好`, still with zero items; the following Enter cleared the draft and saved item 56e236b4-c07f-4b8e-b873-fdc15315d8c1 with title/body `你好`. | Composition Enter did not submit; committed-text Enter submitted. |
| Focus, draft, and hotkey action | `draftcheck` was entered in the real editor. Ctrl+Alt+M hid the product window and returned foreground to DeepSeek Harness; the next Ctrl+Alt+M showed `快速记录`, restored focus to ControlType.Edit, and preserved `draftcheck`. The database item count did not change. | Hide/show, focus restoration, draft preservation, and actual global-hotkey action passed. |
| Native PNG/JPEG drops | Files were dragged from a real Explorer window onto the product. Each drop created a source=drop item with the correct image/png or image/jpeg attachment. Persisted original.png and original.jpg were byte-identical to their Explorer sources. | Real OLE drop passed for PNG and JPEG. |
| Real clipboard | `Set-Clipboard` text followed by `--save-clipboard` created item 4b1739f3-9810-4da1-b803-ac8f8db49d79 with title/body `clipboard-check-0731`. A real 640×160 clipboard bitmap followed by the same command created an image clipboard item whose persisted PNG decoded at exactly 640×160. | Text and image clipboard product flows passed. |
| Complete screenshot UI flow | Actual Ctrl+Alt+S opened the overlay; a real pointer drag from (120,350) to (850,720) saved screenshot item d37a2bae-36ba-4db0-93ff-bf8d93efd20d. At 150% scaling the persisted PNG decoded at 1095×555 physical pixels, had nonblank RGB channel variance, and OCR body text from the captured desktop. | Real pointer selection, physical-pixel capture, persistence, and saved-image checks passed on one monitor. |
| Tray click completion identity | A real tray right-click opened #32768; native HMENU/item rectangles were used to click the intended entry at physical (2074,1100). Exactly item 56e236b4-c07f-4b8e-b873-fdc15315d8c1 changed to completed at 2026-10-31T02:32:09Z; all other records stayed unchanged and the popup closed. Separate keyboard selections completed only 0cf3205d-d97c-41ee-8024-e7d8d127bf67 and 4b1739f3-9810-4da1-b803-ac8f8db49d79. | Direct mouse-click and keyboard selection identity passed. |
| Memory: tray only | Final optimized process (PID 11148), including immediate hidden-shell/store initialization: WorkingSet64 **21,676,032 bytes** (20.7 MiB), PrivateMemorySize64 **4,177,920 bytes** (4.0 MiB). | Below 60 MiB and 100 MiB limits. |
| Memory: 输入面板（CapturePanel） | Final optimized process (PID 11148): WorkingSet64 **24,760,320 bytes** (23.6 MiB), PrivateMemorySize64 **5,967,872 bytes** (5.7 MiB). | Below 80 MiB and 100 MiB limits. |
| Memory: screenshot overlay | Final optimized topmost overlay using the product's default `winit-software` renderer (PID 11148): WorkingSet64 **41,205,760 bytes** (39.3 MiB), PrivateMemorySize64 **22,384,640 bytes** (21.4 MiB). | Below 100 MiB. |

The default renderer is deliberately `winit-software` unless the operator sets `SLINT_BACKEND`. The femtovg/OpenGL runtime was also sampled and exceeded the budget (WorkingSet64 241,258,496 bytes; PrivateMemorySize64 371,798,016 bytes), so it is not the product default.

Interactive acceptance used an isolated APPDATA directory under `target/validation/interactive/appdata`, not the user's real database. Tray diagnosis temporarily added event-only logging; it was removed before the final workspace test, Clippy, and release-build pass. No production behavior fix was needed.

## Remaining hardware blocker

The Windows desktop was unlocked on 2026-10-31 and every single-monitor interactive check above was executed. Genuine multi-monitor acceptance remains blocked by the available hardware: xcap enumerates exactly one physical monitor, `MonitorGeometry { x: 0, y: 0, width: 2560, height: 1600 }`.

- Multi-monitor, mixed-DPI scaling, negative-origin pointer coordinates, and cross-screen selection remain **unexecuted**. Geometry unit tests and one-monitor capture do not prove this hardware case.

This is the only required acceptance item still unexecuted, so issue #7 remains open. No virtualized or single-monitor result is claimed as multi-monitor evidence.

Autostart currently has automated command/registry-adapter coverage, not a persistent machine configuration change.

## Reproducible opt-in probes

Run from `apps/desktop`:

```sh
cargo test -p aimemorandum-windows --test ocr -- --ignored --nocapture
cargo test -p aimemorandum-windows --test screenshot xcap_captures_the_real_virtual_desktop_to_png -- --ignored --nocapture
cargo test -p aimemorandum-windows --test hotkeys native_hotkeys_register_and_report_a_real_conflict -- --ignored --nocapture
cargo test -p aimemorandum-windows --test tray native_tray_accepts_twelve_unicode_entries -- --ignored --nocapture
```

The OCR test requires `AIMEMORANDUM_OCR_FIXTURE` to point at a real image. The screenshot probe writes a real desktop capture under `%APPDATA%/AIMemorandum/captures`; treat that file as sensitive and do not commit it. Hardware probes are ignored in the normal suite so unsupported or locked-down environments do not produce false passes.
