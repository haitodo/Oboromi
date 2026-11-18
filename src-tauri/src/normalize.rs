use caseless::default_case_fold_str;
use chrono::{FixedOffset, NaiveDate, Utc};
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUrl {
    pub display_url: String,
    pub normalized_url: String,
    pub host: String,
}

/// Normalize a search key according to Spec 6.2:
/// NFKC -> Unicode default full casefold -> NFKC -> trim + collapse whitespace to single ASCII space.
pub fn normalize_search_key(input: &str) -> String {
    let nfkc1: String = input.nfkc().collect();
    let folded = default_case_fold_str(&nfkc1);
    let nfkc2: String = folded.nfkc().collect();
    nfkc2.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compute the exclusive BINARY upper bound for prefix search (Spec 6.2).
/// Walks characters from the end, finds the first scalar value that can be incremented,
/// increments it (skipping the surrogate range), and truncates any trailing characters.
pub fn prefix_upper_bound(prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }
    let indices: Vec<(usize, char)> = prefix.char_indices().collect();
    for &(byte_idx, ch) in indices.iter().rev() {
        let code = u32::from(ch);
        if code < 0x10_FFFF {
            let next_code = if code == 0xD7FF { 0xE000 } else { code + 1 };
            if let Some(next_char) = char::from_u32(next_code) {
                let mut upper = String::with_capacity(byte_idx + next_char.len_utf8());
                upper.push_str(&prefix[..byte_idx]);
                upper.push(next_char);
                return Some(upper);
            }
        }
    }
    None
}

/// Escape an already-normalized query into a single FTS5 phrase literal (Spec 6.2).
pub fn to_fts5_phrase(normalized: &str) -> String {
    let escaped = normalized.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Count Unicode scalar values in a string.
pub fn scalar_len(s: &str) -> usize {
    s.chars().count()
}

/// Validate and normalize an external http(s) URL (Spec 5.1).
pub fn validate_and_normalize_url(raw: &str) -> Result<NormalizedUrl, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("URLを入力してください".into()));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(AppError::Validation(
            "URLに制御文字を含めることはできません".into(),
        ));
    }

    let lower_prefix = trimmed.get(..8).unwrap_or(trimmed).to_ascii_lowercase();
    if !(lower_prefix.starts_with("http://") || lower_prefix.starts_with("https://")) {
        return Err(AppError::Validation(
            "URLは http:// または https:// で始めてください".into(),
        ));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|e| AppError::Validation(format!("URLの形式が正しくありません: {e}")))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::Validation(
            "URLは http:// または https:// のみ対応しています".into(),
        ));
    }

    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| AppError::Validation("URLにホスト名が含まれていません".into()))?
        .to_string();

    Ok(NormalizedUrl {
        display_url: trimmed.to_string(),
        normalized_url: parsed.to_string(),
        host,
    })
}

/// Try interpreting a search input as an explicit http(s) URL for exact normalized URL matching.
pub fn try_normalize_url_for_search(raw: &str) -> Option<String> {
    validate_and_normalize_url(raw)
        .ok()
        .map(|u| u.normalized_url)
}

/// Validate a calendar date in strict `YYYY-MM-DD` format (Spec 5).
pub fn validate_diary_date(date: &str) -> Result<NaiveDate, AppError> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        return Err(AppError::Validation(
            "日付は YYYY-MM-DD 形式で指定してください".into(),
        ));
    }

    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::Validation(format!("存在しない日付です: {date}")))
}

pub fn display_web_link_title(title: &str, host: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        host.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn display_page_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "無題".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn display_diary_title(date: &str) -> String {
    format!("{date}の日記")
}

pub fn now_utc_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn current_jst_date() -> String {
    let jst = FixedOffset::east_opt(9 * 3600).expect("valid JST offset");
    Utc::now().with_timezone(&jst).format("%Y-%m-%d").to_string()
}
