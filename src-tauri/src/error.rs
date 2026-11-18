use serde::Serialize;
use thiserror::Error;

use crate::models::WebLinkSummary;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    NotFound(String),

    #[error("同じURLのWebリンクが既に存在します")]
    DuplicateUrl { candidates: Vec<WebLinkSummary> },

    #[error("他の変更と競合しました。保存済みの内容を再読み込みしてください")]
    RevisionConflict {
        current_revision: Option<i64>,
        current_updated_at_ms: Option<i64>,
    },

    #[error("{0}")]
    StorageError(String),

    #[error("{0}")]
    IncompatibleBackup(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<WebLinkSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_updated_at_ms: Option<i64>,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "Validation",
            AppError::NotFound(_) => "NotFound",
            AppError::DuplicateUrl { .. } => "DuplicateUrl",
            AppError::RevisionConflict { .. } => "RevisionConflict",
            AppError::StorageError(_) => "StorageError",
            AppError::IncompatibleBackup(_) => "IncompatibleBackup",
        }
    }

    pub fn to_payload(&self) -> ErrorPayload {
        match self {
            AppError::DuplicateUrl { candidates } => ErrorPayload {
                code: self.code(),
                message: self.to_string(),
                candidates: Some(candidates.clone()),
                current_revision: None,
                current_updated_at_ms: None,
            },
            AppError::RevisionConflict {
                current_revision,
                current_updated_at_ms,
            } => ErrorPayload {
                code: self.code(),
                message: self.to_string(),
                candidates: None,
                current_revision: *current_revision,
                current_updated_at_ms: *current_updated_at_ms,
            },
            _ => ErrorPayload {
                code: self.code(),
                message: self.to_string(),
                candidates: None,
                current_revision: None,
                current_updated_at_ms: None,
            },
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_payload().serialize(serializer)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::StorageError(format!("データベースエラー: {err}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::StorageError(format!("ファイル操作エラー: {err}"))
    }
}
