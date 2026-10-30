use std::{ffi::OsString, io, path::PathBuf};

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.has_root())
}

fn required_home(home: Option<OsString>) -> io::Result<PathBuf> {
    absolute(home).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "$HOME is not set to an absolute path",
        )
    })
}

pub fn data_dir() -> io::Result<PathBuf> {
    data_dir_from(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
}

pub fn data_dir_from(
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> io::Result<PathBuf> {
    Ok(match absolute(xdg_data_home) {
        Some(root) => root.join("aimemorandum"),
        None => required_home(home)?.join(".local/share/aimemorandum"),
    })
}

pub fn autostart_path() -> io::Result<PathBuf> {
    autostart_path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub fn autostart_path_from(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> io::Result<PathBuf> {
    Ok(match absolute(xdg_config_home) {
        Some(root) => root,
        None => required_home(home)?.join(".config"),
    }
    .join("autostart/aimemorandum.desktop"))
}

pub fn socket_path() -> io::Result<PathBuf> {
    socket_path_from(
        std::env::var_os("XDG_RUNTIME_DIR"),
        Some(data_dir()?.into_os_string()),
    )
}

pub fn socket_path_from(
    xdg_runtime_dir: Option<OsString>,
    fallback_dir: Option<OsString>,
) -> io::Result<PathBuf> {
    absolute(xdg_runtime_dir)
        .or_else(|| absolute(fallback_dir))
        .map(|root| root.join("aimemorandum.sock"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "$XDG_RUNTIME_DIR is not set and no data directory is available",
            )
        })
}
