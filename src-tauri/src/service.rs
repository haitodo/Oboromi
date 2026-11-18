mod collection;
mod search;

pub use search::search;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{
    sync_children_search_docs, sync_page_search_doc, sync_web_link_search_doc, DbManager,
};
use crate::error::AppError;
use crate::markdown::{extract_search_text, normalize_line_endings, render_markdown_preview};
use crate::models::{
    CollectionSort, CreateWebLinkInput, KnowledgePageSummary, PageKind, PageSummary, PageView, ParentRef,
    ResultPage, SaveReceipt, Tag, TagSummary, TrashEntry, TrashKind, UpdateWebLinkPatch,
    WebLinkDetail, WebLinkPreview, WebLinkSummary,
};
use crate::normalize::{
    display_diary_title, display_page_title, display_web_link_title, normalize_search_key,
    now_utc_ms, prefix_upper_bound, validate_and_normalize_url, validate_diary_date,
};
use crate::web_metadata::{decode_thumbnail, thumbnail_data_url};

const PAGE_SIZE: usize = 30;
const WEB_LINK_COLUMNS: &str = "r.id, r.url, r.normalized_url, r.title, r.description, r.created_at_ms, r.updated_at_ms,
    EXISTS(SELECT 1 FROM web_link_thumbnails WHERE web_link_id = r.id)";

