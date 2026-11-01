#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_main;

#[cfg(windows)]
fn main() {
    windows_main::main();
}

#[cfg(not(windows))]
fn main() {}
