use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{ApiError, Item, LocalStore, StoreError, SyncApi};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncStats {
    pub pushed: usize,
    pub uploaded: usize,
    pub pulled: usize,
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

pub struct SyncEngine<'a, A: SyncApi> {
    store: &'a LocalStore,
    api: &'a A,
}

impl<'a, A: SyncApi> SyncEngine<'a, A> {
    pub fn new(store: &'a LocalStore, api: &'a A) -> Self {
        Self { store, api }
    }

    pub fn run_once(&self) -> Result<SyncStats, SyncError> {
        let pushed = self.push_pending()?;
        let uploaded = self.upload_attachments()?;
        let pulled = self.pull_changes()?;
        Ok(SyncStats {
            pushed,
            uploaded,
            pulled,
        })
    }

    fn push_pending(&self) -> Result<usize, SyncError> {
        let mut pushed = 0;
        let mut attempted = 0;
        while attempted < 100 {
            let queued = self
                .store
                .pending()?
                .into_iter()
                .find(|operation| operation.state == "queued");
            let Some(queued) = queued else { break };
            let Some(operation) = self.store.prepare(&queued)? else {
                break;
            };
            attempted += 1;
            match self.api.submit(&operation) {
                Ok(remote) => {
                    self.store.acknowledge(&operation, remote)?;
                    pushed += 1;
                }
                Err(error) => match (&error, error.item().cloned()) {
                    (
                        ApiError::Http {
                            status: 409, name, ..
                        },
                        remote,
                    ) if name == "REVISION_CONFLICT" => {
                        let Some(remote) = remote else {
                            return Err(ApiError::Decode(
                                "REVISION_CONFLICT response did not include the current item"
                                    .to_owned(),
                            )
                            .into());
                        };
                        self.store
                            .fail(&operation, &error.to_string(), true, Some(remote))?;
                    }
                    (
                        ApiError::Http {
                            status: 409, name, ..
                        },
                        remote,
                    ) if name == "ITEM_EXISTS" => {
                        let remote = match remote {
                            Some(item) => item,
                            None => match self.api.item(&operation.item_id) {
                                Ok(item) => item,
                                Err(error) if is_permanent(&error) => {
                                    self.store
                                        .fail(&operation, &error.to_string(), true, None)?;
                                    continue;
                                }
                                Err(error) => return Err(error.into()),
                            },
                        };
                        self.store.acknowledge(&operation, remote)?;
                        pushed += 1;
                    }
                    _ if is_permanent(&error) => {
                        self.store
                            .fail(&operation, &error.to_string(), true, None)?;
                    }
                    _ => return Err(error.into()),
                },
            }
        }
        Ok(pushed)
    }

    fn upload_attachments(&self) -> Result<usize, SyncError> {
        let mut uploaded = 0;
        for item in self.store.items()? {
            if !item
                .attachments
                .iter()
                .any(|attachment| attachment.state == "pending")
            {
                continue;
            }
            if self
                .store
                .shadow(&item.id)?
                .map_or(0, |shadow| shadow.revision)
                == 0
            {
                continue;
            }
            for attachment in item
                .attachments
                .into_iter()
                .filter(|attachment| attachment.state == "pending")
            {
                let bytes = std::fs::read(self.store.attachment_path(&attachment))
                    .map_err(StoreError::from)?;
                let digest = format!("{:x}", Sha256::digest(&bytes));
                if bytes.len() as u64 != attachment.byte_size || digest != attachment.sha256 {
                    return Err(StoreError::InvalidState(format!(
                        "attachment {} failed size or SHA-256 validation before upload",
                        attachment.id
                    ))
                    .into());
                }
                match self.api.upload_attachment(&attachment.id, &bytes) {
                    Ok(()) => {
                        let mut ready = attachment;
                        ready.state = "ready".to_owned();
                        self.store.update_attachment(&ready)?;
                    }
                    Err(ApiError::Http {
                        status: 409, name, ..
                    }) if name == "INVALID_STATE" => {
                        let remote = self.api.item(&item.id)?;
                        let ready = matching_ready_attachment(&remote, &attachment)?;
                        self.store.update_attachment(ready)?;
                    }
                    Err(error) => return Err(error.into()),
                }
                uploaded += 1;
            }
        }
        Ok(uploaded)
    }

    fn pull_changes(&self) -> Result<usize, SyncError> {
        let cursor = self.store.meta("cursor")?.unwrap_or_else(|| "0".to_owned());
        let page = self.api.pull(&cursor)?;
        if page.items.is_empty() && page.cursor == cursor {
            return Ok(0);
        }
        let count = page.items.len();
        self.store.apply_page(&page.items, &page.cursor)?;
        Ok(count)
    }
}

impl ApiError {
    fn item(&self) -> Option<&Item> {
        match self {
            ApiError::Http { item, .. } => item.as_deref(),
            _ => None,
        }
    }
}

fn is_permanent(error: &ApiError) -> bool {
    match error {
        ApiError::Http { status, name, .. } => matches!(
            (*status, name.as_str()),
            (404, "NOT_FOUND")
                | (409, "STALE_SCHEDULE")
                | (400, "INVALID_CONTENT")
                | (413, "ATTACHMENT_TOO_LARGE")
                | (422, "INVALID_STATE")
                | (422, "UNSUPPORTED_ACTION")
                | (422, "STALE_SCHEDULE")
                | (422, "INVALID_CONTENT")
                | (422, "INVALID_TIME")
        ),
        _ => false,
    }
}

fn matching_ready_attachment<'a>(
    remote: &'a Item,
    local: &crate::Attachment,
) -> Result<&'a crate::Attachment, SyncError> {
    remote
        .attachments
        .iter()
        .find(|candidate| {
            candidate.id == local.id
                && candidate.state == "ready"
                && candidate.byte_size == local.byte_size
                && candidate.sha256 == local.sha256
        })
        .ok_or_else(|| {
            ApiError::Decode(format!(
                "duplicate upload could not be reconciled for attachment {}",
                local.id
            ))
            .into()
        })
}