fn encode_cursor<T: Serialize>(cursor: &T) -> Result<String, AppError> {
    let json = serde_json::to_vec(cursor)
        .map_err(|e| AppError::StorageError(format!("カーソルの生成に失敗しました: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

fn decode_cursor<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| AppError::Validation("カーソルが不正です".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| AppError::Validation("カーソルの形式が不正です".into()))
}

fn host_from_normalized_url(normalized_url: &str) -> String {
    url::Url::parse(normalized_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn read_web_link_summary(conn: &Connection, row: &rusqlite::Row<'_>) -> Result<WebLinkSummary, AppError> {
    let id: String = row.get(0)?;
    let normalized_url: String = row.get(2)?;
    let title: String = row.get(3)?;
    let host = host_from_normalized_url(&normalized_url);
    let display_title = display_web_link_title(&title, &host);
    let tags = fetch_tags_for_web_link(conn, &id)?;
    Ok(WebLinkSummary {
        id,
        url: row.get(1)?,
        normalized_url,
        title,
        display_title,
        description: row.get(4)?,
        created_at_ms: row.get(5)?,
        updated_at_ms: row.get(6)?,
        has_thumbnail: row.get(7)?,
        tags,
    })
}

fn fetch_tags_for_web_link(conn: &Connection, web_link_id: &str) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.name, t.name_key
         FROM tags t
         JOIN web_link_tags wt ON wt.tag_id = t.id
         WHERE wt.web_link_id = ?1
         ORDER BY t.name_key ASC, t.id ASC",
    )?;
    let rows = stmt.query_map([web_link_id], |r| {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            name_key: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn fetch_tags_for_page(conn: &Connection, page_id: &str) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.name, t.name_key
         FROM tags t
         JOIN page_tags pt ON pt.tag_id = t.id
         WHERE pt.page_id = ?1
         ORDER BY t.name_key ASC, t.id ASC",
    )?;
    let rows = stmt.query_map([page_id], |r| {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            name_key: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn get_web_link_inner(conn: &Connection, id: &str) -> Result<WebLinkDetail, AppError> {
    let row: Option<(String, String, String, String, i64, i64, bool)> = conn
        .query_row(
            "SELECT url, normalized_url, title, description, created_at_ms, updated_at_ms,
                EXISTS(SELECT 1 FROM web_link_thumbnails WHERE web_link_id = w.id)
             FROM web_links w WHERE w.id = ?1 AND w.deleted_at_ms IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;

    let Some((url, normalized_url, title, description, created_at_ms, updated_at_ms, has_thumbnail)) = row else {
        return Err(AppError::NotFound("指定されたWebリンクが見つかりません".into()));
    };

    let host = host_from_normalized_url(&normalized_url);
    let display_title = display_web_link_title(&title, &host);
    let tags = fetch_tags_for_web_link(conn, id)?;

    let mut page_stmt = conn.prepare(
        "SELECT id, web_link_id, title, position, revision, updated_at_ms
         FROM pages
         WHERE web_link_id = ?1 AND deleted_at_ms IS NULL
         ORDER BY position ASC, id ASC",
    )?;
    let pages = page_stmt
        .query_map([id], |r| {
            let p_title: String = r.get(2)?;
            let disp = display_page_title(&p_title);
            Ok(PageSummary {
                id: r.get(0)?,
                web_link_id: r.get(1)?,
                title: p_title,
                display_title: disp,
                position: r.get(3)?,
                revision: r.get(4)?,
                updated_at_ms: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(WebLinkDetail {
        id: id.to_string(),
        url,
        normalized_url,
        title,
        display_title,
        description,
        created_at_ms,
        updated_at_ms,
        has_thumbnail,
        tags,
        pages,
    })
}

fn get_page_inner(conn: &Connection, id: &str) -> Result<PageView, AppError> {
    let row: Option<(
        Option<String>,
        Option<String>,
        String,
        String,
        i64,
        i64,
        i64,
        Option<String>,
        Option<String>,
    )> = conn
        .query_row(
            "SELECT p.web_link_id, p.diary_date, p.title, p.body_markdown, p.revision,
                    p.created_at_ms, p.updated_at_ms,
                    w.title, w.normalized_url
             FROM pages p
             LEFT JOIN web_links w ON w.id = p.web_link_id
             WHERE p.id = ?1 AND p.deleted_at_ms IS NULL
               AND w.deleted_at_ms IS NULL",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .optional()?;

    let Some((
        web_link_id,
        diary_date,
        title,
        body_markdown,
        revision,
        created_at_ms,
        updated_at_ms,
        parent_title,
        parent_norm_url,
    )) = row
    else {
        return Err(AppError::NotFound("指定されたページが見つかりません".into()));
    };

    let kind = match (&diary_date, &web_link_id) {
        (Some(_), _) => PageKind::Diary,
        (_, Some(_)) => PageKind::AttachedPage,
        _ => PageKind::KnowledgePage,
    };
    let display_title = match &diary_date {
        Some(date) => display_diary_title(date),
        None => display_page_title(&title),
    };
    let parent = web_link_id.map(|id| {
        let host = parent_norm_url.as_deref().map(host_from_normalized_url).unwrap_or_default();
        ParentRef {
            id,
            title: display_web_link_title(parent_title.as_deref().unwrap_or(""), &host),
        }
    });

    Ok(PageView {
        id: id.to_string(),
        kind,
        title,
        display_title,
        body_markdown,
        revision,
        created_at_ms,
        updated_at_ms,
        tags: fetch_tags_for_page(conn, id)?,
        parent,
        diary_date,
    })
}

// ============================================================
// Diary
// ============================================================

pub fn get_diary(db: &DbManager, date: &str) -> Result<Option<PageView>, AppError> {
    validate_diary_date(date)?;
    db.with_reader(|conn| {
        let page_id: Option<String> = conn
            .query_row(
                "SELECT id FROM pages WHERE diary_date = ?1 AND deleted_at_ms IS NULL",
                [date],
                |r| r.get(0),
            )
            .optional()?;
        match page_id {
            Some(id) => Ok(Some(get_page_inner(conn, &id)?)),
            None => Ok(None),
        }
    })
}

pub fn save_diary(
    db: &DbManager,
    date: &str,
    markdown: &str,
    expected_revision: Option<i64>,
) -> Result<Option<SaveReceipt>, AppError> {
    validate_diary_date(date)?;
    let body = normalize_line_endings(markdown);

    db.with_transaction(|tx| {
        let existing: Option<(String, String, i64, i64)> = tx
            .query_row(
                "SELECT id, body_markdown, revision, updated_at_ms
                 FROM pages
                 WHERE diary_date = ?1 AND deleted_at_ms IS NULL",
                [date],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        match existing {
            None => {
                if expected_revision.is_some() {
                    return Err(AppError::RevisionConflict {
                        current_revision: None,
                        current_updated_at_ms: None,
                    });
                }
                if body.trim().is_empty() {
                    return Ok(None);
                }

                let id = Uuid::now_v7().to_string();
                let now = now_utc_ms();
                tx.execute(
                    "INSERT INTO pages (
                        id, web_link_id, diary_date, title, body_markdown,
                        position, revision, created_at_ms, updated_at_ms, deleted_at_ms
                     ) VALUES (?1, NULL, ?2, '', ?3, 0, 1, ?4, ?4, NULL)",
                    params![id, date, body, now],
                )?;
                sync_page_search_doc(tx, &id)?;
                Ok(Some(SaveReceipt {
                    page_id: id,
                    revision: 1,
                    updated_at_ms: now,
                }))
            }
            Some((id, current_body, current_rev, current_updated_at_ms)) => {
                if current_body == body {
                    return Ok(Some(SaveReceipt {
                        page_id: id,
                        revision: current_rev,
                        updated_at_ms: current_updated_at_ms,
                    }));
                }
                if expected_revision != Some(current_rev) {
                    return Err(AppError::RevisionConflict {
                        current_revision: Some(current_rev),
                        current_updated_at_ms: Some(current_updated_at_ms),
                    });
                }

                let new_rev = current_rev + 1;
                let now = now_utc_ms();
                tx.execute(
                    "UPDATE pages
                     SET body_markdown = ?1, revision = ?2, updated_at_ms = ?3
                     WHERE id = ?4",
                    params![body, new_rev, now, id],
                )?;
                sync_page_search_doc(tx, &id)?;

                Ok(Some(SaveReceipt {
                    page_id: id,
                    revision: new_rev,
                    updated_at_ms: now,
                }))
            }
        }
    })
}

pub fn list_diary_dates(db: &DbManager, year: i32, month: u32) -> Result<Vec<String>, AppError> {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return Err(AppError::Validation("年月が不正です".into()));
    }
    let first = format!("{year:04}-{month:02}-01");
    let last = format!("{year:04}-{month:02}-31");

    db.with_reader(|conn| {
        let mut stmt = conn.prepare(
            "SELECT diary_date, body_markdown
             FROM pages
             WHERE diary_date BETWEEN ?1 AND ?2 AND deleted_at_ms IS NULL
             ORDER BY diary_date ASC",
        )?;
        let rows = stmt.query_map([first, last], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut dates = Vec::new();
        for row in rows {
            let (date, body) = row?;
            if !body.trim().is_empty() {
                dates.push(date);
            }
        }
        Ok(dates)
    })
}

// ============================================================
// WebLinks
// ============================================================

fn query_duplicate_web_links(
    conn: &Connection,
    normalized_url: &str,
    exclude_id: Option<&str>,
) -> Result<Vec<WebLinkSummary>, AppError> {
    let sql = format!("SELECT {WEB_LINK_COLUMNS} FROM web_links r
        WHERE r.normalized_url = ?1 AND r.deleted_at_ms IS NULL AND (?2 IS NULL OR r.id <> ?2)
        ORDER BY r.updated_at_ms DESC, r.id ASC");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params![normalized_url, exclude_id])?;
    let mut results = Vec::new();
    while let Some(row) = rows.next()? {
        results.push(read_web_link_summary(conn, row)?);
    }
    Ok(results)
}

pub fn find_web_link_duplicates(
    db: &DbManager,
    url: &str,
    exclude_id: Option<&str>,
) -> Result<Vec<WebLinkSummary>, AppError> {
    let norm = validate_and_normalize_url(url)?;
    db.with_reader(|conn| query_duplicate_web_links(conn, &norm.normalized_url, exclude_id))
}

pub fn create_web_link(
    db: &DbManager,
    input: CreateWebLinkInput,
    allow_duplicate: bool,
) -> Result<WebLinkDetail, AppError> {
    let norm = validate_and_normalize_url(&input.url)?;
    let title = input.title.trim().to_string();
    let description = normalize_line_endings(input.description.trim());
    let thumbnail = input.thumbnail.as_deref().map(decode_thumbnail).transpose()?;

    db.with_transaction(|tx| {
        if !allow_duplicate {
            let dups = query_duplicate_web_links(tx, &norm.normalized_url, None)?;
            if !dups.is_empty() {
                return Err(AppError::DuplicateUrl { candidates: dups });
            }
        }

        let id = Uuid::now_v7().to_string();
        let now = now_utc_ms();
        tx.execute(
            "INSERT INTO web_links (
                id, url, normalized_url, title, description,
                created_at_ms, updated_at_ms, deleted_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL)",
            params![
                id,
                norm.display_url,
                norm.normalized_url,
                title,
                description,
                now
            ],
        )?;
        if let Some(jpeg) = thumbnail {
            tx.execute("INSERT INTO web_link_thumbnails (web_link_id, jpeg) VALUES (?1, ?2)", params![id, jpeg])?;
        }
        sync_web_link_search_doc(tx, &id)?;
        get_web_link_inner(tx, &id)
    })
}

pub fn update_web_link(
    db: &DbManager,
    id: &str,
    patch: UpdateWebLinkPatch,
    allow_duplicate: bool,
) -> Result<WebLinkDetail, AppError> {
    let parsed_url = match patch.url {
        Some(raw) => Some(validate_and_normalize_url(&raw)?),
        None => None,
    };
    let thumbnail = patch.thumbnail.as_deref().map(decode_thumbnail).transpose()?;

    db.with_transaction(|tx| {
        let existing: Option<(String, String, String, String)> = tx
            .query_row(
                "SELECT url, normalized_url, title, description
                 FROM web_links
                 WHERE id = ?1 AND deleted_at_ms IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();

        let Some((old_url, old_norm_url, old_title, old_desc)) = existing else {
            return Err(AppError::NotFound("指定されたWebリンクが見つかりません".into()));
        };

        let (new_url, new_norm_url) = match &parsed_url {
            Some(u) => (u.display_url.clone(), u.normalized_url.clone()),
            None => (old_url.clone(), old_norm_url.clone()),
        };

        let norm_changed = new_norm_url != old_norm_url;
        if norm_changed && !allow_duplicate {
            let dups = query_duplicate_web_links(tx, &new_norm_url, Some(id))?;
            if !dups.is_empty() {
                return Err(AppError::DuplicateUrl { candidates: dups });
            }
        }

        let new_title = match patch.title {
            Some(t) => t.trim().to_string(),
            None => old_title.clone(),
        };
        let new_desc = match patch.description {
            Some(d) => normalize_line_endings(d.trim()),
            None => old_desc.clone(),
        };

        let thumbnail_changed = match thumbnail {
            Some(jpeg) => tx.execute(
                "INSERT INTO web_link_thumbnails (web_link_id, jpeg) VALUES (?1, ?2)
                 ON CONFLICT(web_link_id) DO UPDATE SET jpeg = excluded.jpeg WHERE jpeg <> excluded.jpeg",
                params![id, jpeg],
            )? > 0,
            None if norm_changed => tx.execute("DELETE FROM web_link_thumbnails WHERE web_link_id = ?1", [id])? > 0,
            None => false,
        };
        let any_changed = new_url != old_url
            || new_norm_url != old_norm_url
            || new_title != old_title
            || new_desc != old_desc
            || thumbnail_changed;

        if any_changed {
            let now = now_utc_ms();
            tx.execute(
                "UPDATE web_links
                 SET url = ?1, normalized_url = ?2, title = ?3, description = ?4, updated_at_ms = ?5
                 WHERE id = ?6",
                params![new_url, new_norm_url, new_title, new_desc, now, id],
            )?;
            sync_web_link_search_doc(tx, id)?;

            let old_host = host_from_normalized_url(&old_norm_url);
            let new_host = host_from_normalized_url(&new_norm_url);
            if display_web_link_title(&old_title, &old_host)
                != display_web_link_title(&new_title, &new_host)
            {
                sync_children_search_docs(tx, id)?;
            }
        }

        get_web_link_inner(tx, id)
    })
}

pub fn list_web_links(
    db: &DbManager,
    cursor: Option<&str>,
    sort: CollectionSort,
) -> Result<ResultPage<WebLinkSummary>, AppError> {
    db.with_reader(|conn| {
        collection::list(conn, collection::Collection::WebLinks, WEB_LINK_COLUMNS, sort, cursor,
            |row| read_web_link_summary(conn, row))
    })
}

pub fn get_web_link(db: &DbManager, id: &str) -> Result<WebLinkDetail, AppError> {
    db.with_reader(|conn| get_web_link_inner(conn, id))
}

pub fn get_web_link_thumbnail(db: &DbManager, id: &str) -> Result<Option<String>, AppError> {
    db.with_reader(|conn| {
        let image: Option<Vec<u8>> = conn.prepare_cached(
            "SELECT thumbnail.jpeg FROM web_link_thumbnails thumbnail
             JOIN web_links w ON w.id = thumbnail.web_link_id WHERE w.id = ?1 AND w.deleted_at_ms IS NULL",
        )?.query_row([id], |row| row.get(0)).optional()?;
        Ok(image.as_deref().map(thumbnail_data_url))
    })
}

pub fn get_web_link_preview(db: &DbManager, id: &str) -> Result<WebLinkPreview, AppError> {
    db.with_reader(|conn| {
        conn.prepare_cached(
            "SELECT w.url, w.description,
                    (SELECT COUNT(*) FROM pages p WHERE p.web_link_id = w.id AND p.deleted_at_ms IS NULL)
             FROM web_links w WHERE w.id = ?1 AND w.deleted_at_ms IS NULL",
        )?
        .query_row([id], |row| {
            Ok(WebLinkPreview {
                url: row.get(0)?,
                description: row.get(1)?,
                page_count: row.get(2)?,
            })
        })
        .optional()?
        .ok_or_else(|| AppError::NotFound("指定されたWebリンクが見つかりません".into()))
    })
}

pub fn open_external_url(url: &str) -> Result<(), AppError> {
    let norm = validate_and_normalize_url(url)?;
    open::that(&norm.normalized_url)
        .map_err(|e| AppError::StorageError(format!("既定ブラウザでURLを開けません: {e}")))
}

// ============================================================
// Attached & Knowledge Pages
// ============================================================

pub fn create_page(db: &DbManager, web_link_id: &str, title: &str) -> Result<PageView, AppError> {
    let clean_title = title.trim().to_string();

    db.with_transaction(|tx| {
        let parent_exists: bool = tx
            .query_row(
                "SELECT count(*) FROM web_links WHERE id = ?1 AND deleted_at_ms IS NULL",
                [web_link_id],
                |r| r.get::<_, i64>(0),
            )?
            > 0;

        if !parent_exists {
            return Err(AppError::NotFound("親Webリンクが見つかりません".into()));
        }

        let next_pos: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM pages WHERE web_link_id = ?1",
            [web_link_id],
            |r| r.get(0),
        )?;

        let id = Uuid::now_v7().to_string();
        let now = now_utc_ms();

        tx.execute(
            "INSERT INTO pages (
                id, web_link_id, diary_date, title, body_markdown,
                position, revision, created_at_ms, updated_at_ms, deleted_at_ms
             ) VALUES (?1, ?2, NULL, ?3, '', ?4, 1, ?5, ?5, NULL)",
            params![id, web_link_id, clean_title, next_pos, now],
        )?;
        sync_page_search_doc(tx, &id)?;
        get_page_inner(tx, &id)
    })
}

pub fn create_knowledge_page(
    db: &DbManager,
    title: &str,
    body_markdown: Option<&str>,
) -> Result<PageView, AppError> {
    let clean_title = title.trim().to_string();
    let body = normalize_line_endings(body_markdown.unwrap_or(""));

    db.with_transaction(|tx| {
        let id = Uuid::now_v7().to_string();
        let now = now_utc_ms();

        tx.execute(
            "INSERT INTO pages (
                id, web_link_id, diary_date, title, body_markdown,
                position, revision, created_at_ms, updated_at_ms, deleted_at_ms
             ) VALUES (?1, NULL, NULL, ?2, ?3, 0, 1, ?4, ?4, NULL)",
            params![id, clean_title, body, now],
        )?;
        sync_page_search_doc(tx, &id)?;
        get_page_inner(tx, &id)
    })
}

pub fn list_knowledge_pages(
    db: &DbManager,
    cursor: Option<&str>,
    sort: CollectionSort,
) -> Result<ResultPage<KnowledgePageSummary>, AppError> {
    db.with_reader(|conn| {
        collection::list(conn, collection::Collection::Knowledge,
            "r.id, r.title, r.revision, r.created_at_ms, r.updated_at_ms, substr(r.body_markdown, 1, 500)",
            sort, cursor, |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let markdown: String = row.get(5)?;
                Ok(KnowledgePageSummary {
                    tags: fetch_tags_for_page(conn, &id)?,
                    id,
                    display_title: display_page_title(&title),
                    title,
                    revision: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                    excerpt: extract_search_text(&markdown).chars().take(260).collect(),
                })
            })
    })
}

pub fn get_page(db: &DbManager, id: &str) -> Result<PageView, AppError> {
    db.with_reader(|conn| get_page_inner(conn, id))
}

fn get_page_markdown(db: &DbManager, id: &str) -> Result<String, AppError> {
    db.with_reader(|conn| {
        conn.prepare_cached(
            "SELECT p.body_markdown
             FROM pages p LEFT JOIN web_links w ON w.id = p.web_link_id
             WHERE p.id = ?1 AND p.deleted_at_ms IS NULL AND w.deleted_at_ms IS NULL",
        )?
        .query_row([id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::NotFound("指定されたページが見つかりません".into()))
    })
}

pub fn get_page_preview(db: &DbManager, id: &str) -> Result<Option<String>, AppError> {
    let markdown = get_page_markdown(db, id)?;
    Ok((!markdown.trim().is_empty()).then(|| render_markdown_preview(&markdown)))
}

pub fn save_page(
    db: &DbManager,
    id: &str,
    markdown: &str,
    expected_revision: i64,
) -> Result<SaveReceipt, AppError> {
    let body = normalize_line_endings(markdown);

    db.with_transaction(|tx| {
        let row: Option<(Option<String>, String, i64, i64, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT p.web_link_id, p.body_markdown, p.revision, p.updated_at_ms, p.deleted_at_ms, w.deleted_at_ms
                 FROM pages p
                 LEFT JOIN web_links w ON w.id = p.web_link_id
                 WHERE p.id = ?1 AND p.diary_date IS NULL",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;

        let Some((web_link_id, current_body, current_rev, current_updated_at_ms, p_del, w_del)) =
            row
        else {
            return Err(AppError::NotFound("指定されたページが見つかりません".into()));
        };
        if p_del.is_some() || (web_link_id.is_some() && w_del.is_some()) {
            return Err(AppError::NotFound("このページは削除されています".into()));
        }

        if current_body == body {
            return Ok(SaveReceipt {
                page_id: id.to_string(),
                revision: current_rev,
                updated_at_ms: current_updated_at_ms,
            });
        }
        if expected_revision != current_rev {
            return Err(AppError::RevisionConflict {
                current_revision: Some(current_rev),
                current_updated_at_ms: Some(current_updated_at_ms),
            });
        }

        let new_rev = current_rev + 1;
        let now = now_utc_ms();
        tx.execute(
            "UPDATE pages
             SET body_markdown = ?1, revision = ?2, updated_at_ms = ?3
             WHERE id = ?4",
            params![body, new_rev, now, id],
        )?;
        sync_page_search_doc(tx, id)?;

        Ok(SaveReceipt {
            page_id: id.to_string(),
            revision: new_rev,
            updated_at_ms: now,
        })
    })
}

pub fn rename_page(db: &DbManager, id: &str, title: &str) -> Result<PageView, AppError> {
    let clean_title = title.trim().to_string();

    db.with_transaction(|tx| {
        let row: Option<(Option<String>, String, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT p.web_link_id, p.title, p.deleted_at_ms, w.deleted_at_ms
                 FROM pages p
                 LEFT JOIN web_links w ON w.id = p.web_link_id
                 WHERE p.id = ?1 AND p.diary_date IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();

        let Some((web_link_id, old_title, p_del, w_del)) = row else {
            return Err(AppError::NotFound("指定されたページが見つかりません".into()));
        };
        if p_del.is_some() || (web_link_id.is_some() && w_del.is_some()) {
            return Err(AppError::NotFound("このページは削除されています".into()));
        }

        if clean_title != old_title {
            let now = now_utc_ms();
            tx.execute(
                "UPDATE pages SET title = ?1, updated_at_ms = ?2 WHERE id = ?3",
                params![clean_title, now, id],
            )?;
            sync_page_search_doc(tx, id)?;
        }

        get_page_inner(tx, id)
    })
}

pub fn reorder_pages(
    db: &DbManager,
    web_link_id: &str,
    page_ids: Vec<String>,
) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let parent_active: bool = tx
            .query_row(
                "SELECT count(*) FROM web_links WHERE id = ?1 AND deleted_at_ms IS NULL",
                [web_link_id],
                |r| r.get::<_, i64>(0),
            )?
            > 0;
        if !parent_active {
            return Err(AppError::NotFound("親Webリンクが見つかりません".into()));
        }

        let active_ids: HashSet<String> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM pages WHERE web_link_id = ?1 AND deleted_at_ms IS NULL",
            )?;
            let rows = stmt.query_map([web_link_id], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };

        let provided_set: HashSet<&str> = page_ids.iter().map(String::as_str).collect();
        if provided_set.len() != page_ids.len()
            || active_ids.len() != page_ids.len()
            || !page_ids.iter().all(|id| active_ids.contains(id))
        {
            return Err(AppError::Validation(
                "並べ替え対象には親Webリンクに属する有効な全ページIDを重複なく指定してください"
                    .into(),
            ));
        }

        for (idx, page_id) in page_ids.iter().enumerate() {
            tx.execute(
                "UPDATE pages SET position = ?1 WHERE id = ?2",
                params![idx as i64, page_id],
            )?;
        }

        Ok(())
    })
}

// ============================================================
// Tags
// ============================================================

pub fn list_tags(db: &DbManager, query: Option<&str>) -> Result<Vec<TagSummary>, AppError> {
    let prefix = query.map(normalize_search_key).unwrap_or_default();

    db.with_reader(|conn| {
        let base_select = "
            SELECT t.id, t.name, t.name_key,
                (
                    (SELECT count(*) FROM web_link_tags wt
                     JOIN web_links w ON w.id = wt.web_link_id
                     WHERE wt.tag_id = t.id AND w.deleted_at_ms IS NULL)
                    +
                    (SELECT count(*) FROM page_tags pt
                     JOIN pages p ON p.id = pt.page_id
                     LEFT JOIN web_links w ON w.id = p.web_link_id
                     WHERE pt.tag_id = t.id
                       AND p.deleted_at_ms IS NULL
                       AND (p.web_link_id IS NULL OR w.deleted_at_ms IS NULL))
                ) AS cnt
            FROM tags t";

        let mut tags = Vec::new();
        if prefix.is_empty() {
            let sql = format!("{base_select} ORDER BY t.name_key ASC, t.id ASC");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |r| {
                Ok(TagSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    name_key: r.get(2)?,
                    count: r.get(3)?,
                })
            })?;
            for r in rows {
                tags.push(r?);
            }
        } else if let Some(upper) = prefix_upper_bound(&prefix) {
            let sql = format!(
                "{base_select} WHERE t.name_key >= ?1 AND t.name_key < ?2 ORDER BY t.name_key ASC, t.id ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![prefix, upper], |r| {
                Ok(TagSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    name_key: r.get(2)?,
                    count: r.get(3)?,
                })
            })?;
            for r in rows {
                tags.push(r?);
            }
        } else {
            let sql = format!(
                "{base_select} WHERE t.name_key >= ?1 ORDER BY t.name_key ASC, t.id ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([prefix], |r| {
                Ok(TagSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    name_key: r.get(2)?,
                    count: r.get(3)?,
                })
            })?;
            for r in rows {
                tags.push(r?);
            }
        }

        Ok(tags)
    })
}

pub fn create_tag(db: &DbManager, name: &str) -> Result<Tag, AppError> {
    let trimmed = name.trim();
    let key = normalize_search_key(trimmed);
    if key.is_empty() {
        return Err(AppError::Validation("タグ名を入力してください".into()));
    }

    db.with_transaction(|tx| {
        let existing: Option<Tag> = tx
            .query_row(
                "SELECT id, name, name_key FROM tags WHERE name_key = ?1",
                [&key],
                |r| {
                    Ok(Tag {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        name_key: r.get(2)?,
                    })
                },
            )
            .ok();

        if let Some(tag) = existing {
            return Ok(tag);
        }

        let id = Uuid::now_v7().to_string();
        tx.execute(
            "INSERT INTO tags (id, name, name_key) VALUES (?1, ?2, ?3)",
            params![id, trimmed, key],
        )?;
        Ok(Tag {
            id,
            name: trimmed.to_string(),
            name_key: key,
        })
    })
}

pub fn rename_tag(db: &DbManager, id: &str, name: &str) -> Result<Tag, AppError> {
    let trimmed = name.trim();
    let key = normalize_search_key(trimmed);
    if key.is_empty() {
        return Err(AppError::Validation("タグ名を入力してください".into()));
    }

    db.with_transaction(|tx| {
        let exists: bool = tx
            .query_row("SELECT count(*) FROM tags WHERE id = ?1", [id], |r| {
                r.get::<_, i64>(0)
            })?
            > 0;
        if !exists {
            return Err(AppError::NotFound("指定されたタグが見つかりません".into()));
        }

        let conflict: bool = tx
            .query_row(
                "SELECT count(*) FROM tags WHERE name_key = ?1 AND id <> ?2",
                params![key, id],
                |r| r.get::<_, i64>(0),
            )?
            > 0;
        if conflict {
            return Err(AppError::Validation("同じ名前のタグが既に存在します".into()));
        }

        tx.execute(
            "UPDATE tags SET name = ?1, name_key = ?2 WHERE id = ?3",
            params![trimmed, key, id],
        )?;
        Ok(Tag {
            id: id.to_string(),
            name: trimmed.to_string(),
            name_key: key,
        })
    })
}

pub fn delete_tag(db: &DbManager, id: &str) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let deleted = tx.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(AppError::NotFound("指定されたタグが見つかりません".into()));
        }
        Ok(())
    })
}

