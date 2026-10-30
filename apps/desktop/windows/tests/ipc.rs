#![cfg(windows)]

use aimemorandum_windows::{CliAction, IpcClient, IpcServer};
use std::time::{SystemTime, UNIX_EPOCH};

fn pipe_name() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(r"\\.\pipe\aimemorandum-test-{}-{nonce}", std::process::id())
}

#[test]
fn named_pipe_enforces_single_instance_and_forwards_one_action() {
    let name = pipe_name();
    let server = IpcServer::bind_to(&name).unwrap();
    let error = IpcServer::bind_to(&name).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists, "{error:?}");

    let sender = std::thread::spawn({
        let name = name.clone();
        move || IpcClient::send_to(&name, CliAction::TogglePanel).unwrap()
    });
    assert_eq!(server.accept_once().unwrap(), CliAction::TogglePanel);
    sender.join().unwrap();
}
