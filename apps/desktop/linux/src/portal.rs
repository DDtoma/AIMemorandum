use crate::{platform_error, FrameProvider, MonitorGeometry, StaticFrame};
use aimemorandum_ui::UiError;
use ashpd::desktop::{
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
    PersistMode,
};
use image::RgbaImage;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::{
    buffer::{Data, DataType},
    param::video::{VideoFormat, VideoInfoRaw},
};
use std::{
    cell::RefCell,
    error::Error,
    fs,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

pub struct PortalFrameProvider {
    restore_token_path: PathBuf,
}

impl PortalFrameProvider {
    pub fn new(restore_token_path: PathBuf) -> Self {
        Self { restore_token_path }
    }
}

impl FrameProvider for PortalFrameProvider {
    fn capture_frame(&mut self) -> Result<StaticFrame, UiError> {
        let saved = fs::read_to_string(&self.restore_token_path)
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(platform_error)?;
        runtime
            .block_on(capture_portal_frame(
                saved.as_deref(),
                &self.restore_token_path,
            ))
            .map_err(platform_error)
    }
}

struct Frame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

async fn capture_portal_frame(
    saved_token: Option<&str>,
    token_path: &Path,
) -> Result<StaticFrame, Box<dyn Error>> {
    let portal = Screencast::new().await?;
    let session = portal.create_session(Default::default()).await?;
    let result: Result<StaticFrame, Box<dyn Error>> = async {
        portal
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(CursorMode::Hidden)
                    .set_sources(Some(SourceType::Monitor.into()))
                    .set_multiple(false)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(saved_token),
            )
            .await?;
        let response = portal
            .start(&session, None, Default::default())
            .await?
            .response()?;
        if let Some(token) = response.restore_token() {
            persist_token(token_path, token)?;
        }
        let stream = response
            .streams()
            .first()
            .ok_or("portal returned no streams")?;
        let node_id = stream.pipe_wire_node_id();
        let (x, y) = stream.position().unwrap_or((0, 0));
        let logical_size = stream.size();
        let fd = portal
            .open_pipe_wire_remote(&session, Default::default())
            .await?;
        let frame = read_pipewire_frame(node_id, fd)?;
        let (logical_width, logical_height) =
            logical_size.unwrap_or((frame.width as i32, frame.height as i32));
        if logical_width <= 0 || logical_height <= 0 {
            return Err("portal returned invalid logical monitor dimensions".into());
        }
        let scale_x = frame.width as f64 / logical_width as f64;
        let scale_y = frame.height as f64 / logical_height as f64;
        if !scale_x.is_finite() || scale_x <= 0.0 || (scale_x - scale_y).abs() > 0.01 {
            return Err("portal returned unsupported non-uniform monitor scaling".into());
        }
        let image = RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
            .ok_or("PipeWire returned an invalid RGBA buffer")?;
        Ok(StaticFrame::new(
            image,
            MonitorGeometry {
                x,
                y,
                width: logical_width as u32,
                height: logical_height as u32,
                scale_factor: scale_x,
            },
        ))
    }
    .await;
    let closed = session.close().await;
    match result {
        Ok(frame) => {
            closed?;
            Ok(frame)
        }
        Err(error) => Err(error),
    }
}