fn dedup_and_validate_tag_ids(
    conn: &Connection,
    tag_ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for tid in tag_ids {
        if seen.insert(tid.clone()) {
            deduped.push(tid);
        }
    }

    for tid in &deduped {
        let exists: bool = conn
            .query_row("SELECT count(*) FROM tags WHERE id = ?1", [tid], |r| {
                r.get::<_, i64>(0)
            })?
            > 0;
        if !exists {
            return Err(AppError::NotFound(format!("タグが見つかりません: {tid}")));
        }
    }

    Ok(deduped)
}

pub fn set_web_link_tags(
    db: &DbManager,
    id: &str,
    tag_ids: Vec<String>,
) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let active: bool = tx
            .query_row(
                "SELECT count(*) FROM web_links WHERE id = ?1 AND deleted_at_ms IS NULL",
                [id],
                |r| r.get::<_, i64>(0),
            )?
            > 0;
        if !active {
            return Err(AppError::NotFound("指定されたWebリンクが見つかりません".into()));
        }

        let new_ids = dedup_and_validate_tag_ids(tx, tag_ids)?;
        let old_ids: HashSet<String> = {
            let mut stmt =
                tx.prepare("SELECT tag_id FROM web_link_tags WHERE web_link_id = ?1")?;
            let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        let new_set: HashSet<String> = new_ids.iter().cloned().collect();

        if old_ids != new_set {
            tx.execute("DELETE FROM web_link_tags WHERE web_link_id = ?1", [id])?;
            for tid in &new_ids {
                tx.execute(
                    "INSERT INTO web_link_tags (web_link_id, tag_id) VALUES (?1, ?2)",
                    params![id, tid],
                )?;
            }

            let now = now_utc_ms();
            tx.execute(
                "UPDATE web_links SET updated_at_ms = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            tx.execute(
                "UPDATE search_documents SET updated_at_ms = ?1
                 WHERE kind = 'web_link' AND target_id = ?2",
                params![now, id],
            )?;
        }

        Ok(())
    })
}

