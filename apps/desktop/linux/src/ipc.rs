use crate::{decode_command, encode_command, CliAction, CommandError};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_RESPONSE_BYTES: u64 = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("another AIMemorandum instance is already running")]
    AlreadyRunning,
    #[error("IPC peer belongs to a different user")]
    ForeignPeer,
    #[error("primary instance rejected the command: {0}")]
    Remote(String),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct PrimaryInstance {
    listener: UnixListener,
    path: PathBuf,
}

impl PrimaryInstance {
    pub fn acquire(path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let path = path.as_ref();
        match Self::bind(path) {
            Ok(primary) => Ok(primary),
            Err(IpcError::Io(error)) if error.kind() == io::ErrorKind::AddrInUse => {
                if UnixStream::connect(path).is_ok() {
                    return Err(IpcError::AlreadyRunning);
                }
                remove_socket_if_owned(path)?;
                Self::bind(path).map_err(|error| match error {
                    IpcError::Io(error) if error.kind() == io::ErrorKind::AddrInUse => {
                        IpcError::AlreadyRunning
                    }
                    other => other,
                })
            }
            Err(error) => Err(error),
        }
    }

    fn bind(path: &Path) -> Result<Self, IpcError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn serve_next<F>(&self, handle: F) -> Result<(), IpcError>
    where
        F: FnOnce(CliAction) -> Result<(), String>,
    {
        let (mut stream, _) = self.listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let result = serve_stream(&mut stream, handle);
        let response = match &result {
            Ok(()) => "ok\n".to_owned(),
            Err(error) => format!("error:{}\n", single_line(&error.to_string())),
        };
        stream.write_all(response.as_bytes())?;
        result
    }
}

impl Drop for PrimaryInstance {
    fn drop(&mut self) {
        let _ = remove_socket_if_owned(&self.path);
    }
}

pub fn send_command(path: impl AsRef<Path>, action: CliAction) -> Result<(), IpcError> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(&encode_command(action)?)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut response = String::new();
    stream
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut response)?;
    match response.as_str() {
        "ok\n" => Ok(()),
        value if value.starts_with("error:") => Err(IpcError::Remote(
            value
                .trim_end_matches('\n')
                .trim_start_matches("error:")
                .to_owned(),
        )),
        _ => Err(IpcError::Remote("invalid primary response".to_owned())),
    }
}

fn serve_stream<F>(stream: &mut UnixStream, handle: F) -> Result<(), IpcError>
where
    F: FnOnce(CliAction) -> Result<(), String>,
{
    if peer_uid(stream)? != current_uid() {
        return Err(IpcError::ForeignPeer);
    }
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut frame = Vec::new();
    reader.by_ref().take(4097).read_until(b'\n', &mut frame)?;
    let action = decode_command(&frame)?;
    handle(action).map_err(IpcError::Remote)
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    unsafe { libc::geteuid() }
}

fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the fd is valid, and credentials/length point to writable values of the stated size.
    let result = unsafe {
        libc::getsockopt(
            std::os::fd::AsRawFd::as_raw_fd(stream),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(credentials.uid)
    }
}

fn remove_socket_if_owned(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_socket() || metadata.uid() != current_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to remove a socket not owned by the current user",
        ));
    }
    fs::remove_file(path)
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .take(512)
        .collect::<String>()
        .replace(['\r', '\n'], " ")
}
