use std::{fs, io, path::Path};

pub fn set_autostart_at(path: &Path, executable: &Path, enabled: bool) -> io::Result<()> {
    if !enabled {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "autostart path must have a parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let executable = executable.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "executable path must be valid UTF-8",
        )
    })?;
    if executable.contains(['\r', '\n']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "executable path contains a newline",
        ));
    }
    let contents = format!(
        "[Desktop Entry]\nType=Application\nName=AIMemorandum\nComment=Quick capture and local-first reminders\nExec={} --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        quote_exec(executable)
    );
    if fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    let temporary = parent.join(format!(".aimemorandum.desktop.{}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

fn quote_exec(executable: &str) -> String {
    if executable.chars().any(char::is_whitespace)
        || executable.contains(['\"', '\'', '\\', '$'])
        || executable.contains(char::from(96))
    {
        let escaped = executable
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('$', "\\$")
            .replace(char::from(96), &format!("\\{}", char::from(96)));
        format!("\"{}\"", escaped)
    } else {
        executable.to_owned()
    }
}