pub fn set_page_tags(db: &DbManager, id: &str, tag_ids: Vec<String>) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let row: Option<(Option<String>, Option<String>, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT p.web_link_id, p.diary_date, p.deleted_at_ms, w.deleted_at_ms
                 FROM pages p
                 LEFT JOIN web_links w ON w.id = p.web_link_id
                 WHERE p.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();

        let Some((web_link_id, diary_date, p_del, w_del)) = row else {
            return Err(AppError::NotFound("指定されたページが見つかりません".into()));
        };
        if p_del.is_some() || (web_link_id.is_some() && w_del.is_some()) {
            return Err(AppError::NotFound("指定されたページは削除されています".into()));
        }
        let kind_str = if diary_date.is_some() {
            "diary"
        } else if web_link_id.is_some() {
            "attached_page"
        } else {
            "knowledge_page"
        };

        let new_ids = dedup_and_validate_tag_ids(tx, tag_ids)?;
        let old_ids: HashSet<String> = {
            let mut stmt = tx.prepare("SELECT tag_id FROM page_tags WHERE page_id = ?1")?;
            let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        let new_set: HashSet<String> = new_ids.iter().cloned().collect();

        if old_ids != new_set {
            tx.execute("DELETE FROM page_tags WHERE page_id = ?1", [id])?;
            for tid in &new_ids {
                tx.execute(
                    "INSERT INTO page_tags (page_id, tag_id) VALUES (?1, ?2)",
                    params![id, tid],
                )?;
            }

            let now = now_utc_ms();
            tx.execute(
                "UPDATE pages SET updated_at_ms = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            tx.execute(
                "UPDATE search_documents SET updated_at_ms = ?1
                 WHERE kind = ?2 AND target_id = ?3",
                params![now, kind_str, id],
            )?;
        }

        Ok(())
    })
}