fn persist_token(path: &Path, token: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("restore-token"))
        .to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), Box<dyn Error>> {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn copy_rgba(info: VideoInfoRaw, data: &mut Data) -> Result<Frame, String> {
    if data.type_() != DataType::MemPtr && data.type_() != DataType::MemFd {
        return Err(format!(
            "unsupported PipeWire buffer type {:?}",
            data.type_()
        ));
    }
    let width = info.size().width as usize;
    let height = info.size().height as usize;
    if width == 0 || height == 0 {
        return Err("zero-sized frame".into());
    }
    let row_bytes = width.checked_mul(4).ok_or("row size overflow")?;
    let out_len = row_bytes.checked_mul(height).ok_or("frame size overflow")?;
    let (offset, chunk_size, stride, corrupted) = {
        let chunk = data.chunk();
        (
            chunk.offset() as usize,
            chunk.size() as usize,
            chunk.stride(),
            chunk.flags().contains(spa::buffer::ChunkFlags::CORRUPTED),
        )
    };
    if corrupted {
        return Err("PipeWire marked buffer corrupted".into());
    }
    if stride <= 0 {
        return Err(format!("unsupported stride {stride}"));
    }
    let stride = stride as usize;
    if stride < row_bytes {
        return Err("stride smaller than packed row".into());
    }
    let needed = stride
        .checked_mul(height - 1)
        .and_then(|size| size.checked_add(row_bytes))
        .ok_or("buffer size overflow")?;
    if chunk_size < needed {
        return Err(format!("short chunk: {chunk_size} bytes, need {needed}"));
    }
    let end = offset.checked_add(needed).ok_or("chunk offset overflow")?;
    let memory = data
        .data()
        .ok_or("buffer is not CPU-mapped (DMA-Buf needs a separate importer)")?;
    let src = memory
        .get(offset..end)
        .ok_or("chunk exceeds maxsize mapping")?;
    let (ri, gi, bi, ai) = match info.format() {
        VideoFormat::RGBA => (0, 1, 2, Some(3)),
        VideoFormat::BGRA => (2, 1, 0, Some(3)),
        VideoFormat::ARGB => (1, 2, 3, Some(0)),
        VideoFormat::ABGR => (3, 2, 1, Some(0)),
        VideoFormat::RGBx => (0, 1, 2, None),
        VideoFormat::BGRx => (2, 1, 0, None),
        VideoFormat::xRGB => (1, 2, 3, None),
        VideoFormat::xBGR => (3, 2, 1, None),
        format => return Err(format!("unsupported video format {format:?}")),
    };
    let mut rgba = vec![0; out_len];
    for y in 0..height {
        for x in 0..width {
            let pixel = &src[y * stride + x * 4..][..4];
            let output = &mut rgba[y * row_bytes + x * 4..][..4];
            output[0] = pixel[ri];
            output[1] = pixel[gi];
            output[2] = pixel[bi];
            output[3] = ai.map_or(255, |index| pixel[index]);
        }
    }
    Ok(Frame {
        width: info.size().width,
        height: info.size().height,
        rgba,
    })
}

struct CaptureState {
    info: VideoInfoRaw,
    mainloop: Rc<pw::main_loop::MainLoopBox>,
    result: Rc<RefCell<Option<Result<Frame, String>>>>,
}

fn run_capture_loop(
    mainloop: &Rc<pw::main_loop::MainLoopBox>,
    result: &Rc<RefCell<Option<Result<Frame, String>>>>,
    timeout: Duration,
) -> Result<(), Box<dyn Error>> {
    let timeout_loop = Rc::clone(mainloop);
    let timeout_result = Rc::clone(result);
    let timer = mainloop.loop_().add_timer(move |_| {
        let mut outcome = timeout_result.borrow_mut();
        if outcome.is_none() {
            *outcome = Some(Err("timed out waiting for a PipeWire frame".to_owned()));
        }
        timeout_loop.quit();
    });
    timer.update_timer(Some(timeout), None).into_result()?;
    mainloop.run();
    Ok(())
}

fn read_pipewire_frame(node_id: u32, fd: OwnedFd) -> Result<Frame, Box<dyn Error>> {
    pw::init();
    let mainloop = Rc::new(pw::main_loop::MainLoopBox::new(None)?);
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)?;
    let core = context.connect_fd(fd, None)?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "single-frame",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;
    let result: Rc<RefCell<Option<Result<Frame, String>>>> = Rc::new(RefCell::new(None));
    let state = CaptureState {
        info: VideoInfoRaw::default(),
        mainloop: mainloop.clone(),
        result: result.clone(),
    };
    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(|_, state, _, new| {
            if let pw::stream::StreamState::Error(error) = new {
                let mut slot = state.result.borrow_mut();
                if slot.is_none() {
                    *slot = Some(Err(format!("PipeWire stream error: {error}")));
                    state.mainloop.quit();
                }
            }
        })
        .param_changed(|_, state, id, param| {
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            state.info = VideoInfoRaw::default();
            let Some(param) = param else {
                return;
            };
            let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != spa::param::format::MediaType::Video
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            let mut info = VideoInfoRaw::new();
            if info.parse(param).is_ok() {
                state.info = info;
            }
        })
        .process(|stream, state| {
            if state.result.borrow().is_some() {
                return;
            }
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let copied = {
                let datas = buffer.datas_mut();
                if datas.len() != 1 {
                    Err(format!("expected one packed plane, got {}", datas.len()))
                } else {
                    copy_rgba(state.info, &mut datas[0])
                }
            };
            drop(buffer);
            *state.result.borrow_mut() = Some(copied);
            let _ = stream.set_active(false);
            state.mainloop.quit();
        })
        .register()?;
    let object = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::RGBA,
            VideoFormat::RGBA,
            VideoFormat::BGRA,
            VideoFormat::ARGB,
            VideoFormat::ABGR,
            VideoFormat::RGBx,
            VideoFormat::BGRx,
            VideoFormat::xRGB,
            VideoFormat::xBGR
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 32768,
                height: 32768
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 30, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction {
                num: 1000,
                denom: 1
            }
        ),
    );
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).ok_or("invalid format pod")?];
    stream.connect(
        spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    run_capture_loop(&mainloop, &result, Duration::from_secs(10))?;
    let outcome = result.borrow_mut().take();
    match outcome {
        Some(Ok(frame)) => Ok(frame),
        Some(Err(error)) => Err(error.into()),
        None => Err("PipeWire stopped without a frame".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{persist_token, run_capture_loop};
    use pipewire as pw;
    use std::{cell::RefCell, error::Error, rc::Rc, time::Duration};

    #[test]
    fn token_persistence_cleans_up_and_can_retry_after_a_failure() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("restore-token");
        std::fs::create_dir(&destination).unwrap();

        assert!(persist_token(&destination, "first").is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);

        std::fs::remove_dir(&destination).unwrap();
        persist_token(&destination, "second").unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "second");
    }
    #[test]
    fn capture_loop_times_out_without_a_frame() {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = (|| -> Result<(), Box<dyn Error>> {
                let mainloop = Rc::new(pw::main_loop::MainLoopBox::new(None)?);
                let result = Rc::new(RefCell::new(None));
                run_capture_loop(&mainloop, &result, Duration::from_millis(20))?;
                let captured = result.borrow_mut().take();
                match captured {
                    Some(Err(message)) if message.contains("timed out") => Ok(()),
                    Some(Err(message)) => Err(format!("unexpected error: {message}").into()),
                    Some(Ok(_)) => Err("unexpected frame".into()),
                    None => Err("missing timeout result".into()),
                }
            })();
            let _ = sender.send(outcome.map_err(|error| error.to_string()));
        });
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("capture-loop worker failed or did not exit")
            .unwrap();
    }
}
