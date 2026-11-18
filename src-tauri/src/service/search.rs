use rusqlite::{types::Value, ToSql};
use serde::{Deserialize, Serialize};

use super::{decode_cursor, encode_cursor, host_from_normalized_url, PAGE_SIZE};
use crate::db::DbManager;
use crate::error::AppError;
use crate::models::{MatchedField, ParentRef, ResultPage, SearchHit, SearchKind, SearchSort};
use crate::normalize::{
    display_diary_title, display_page_title, display_web_link_title, normalize_search_key,
    prefix_upper_bound, scalar_len, to_fts5_phrase, try_normalize_url_for_search,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchCursor {
    priority: u8,
    updated_at_ms: i64,
    kind: String,
    target_id: String,
}

const SELECTED_TAGS: &str = "WITH selected_tags AS (SELECT value FROM json_each(:tag_ids))";
const ORDERING_KEYS: &str = "sd.id, sd.kind, sd.target_id, sd.updated_at_ms";
const TAG_FILTER: &str = "
    AND (
        (sd.kind = 'web_link' AND sd.target_id IN (
            SELECT wt.web_link_id FROM selected_tags st
            JOIN web_link_tags wt ON wt.tag_id = st.value
            GROUP BY wt.web_link_id HAVING COUNT(*) = :tag_count
        ))
        OR (sd.kind <> 'web_link' AND sd.target_id IN (
            SELECT pt.page_id FROM selected_tags st
            JOIN page_tags pt ON pt.tag_id = st.value
            GROUP BY pt.page_id HAVING COUNT(*) = :tag_count
        ))
    )";

pub fn search(
    db: &DbManager,
    query: &str,
    kind: Option<SearchKind>,
    tag_ids: Option<Vec<String>>,
    sort: Option<SearchSort>,
    cursor: Option<&str>,
) -> Result<ResultPage<SearchHit>, AppError> {
    let query_key = normalize_search_key(query);
    let mut tags = tag_ids.unwrap_or_default();
    tags.sort_unstable();
    tags.dedup();

    let mut bindings = vec![(":limit", Value::Integer((PAGE_SIZE + 1) as i64))];
    let mut sql = String::from("WITH");
    if !tags.is_empty() {
        sql = String::from(SELECTED_TAGS);
        sql.push(',');
        bindings.push((
            ":tag_ids",
            Value::Text(serde_json::to_string(&tags).map_err(|err| {
                AppError::StorageError(format!("タグ条件の変換に失敗しました: {err}"))
            })?),
        ));
        bindings.push((":tag_count", Value::Integer(tags.len() as i64)));
    }

    let kind_filter = if let Some(kind) = kind {
        bindings.push((":kind", Value::Text(kind.as_str().into())));
        "AND sd.kind = :kind"
    } else {
        ""
    };

    let (source, score, by_relevance) = if query_key.is_empty() {
        ("search_documents sd", "0", false)
    } else {
        let candidates = candidate_sql(query, query_key, kind, kind_filter, &mut bindings);
        sql.push_str(&format!(
            " candidates(id, kind, target_id, updated_at_ms, score) AS ({candidates}),
              ranked AS (
                  SELECT kind, target_id, updated_at_ms, MIN(score) AS score
                  FROM candidates WHERE score IS NOT NULL GROUP BY id
              ),"
        ));
        ("ranked sd", "sd.score", sort != Some(SearchSort::Updated))
    };
    let priority = if by_relevance { "sd.score / 10" } else { "0" };
    let priority_order = if by_relevance { "priority, " } else { "" };
    let tag_filter = if tags.is_empty() { "" } else { TAG_FILTER };
    let mut cursor_filter = String::new();
    if let Some(raw) = cursor.filter(|value| !value.trim().is_empty()) {
        let cursor: SearchCursor = decode_cursor(raw)?;
        bindings.extend([
            (":before", Value::Integer(cursor.updated_at_ms)),
            (":last_kind", Value::Text(cursor.kind)),
            (":last_id", Value::Text(cursor.target_id)),
        ]);
        let after = "(sd.updated_at_ms <= :before AND (
            sd.updated_at_ms < :before OR (sd.kind, sd.target_id) > (:last_kind, :last_id)
        ))";
        cursor_filter = if by_relevance {
            bindings.push((":last_priority", Value::Integer(i64::from(cursor.priority))));
            format!(
                "AND ({priority} > :last_priority OR ({priority} = :last_priority AND {after}))"
            )
        } else {
            format!("AND {after}")
        };
    }
    sql.push_str(&format!(
        " result AS (
              SELECT sd.kind, sd.target_id, sd.updated_at_ms, {score} AS score, {priority} AS priority
              FROM {source}
              WHERE 1 = 1 {kind_filter} {tag_filter} {cursor_filter}
              ORDER BY {priority_order}sd.updated_at_ms DESC, sd.kind, sd.target_id LIMIT :limit
          )
          SELECT r.kind, r.target_id, r.updated_at_ms, r.score, r.priority,
                 COALESCE(w.title, p.title), w.url, w.normalized_url,
                 p.diary_date, p.web_link_id, parent.title, parent.normalized_url
          FROM result r
          LEFT JOIN web_links w ON r.kind = 'web_link' AND w.id = r.target_id
          LEFT JOIN pages p ON r.kind <> 'web_link' AND p.id = r.target_id
          LEFT JOIN web_links parent ON parent.id = p.web_link_id
          ORDER BY {priority_order}r.updated_at_ms DESC, r.kind, r.target_id"
    ));
    let params: Vec<_> = bindings
        .iter()
        .map(|(name, value)| (*name, value as &dyn ToSql))
        .collect();

    db.with_search_reader(|conn| {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(params.as_slice())?;
        let mut items = Vec::with_capacity(PAGE_SIZE);
        let mut last_priority = 0;
        while let Some(row) = rows.next()? {
            if items.len() == PAGE_SIZE {
                let last: &SearchHit = items.last().unwrap();
                let next_cursor = Some(encode_cursor(&SearchCursor {
                    priority: last_priority,
                    updated_at_ms: last.updated_at_ms,
                    kind: last.kind.as_str().into(),
                    target_id: last.id.clone(),
                })?);
                return Ok(ResultPage { items, next_cursor });
            }

            let kind_name: String = row.get(0)?;
            let kind = SearchKind::from_str(&kind_name).ok_or_else(|| {
                AppError::StorageError("検索インデックスに不正な項目種別が含まれています".into())
            })?;
            let title: String = row.get(5)?;
            let diary_date: Option<String> = row.get(8)?;
            let parent_id: Option<String> = row.get(9)?;
            let parent = match parent_id {
                Some(id) => Some(ParentRef {
                    id,
                    title: link_title(
                        row.get_ref(10)?.as_str().unwrap_or(""),
                        row.get_ref(11)?.as_str().unwrap_or(""),
                    ),
                }),
                None => None,
            };
            let title = match kind {
                SearchKind::WebLink => link_title(&title, row.get_ref(7)?.as_str().unwrap_or("")),
                SearchKind::Diary => display_diary_title(diary_date.as_deref().unwrap_or("")),
                _ => display_page_title(&title),
            };
            let matched_field = match row.get::<_, i64>(3)? {
                0 => None,
                10 | 20 | 40 => Some(MatchedField::Title),
                11 | 21 | 50 => Some(MatchedField::Url),
                30 => Some(MatchedField::Tag),
                41 => Some(MatchedField::ParentTitle),
                60 => Some(MatchedField::Description),
                70 => Some(MatchedField::Body),
                _ => unreachable!("search scores are produced by the query"),
            };
            last_priority = row.get(4)?;
            items.push(SearchHit {
                id: row.get(1)?,
                kind,
                title,
                updated_at_ms: row.get(2)?,
                url: row.get(6)?,
                parent,
                diary_date,
                matched_field,
            });
        }
        Ok(ResultPage {
            items,
            next_cursor: None,
        })
    })
}