// ============================================================
// Trash
// ============================================================

pub fn soft_delete_web_link(db: &DbManager, id: &str) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let now = now_utc_ms();
        let affected = tx.execute(
            "UPDATE web_links SET deleted_at_ms = ?1 WHERE id = ?2 AND deleted_at_ms IS NULL",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound("指定されたWebリンクが見つかりません".into()));
        }

        sync_web_link_search_doc(tx, id)?;
        sync_children_search_docs(tx, id)?;
        Ok(())
    })
}

pub fn restore_web_link(db: &DbManager, id: &str) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let affected = tx.execute(
            "UPDATE web_links SET deleted_at_ms = NULL WHERE id = ?1 AND deleted_at_ms IS NOT NULL",
            [id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(
                "ごみ箱に指定されたWebリンクが見つかりません".into(),
            ));
        }

        sync_web_link_search_doc(tx, id)?;
        sync_children_search_docs(tx, id)?;
        Ok(())
    })
}

pub fn soft_delete_page(db: &DbManager, id: &str) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let now = now_utc_ms();
        let affected = tx.execute(
            "UPDATE pages SET deleted_at_ms = ?1
             WHERE id = ?2 AND diary_date IS NULL AND deleted_at_ms IS NULL",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound("指定されたページが見つかりません".into()));
        }

        sync_page_search_doc(tx, id)?;
        Ok(())
    })
}

