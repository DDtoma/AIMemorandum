use std::{ffi::OsString, io, path::PathBuf};

pub fn data_dir() -> io::Result<PathBuf> {
    data_dir_from(std::env::var_os("APPDATA"))
}

pub fn data_dir_from(appdata: Option<OsString>) -> io::Result<PathBuf> {
    let appdata =
        appdata.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "%APPDATA% is not set"))?;
    Ok(PathBuf::from(appdata).join("AIMemorandum"))
}
