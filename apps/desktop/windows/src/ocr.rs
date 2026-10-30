use aimemorandum_ui::{TextRecognizer, UiError};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use windows::{
    core::{Result, HSTRING},
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
        ExifOrientationMode,
    },
    Media::Ocr::OcrEngine,
    Storage::{FileAccessMode, StorageFile},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    #[error("OCR worker thread stopped unexpectedly")]
    WorkerStopped,
    #[error("could not resolve image path: {0}")]
    Path(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default)]
pub struct WindowsOcr;

impl WindowsOcr {
    pub fn user_profile() -> Self {
        Self
    }

    pub fn recognize_file(&self, path: &Path) -> std::result::Result<String, OcrError> {
        let path = absolute(path)?;
        std::thread::spawn(move || recognize_on_mta(&path))
            .join()
            .map_err(|_| OcrError::WorkerStopped)?
    }
}

fn recognize_on_mta(path: &Path) -> std::result::Result<String, OcrError> {
    let _apartment = Apartment::initialize()?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))?.join()?;
    let stream = file.OpenAsync(FileAccessMode::Read)?.join()?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
    let (width, height) = fit_within_max_dimension(
        decoder.PixelWidth()?,
        decoder.PixelHeight()?,
        OcrEngine::MaxImageDimension()?,
    );
    let bitmap = if (width, height) == (decoder.PixelWidth()?, decoder.PixelHeight()?) {
        decoder
            .GetSoftwareBitmapConvertedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
            )?
            .join()?
    } else {
        let transform = BitmapTransform::new()?;
        transform.SetScaledWidth(width)?;
        transform.SetScaledHeight(height)?;
        decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &transform,
                ExifOrientationMode::RespectExifOrientation,
                ColorManagementMode::ColorManageToSRgb,
            )?
            .join()?
    };
    Ok(engine
        .RecognizeAsync(&bitmap)?
        .join()?
        .Text()?
        .to_string_lossy())
}

impl TextRecognizer for WindowsOcr {
    fn recognize(&self, path: &Path) -> std::result::Result<String, UiError> {
        self.recognize_file(path)
            .map_err(|error| UiError::Platform(format!("Windows OCR: {error}")))
    }
}

struct Apartment;

impl Apartment {
    fn initialize() -> Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub fn fit_within_max_dimension(width: u32, height: u32, max: u32) -> (u32, u32) {
    if width == 0 || height == 0 || (width <= max && height <= max) {
        return (width, height);
    }
    if width >= height {
        (
            max,
            ((height as u64 * max as u64) / width as u64).max(1) as u32,
        )
    } else {
        (
            ((width as u64 * max as u64) / height as u64).max(1) as u32,
            max,
        )
    }
}

fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // StorageFile rejects forward slashes even though ordinary Win32 file APIs
    // accept them. Normalize at the WinRT boundary without losing non-Unicode paths.
    let native: Vec<u16> = absolute
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == b'/' as u16 {
                b'\\' as u16
            } else {
                unit
            }
        })
        .collect();
    Ok(PathBuf::from(OsString::from_wide(&native)))
}
