#![cfg(windows)]

use crate::{decode_command, encode_command, support::wide, CliAction};
use std::{fmt, io};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_EXISTS,
        ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE,
        OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    },
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    },
};

pub fn pipe_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".into());
    let user: String = user
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(r"\\.\pipe\AIMemorandum-{user}")
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub struct IpcServer {
    handle: OwnedHandle,
}

// SAFETY: IpcServer exclusively owns a named-pipe HANDLE. Win32 pipe handles may be
// used from any thread; moving the server transfers ownership and no thread-local
// state or concurrent access is involved.
unsafe impl Send for IpcServer {}

impl fmt::Debug for IpcServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IpcServer").finish_non_exhaustive()
    }
}

impl IpcServer {
    pub fn bind() -> io::Result<Self> {
        Self::bind_to(&pipe_name())
    }

    pub fn bind_to(name: &str) -> io::Result<Self> {
        let name = wide(name);
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(code) if code == ERROR_ACCESS_DENIED as i32 || code == ERROR_FILE_EXISTS as i32 || code == ERROR_PIPE_BUSY as i32)
            {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, error));
            }
            return Err(error);
        }
        Ok(Self {
            handle: OwnedHandle(handle),
        })
    }

    pub fn accept_once(&self) -> io::Result<CliAction> {
        let connected = unsafe { ConnectNamedPipe(self.handle.0, std::ptr::null_mut()) };
        if connected == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
            return Err(io::Error::last_os_error());
        }

        let result = self.read_action();
        unsafe { DisconnectNamedPipe(self.handle.0) };
        result
    }

    fn read_action(&self) -> io::Result<CliAction> {
        let mut frame = Vec::with_capacity(128);
        let mut buffer = [0_u8; 512];
        loop {
            let mut read = 0;
            let ok = unsafe {
                ReadFile(
                    self.handle.0,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_BROKEN_PIPE && !frame.is_empty() {
                    break;
                }
                return Err(io::Error::last_os_error());
            }
            frame.extend_from_slice(&buffer[..read as usize]);
            if frame.ends_with(b"\n") || frame.len() > 4096 {
                break;
            }
        }
        decode_command(&frame).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

pub struct IpcClient;

impl IpcClient {
    pub fn send(action: CliAction) -> io::Result<()> {
        Self::send_to(&pipe_name(), action)
    }

    pub fn send_to(name: &str, action: CliAction) -> io::Result<()> {
        let name = wide(name);
        let mut handle = open_pipe(name.as_ptr());
        if handle == INVALID_HANDLE_VALUE && unsafe { GetLastError() } == ERROR_PIPE_BUSY {
            if unsafe { WaitNamedPipeW(name.as_ptr(), 5000) } == 0 {
                return Err(io::Error::last_os_error());
            }
            handle = open_pipe(name.as_ptr());
        }
        if handle == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            if unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
                return Err(io::Error::new(io::ErrorKind::NotFound, error));
            }
            return Err(error);
        }
        let handle = OwnedHandle(handle);
        let frame = encode_command(action)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut written = 0;
        let ok = unsafe {
            WriteFile(
                handle.0,
                frame.as_ptr(),
                frame.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || written as usize != frame.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write complete IPC frame",
            ));
        }
        if unsafe { FlushFileBuffers(handle.0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn open_pipe(name: *const u16) -> HANDLE {
    unsafe {
        CreateFileW(
            name,
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    }
}