pub fn restore_page(db: &DbManager, id: &str) -> Result<(), AppError> {
    db.with_transaction(|tx| {
        let row: Option<(Option<String>, Option<i64>, Option<i64>)> = tx
            .query_row(
                "SELECT p.web_link_id, p.deleted_at_ms, w.deleted_at_ms
                 FROM pages p
                 LEFT JOIN web_links w ON w.id = p.web_link_id
                 WHERE p.id = ?1 AND p.diary_date IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        let Some((web_link_id, p_del, w_del)) = row else {
            return Err(AppError::NotFound(
                "ごみ箱に指定されたページが見つかりません".into(),
            ));
        };
        if p_del.is_none() {
            return Err(AppError::Validation("このページは既に有効です".into()));
        }
        if let Some(wid) = web_link_id {
            if w_del.is_some() {
                return Err(AppError::Validation(
                    "親Webリンクがごみ箱にあるため、先に親Webリンクを復元してください".into(),
                ));
            }

            let next_pos: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM pages WHERE web_link_id = ?1 AND deleted_at_ms IS NULL",
                [&wid],
                |r| r.get(0),
            )?;

            tx.execute(
                "UPDATE pages SET deleted_at_ms = NULL, position = ?1 WHERE id = ?2",
                params![next_pos, id],
            )?;
        } else {
            tx.execute(
                "UPDATE pages SET deleted_at_ms = NULL, position = 0 WHERE id = ?1",
                [id],
            )?;
        }
        sync_page_search_doc(tx, id)?;
        Ok(())
    })
}

