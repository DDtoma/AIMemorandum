use thiserror::Error;

#[derive(Debug, Error)]
pub enum UiError {
    #[error(transparent)]
    Store(#[from] aimemorandum_core::StoreError),
    #[error(transparent)]
    Domain(#[from] aimemorandum_core::CoreError),
    #[error("平台能力调用失败：{0}")]
    Platform(String),
    #[error("图片处理失败：{0}")]
    Image(#[from] image::ImageError),
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("界面状态无效：{0}")]
    InvalidState(String),
    #[error("记录不存在：{0}")]
    NotFound(String),
    #[error("当前记录有未保存修改")]
    UnsavedChanges,
}
