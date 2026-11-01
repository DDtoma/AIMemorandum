use std::ffi::OsString;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CliAction {
    Launch,
    TogglePanel,
    Screenshot,
    SaveClipboard,
    ShowItems,
    Background,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("invalid UTF-8 argument")]
    InvalidUtf8,
    #[error("unknown argument: {0}")]
    Unknown(String),
    #[error("expected at most one action")]
    MultipleActions,
    #[error("invalid command frame: {0}")]
    InvalidFrame(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCommand {
    version: u8,
    action: CliAction,
}

pub fn encode_command(action: CliAction) -> Result<Vec<u8>, CommandError> {
    let mut frame = serde_json::to_vec(&WireCommand { version: 1, action })?;
    frame.push(b'\n');
    Ok(frame)
}

pub fn decode_command(frame: &[u8]) -> Result<CliAction, CommandError> {
    if frame.len() > 4096 || !frame.ends_with(b"\n") {
        return Err(CommandError::InvalidFrame(
            "expected one newline-terminated frame of at most 4096 bytes",
        ));
    }
    let command: WireCommand = serde_json::from_slice(&frame[..frame.len() - 1])?;
    if command.version != 1 {
        return Err(CommandError::InvalidFrame("unsupported protocol version"));
    }
    Ok(command.action)
}

pub fn parse_args<I, S>(args: I) -> Result<CliAction, CommandError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let action = match args.next() {
        None => return Ok(CliAction::Launch),
        Some(arg) => match arg.to_str().ok_or(CommandError::InvalidUtf8)? {
            "--toggle-panel" => CliAction::TogglePanel,
            "--screenshot" => CliAction::Screenshot,
            "--save-clipboard" => CliAction::SaveClipboard,
            "--items" => CliAction::ShowItems,
            "--background" => CliAction::Background,
            unknown => return Err(CommandError::Unknown(unknown.to_owned())),
        },
    };
    if args.next().is_some() {
        return Err(CommandError::MultipleActions);
    }
    Ok(action)
}