fn link_title(title: &str, normalized_url: &str) -> String {
    if title.trim().is_empty() {
        display_web_link_title(title, &host_from_normalized_url(normalized_url))
    } else {
        display_web_link_title(title, "")
    }
}

fn candidate_sql(
    query: &str,
    query_key: String,
    kind: Option<SearchKind>,
    kind_filter: &str,
    bindings: &mut Vec<(&'static str, Value)>,
) -> String {
    let upper = prefix_upper_bound(&query_key);
    let has_upper = upper.is_some();
    if let Some(upper) = upper {
        bindings.push((":upper", Value::Text(upper)));
    }
    let upper = |column: &str| {
        if has_upper {
            format!("AND {column} < :upper")
        } else {
            String::new()
        }
    };
    let include_links = kind.is_none() || kind == Some(SearchKind::WebLink);
    let include_pages = kind != Some(SearchKind::WebLink);
    let mut candidates = vec![format!(
        "SELECT {ORDERING_KEYS}, CASE WHEN sd.title_key = :query THEN 10 ELSE 20 END
         FROM search_documents sd
         WHERE sd.title_key >= :query {} {kind_filter}",
        upper("sd.title_key")
    )];
    // Scores reserve the units digit for tie-breaking between fields on the same document.
    if include_links {
        if let Some(url) = try_normalize_url_for_search(query) {
            bindings.push((":url", Value::Text(url)));
            candidates.push(format!(
                "SELECT {ORDERING_KEYS}, 11 FROM web_links w
                 CROSS JOIN search_documents sd ON sd.kind = 'web_link' AND sd.target_id = w.id
                 WHERE w.deleted_at_ms IS NULL AND w.normalized_url = :url"
            ));
        }
        candidates.push(format!(
            "SELECT {ORDERING_KEYS}, 21 FROM search_documents sd
             WHERE sd.kind = 'web_link' AND sd.host_key >= :query {}",
            upper("sd.host_key")
        ));
        candidates.push(format!(
            "SELECT {ORDERING_KEYS}, 30 FROM tags t
             CROSS JOIN web_link_tags wt ON wt.tag_id = t.id
             CROSS JOIN search_documents sd ON sd.kind = 'web_link' AND sd.target_id = wt.web_link_id
             WHERE t.name_key >= :query {}",
            upper("t.name_key")
        ));
    }
    if include_pages {
        candidates.push(format!(
            "SELECT {ORDERING_KEYS}, 30 FROM tags t
             CROSS JOIN page_tags pt ON pt.tag_id = t.id
             CROSS JOIN search_documents sd
               ON sd.kind IN ('attached_page', 'knowledge_page', 'diary') AND sd.target_id = pt.page_id
             WHERE t.name_key >= :query {} {kind_filter}",
            upper("t.name_key")
        ));
    }
    if scalar_len(&query_key) >= 3 {
        // A literal case-sensitive trigram phrase must occur in one indexed column.
        // Only queries containing a NUL need a final text check after FTS tokenization.
        let body_match = if query_key.contains('\0') {
            "WHEN instr(sd.body_key, :query) > 0 THEN 70"
        } else {
            "ELSE 70"
        };
        bindings.push((":phrase", Value::Text(to_fts5_phrase(&query_key))));
        candidates.push(format!(
            "SELECT {ORDERING_KEYS}, CASE
                 WHEN instr(sd.title_key, :query) > 0 THEN 40
                 WHEN instr(sd.parent_title_key, :query) > 0 THEN 41
                 WHEN instr(sd.url_text_key, :query) > 0 THEN 50
                 WHEN instr(sd.description_key, :query) > 0 THEN 60
                 {body_match}
             END
             FROM search_fts f
             CROSS JOIN search_documents sd INDEXED BY idx_search_match ON sd.id = f.rowid
             WHERE search_fts MATCH :phrase {kind_filter}"
        ));
    }
    bindings.push((":query", Value::Text(query_key)));
    candidates.join(" UNION ALL ")
}
