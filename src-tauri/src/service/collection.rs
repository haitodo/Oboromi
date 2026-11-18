use rusqlite::{params, types::Value, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    models::{CollectionSort, ResultPage},
};

use super::{decode_cursor, encode_cursor, PAGE_SIZE};

pub(super) enum Collection {
    WebLinks,
    Knowledge,
}

#[derive(Serialize, Deserialize)]
struct Cursor {
    sort: CollectionSort,
    key: Key,
    id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Key {
    Date(i64),
    Title(String),
}

pub(super) fn list<T>(
    conn: &Connection,
    collection: Collection,
    columns: &str,
    sort: CollectionSort,
    cursor: Option<&str>,
    read: impl Fn(&Row<'_>) -> Result<T, AppError>,
) -> Result<ResultPage<T>, AppError> {
    let cursor: Option<Cursor> = cursor
        .filter(|value| !value.trim().is_empty())
        .map(decode_cursor)
        .transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.sort != sort) {
        return Err(AppError::Validation(
            "カーソルの並び順が一致しません".into(),
        ));
    }

    let (table, kind, condition) = match collection {
        Collection::WebLinks => ("web_links", "web_link", "r.deleted_at_ms IS NULL"),
        Collection::Knowledge => (
            "pages",
            "knowledge_page",
            "r.deleted_at_ms IS NULL AND r.web_link_id IS NULL AND r.diary_date IS NULL",
        ),
    };
    let key = match sort {
        CollectionSort::UpdatedDesc | CollectionSort::UpdatedAsc => "r.updated_at_ms",
        CollectionSort::CreatedDesc | CollectionSort::CreatedAsc => "r.created_at_ms",
        CollectionSort::TitleDesc | CollectionSort::TitleAsc => "document.title_key",
    };
    let title_order = matches!(sort, CollectionSort::TitleAsc | CollectionSort::TitleDesc);
    let join = if title_order {
        format!("JOIN search_documents document ON document.kind = '{kind}' AND document.target_id = r.id")
    } else {
        String::new()
    };
    let (direction, comparison) = match sort {
        CollectionSort::UpdatedDesc | CollectionSort::CreatedDesc | CollectionSort::TitleDesc => {
            ("DESC", "<")
        }
        _ => ("ASC", ">"),
    };
    let after = if cursor.is_some() {
        format!("AND ({key} {comparison} ?2 OR ({key} = ?2 AND r.id > ?3))")
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT {columns}, {key} AS sort_key FROM {table} r {join}
         WHERE {condition} {after} ORDER BY sort_key {direction}, r.id ASC LIMIT ?1",
    );
    let limit = (PAGE_SIZE + 1) as i64;
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = match cursor {
        Some(cursor) => {
            let key = match cursor.key {
                Key::Date(value) if !title_order => Value::Integer(value),
                Key::Title(value) if title_order => Value::Text(value),
                _ => return Err(AppError::Validation("カーソルの形式が不正です".into())),
            };
            stmt.query(params![limit, key, cursor.id])?
        }
        None => stmt.query([limit])?,
    };

    let mut items = Vec::with_capacity(PAGE_SIZE);
    let mut last = None;
    let mut next_cursor = None;
    while let Some(row) = rows.next()? {
        if items.len() == PAGE_SIZE {
            next_cursor = last.as_ref().map(encode_cursor).transpose()?;
            break;
        }
        items.push(read(row)?);
        if items.len() == PAGE_SIZE {
            let key = if title_order {
                Key::Title(row.get("sort_key")?)
            } else {
                Key::Date(row.get("sort_key")?)
            };
            last = Some(Cursor {
                sort,
                key,
                id: row.get(0)?,
            });
        }
    }
    Ok(ResultPage { items, next_cursor })
}
