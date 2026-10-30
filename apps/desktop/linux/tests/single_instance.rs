#![cfg(target_os = "linux")]

use aimemorandum_linux::{send_command, CliAction, IpcError, PrimaryInstance};
use std::{os::unix::fs::PermissionsExt, thread, time::Duration};

#[test]
fn primary_forwards_one_command_and_enforces_single_instance() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("aimemorandum.sock");
    let primary = PrimaryInstance::acquire(&socket).unwrap();
    assert_eq!(
        std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(matches!(
        PrimaryInstance::acquire(&socket),
        Err(IpcError::AlreadyRunning)
    ));

    let server = thread::spawn(move || {
        primary.serve_next(|action| {
            assert_eq!(action, CliAction::SaveClipboard);
            Ok(())
        })
    });
    send_command(&socket, CliAction::SaveClipboard).unwrap();
    server.join().unwrap().unwrap();
}

#[test]
fn stale_socket_is_reclaimed_and_live_socket_is_not_removed() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("aimemorandum.sock");
    std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(socket.exists());

    let primary = PrimaryInstance::acquire(&socket).unwrap();
    assert!(matches!(
        PrimaryInstance::acquire(&socket),
        Err(IpcError::AlreadyRunning)
    ));
    assert!(socket.exists());
    drop(primary);
    assert!(!socket.exists());
}

#[test]
fn malformed_clients_receive_an_error_without_stopping_the_primary() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("aimemorandum.sock");
    let primary = PrimaryInstance::acquire(&socket).unwrap();
    let server = thread::spawn(move || {
        let _ = primary.serve_next(|_| Ok(()));
        thread::sleep(Duration::from_millis(20));
        primary.serve_next(|action| {
            assert_eq!(action, CliAction::TogglePanel);
            Ok(())
        })
    });
    let mut client = std::os::unix::net::UnixStream::connect(&socket).unwrap();
    use std::io::{Read, Write};
    client.write_all(b"not json\n").unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("error:"));
    send_command(&socket, CliAction::TogglePanel).unwrap();
    server.join().unwrap().unwrap();
}