pub fn list_trash(db: &DbManager) -> Result<Vec<TrashEntry>, AppError> {
    db.with_reader(|conn| {
        let mut entries = Vec::new();

        {
            let mut stmt = conn.prepare(
                "SELECT id, title, url, normalized_url, deleted_at_ms
                 FROM web_links
                 WHERE deleted_at_ms IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| {
                let title: String = r.get(1)?;
                let url: String = r.get(2)?;
                let norm_url: String = r.get(3)?;
                let host = host_from_normalized_url(&norm_url);
                let disp = display_web_link_title(&title, &host);
                Ok(TrashEntry {
                    id: r.get(0)?,
                    kind: TrashKind::WebLink,
                    title: disp,
                    url: Some(url),
                    parent_id: None,
                    parent_title: None,
                    deleted_at_ms: r.get(4)?,
                })
            })?;
            for r in rows {
                entries.push(r?);
            }
        }

        {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.title, p.deleted_at_ms, w.id, w.title, w.normalized_url
                 FROM pages p
                 JOIN web_links w ON w.id = p.web_link_id
                 WHERE p.deleted_at_ms IS NOT NULL
                   AND p.web_link_id IS NOT NULL
                   AND w.deleted_at_ms IS NULL",
            )?;
            let rows = stmt.query_map([], |r| {
                let p_title: String = r.get(1)?;
                let w_title: String = r.get(4)?;
                let w_norm: String = r.get(5)?;
                let host = host_from_normalized_url(&w_norm);
                Ok(TrashEntry {
                    id: r.get(0)?,
                    kind: TrashKind::AttachedPage,
                    title: display_page_title(&p_title),
                    url: None,
                    parent_id: Some(r.get(3)?),
                    parent_title: Some(display_web_link_title(&w_title, &host)),
                    deleted_at_ms: r.get(2)?,
                })
            })?;
            for r in rows {
                entries.push(r?);
            }
        }

        {
            let mut stmt = conn.prepare(
                "SELECT id, title, deleted_at_ms
                 FROM pages
                 WHERE deleted_at_ms IS NOT NULL
                   AND web_link_id IS NULL
                   AND diary_date IS NULL",
            )?;
            let rows = stmt.query_map([], |r| {
                let p_title: String = r.get(1)?;
                Ok(TrashEntry {
                    id: r.get(0)?,
                    kind: TrashKind::KnowledgePage,
                    title: display_page_title(&p_title),
                    url: None,
                    parent_id: None,
                    parent_title: None,
                    deleted_at_ms: r.get(2)?,
                })
            })?;
            for r in rows {
                entries.push(r?);
            }
        }

        entries.sort_by(|a, b| {
            b.deleted_at_ms
                .cmp(&a.deleted_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(entries)
    })
}

// ============================================================
// Markdown Export (Spec 8.1, Chapter 9)
// ============================================================

pub fn export_page_markdown(
    db: &DbManager,
    id: &str,
    destination: &str,
) -> Result<(), AppError> {
    let trimmed = destination.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("書き出し先のファイルパスを指定してください".into()));
    }
    let dest_path = Path::new(trimmed);
    if !dest_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(AppError::Validation("拡張子 .md のファイルを指定してください".into()));
    }
    let content = normalize_line_endings(&get_page_markdown(db, id)?);
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(dest_path, content.as_bytes())?;
    Ok(())
}
