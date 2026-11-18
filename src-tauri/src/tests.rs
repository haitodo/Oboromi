use std::collections::HashSet;
use std::fs;
use std::time::Instant;

use tempfile::TempDir;

use crate::db::{DbManager, OBOROMI_APPLICATION_ID, OBOROMI_USER_VERSION};
use crate::error::AppError;
use crate::markdown::{extract_search_text, render_markdown_preview};
use crate::models::{
    CollectionSort, CreateWebLinkInput, MatchedField, PageKind, SearchKind, SearchSort, TrashKind,
    UpdateWebLinkPatch,
};
use crate::normalize::{normalize_search_key, prefix_upper_bound, validate_and_normalize_url};
use crate::service;

fn open_temp_db() -> (TempDir, DbManager) {
    let dir = TempDir::new().unwrap();
    let db = DbManager::open(dir.path().to_path_buf(), false).unwrap();
    assert!(db.status().startup_error.is_none());
    (dir, db)
}

#[test]
fn test_normalization_and_prefix_bounds() {
    assert_eq!(normalize_search_key("  Ｒｕｓｔ\u{3000}ＭＤ  "), "rust md");
    assert_eq!(normalize_search_key("ＡＢＣ　１２３"), "abc 123");
    assert_eq!(prefix_upper_bound("ab"), Some("ac".to_string()));
    assert_eq!(prefix_upper_bound("a\u{10FFFF}"), Some("b".to_string()));
    assert_eq!(prefix_upper_bound("\u{10FFFF}"), None);
    assert_eq!(prefix_upper_bound("a\u{D7FF}"), Some("a\u{E000}".to_string()));

    let u1 = validate_and_normalize_url("  https://EXAMPLE.com:443  ").unwrap();
    let u2 = validate_and_normalize_url("https://example.com/").unwrap();
    assert_eq!(u1.normalized_url, u2.normalized_url);
    assert_eq!(u1.host, "example.com");

    assert!(validate_and_normalize_url("javascript:alert(1)").is_err());
    assert!(validate_and_normalize_url("https://example.com/\nfoo").is_err());
}

#[test]
fn test_collection_sort_orders_before_paginating_with_equal_keys() {
    let (_dir, db) = open_temp_db();
    for index in 0..35 {
        let title = if index == 0 { "Ｚｅｂｒａ" } else if index == 34 { "apple" } else { "同じタイトル" };
        service::create_web_link(&db, CreateWebLinkInput {
            url: format!("https://collection.example/{index}"),
            title: title.into(), description: String::new(), thumbnail: None,
        }, false).unwrap();
        service::create_knowledge_page(&db, title, Some("# 本文の抜粋")).unwrap();
    }

    let first = service::list_web_links(&db, None, CollectionSort::TitleAsc).unwrap();
    assert_eq!(first.items[0].display_title, "apple");
    assert_eq!(first.items[1].display_title, "Ｚｅｂｒａ");
    assert_eq!(first.items.len(), 30);
    let cursor = first.next_cursor.as_deref().unwrap();
    assert!(service::list_web_links(&db, Some(cursor), CollectionSort::CreatedDesc).is_err());
    let second = service::list_web_links(&db, Some(cursor), CollectionSort::TitleAsc).unwrap();
    assert_eq!(second.items.len(), 5);
    assert!(second.next_cursor.is_none());
    let ids: HashSet<_> = first.items.iter().chain(&second.items).map(|item| &item.id).collect();
    assert_eq!(ids.len(), 35);
    assert!(first.items[29].id < second.items[0].id);

    let latest = service::list_web_links(&db, None, CollectionSort::CreatedDesc).unwrap();
    assert!(latest.items.windows(2).all(|pair| pair[0].created_at_ms >= pair[1].created_at_ms));
    let oldest = service::list_web_links(&db, None, CollectionSort::CreatedAsc).unwrap();
    assert!(oldest.items.windows(2).all(|pair| pair[0].created_at_ms <= pair[1].created_at_ms));

    let first = service::list_knowledge_pages(&db, None, CollectionSort::TitleDesc).unwrap();
    let second = service::list_knowledge_pages(&db, first.next_cursor.as_deref(), CollectionSort::TitleDesc).unwrap();
    assert_eq!(second.items.last().unwrap().display_title, "apple");
    assert_eq!(second.items[0].excerpt.trim(), "本文の抜粋");
}

#[test]
fn test_thumbnail_is_restored_from_backup_and_cleared_on_url_change() {
    let (_dir, db) = open_temp_db();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes).encode_image(&image::RgbImage::new(16, 9)).unwrap();
    let thumbnail = crate::web_metadata::thumbnail_data_url(&bytes);
    let link = service::create_web_link(&db, CreateWebLinkInput {
        url: "https://example.org/one".into(), title: "サムネイル付きリンク".into(),
        description: String::new(), thumbnail: Some(thumbnail.clone()),
    }, false).unwrap();
    assert!(link.has_thumbnail);
    assert_eq!(service::get_web_link_thumbnail(&db, &link.id).unwrap().as_deref(), Some(thumbnail.as_str()));
    let backup = db.create_backup(None).unwrap();

    let updated = service::update_web_link(&db, &link.id, UpdateWebLinkPatch {
        url: Some("https://example.org/two".into()), title: None, description: None, thumbnail: None,
    }, false).unwrap();
    assert!(!updated.has_thumbnail);
    assert!(service::get_web_link_thumbnail(&db, &link.id).unwrap().is_none());

    db.restore_backup(&backup.path).unwrap();
    assert_eq!(service::get_web_link_thumbnail(&db, &link.id).unwrap().as_deref(), Some(thumbnail.as_str()));
}

#[test]
fn test_v3_library_and_backup_are_migrated() {
    let (dir, db) = open_temp_db();
    let link = service::create_web_link(&db, CreateWebLinkInput {
        url: "https://example.org/legacy".into(), title: "以前のリンク".into(),
        description: String::new(), thumbnail: None,
    }, false).unwrap();
    let backup = db.create_backup(None).unwrap();
    let path = db.db_path().to_owned();
    drop(db);

    for path in [path.as_path(), std::path::Path::new(&backup.path)] {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("DROP TABLE web_link_thumbnails;
            DROP INDEX idx_web_links_active_created;
            DROP INDEX idx_pages_knowledge_active_created;
            PRAGMA user_version = 3;").unwrap();
    }

    let db = DbManager::open(dir.path().to_path_buf(), false).unwrap();
    assert_eq!(service::get_web_link(&db, &link.id).unwrap(), link);
    db.restore_backup(&backup.path).unwrap();
    assert_eq!(service::get_web_link(&db, &link.id).unwrap(), link);
    let version = rusqlite::Connection::open(&backup.path).unwrap()
        .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0)).unwrap();
    assert_eq!(version, 3);
}

#[test]
fn test_at01_at02_at03_diary_lifecycle() {
    let (dir, db) = open_temp_db();

    // AT-01: whitespace-only save on non-existent diary does not create row or mark date
    let r = service::save_diary(&db, "2026-11-18", "  \u{3000}\n\t ", None).unwrap();
    assert!(r.is_none());
    assert!(service::get_diary(&db, "2026-11-18").unwrap().is_none());
    assert!(service::list_diary_dates(&db, 2026, 11).unwrap().is_empty());

    // AT-02: saving to explicit selected date 2026-11-18 stays on 2026-11-18
    let r1 = service::save_diary(&db, "2026-11-18", "昨日の続きメモ", None)
        .unwrap()
        .unwrap();
    assert_eq!(r1.revision, 1);

    // AT-03: reopen DB, save again on same date, add tag, then clear body
    drop(db);
    let db = DbManager::open(dir.path().to_path_buf(), false).unwrap();
    let tag = service::create_tag(&db, "日課").unwrap();
    service::set_page_tags(&db, &r1.page_id, vec![tag.id.clone()]).unwrap();

    let loaded = service::get_diary(&db, "2026-11-18").unwrap().unwrap();
    assert_eq!(loaded.kind, PageKind::Diary);
    assert_eq!(loaded.display_title, "2026-11-18の日記");
    assert_eq!(loaded.tags.len(), 1);
    assert_eq!(service::list_diary_dates(&db, 2026, 11).unwrap(), vec!["2026-11-18"]);

    // Clear body -> calendar mark disappears, row & tags stay
    let r2 = service::save_diary(&db, "2026-11-18", "   ", Some(1))
        .unwrap()
        .unwrap();
    assert_eq!(r2.page_id, r1.page_id);
    assert_eq!(r2.revision, 2);
    assert!(service::list_diary_dates(&db, 2026, 11).unwrap().is_empty());

    let after_clear = service::get_diary(&db, "2026-11-18").unwrap().unwrap();
    assert_eq!(after_clear.id, r1.page_id);
    assert_eq!(after_clear.tags.len(), 1);

    // Re-writing preserves same row id
    let r3 = service::save_diary(&db, "2026-11-18", "再入力", Some(2))
        .unwrap()
        .unwrap();
    assert_eq!(r3.page_id, r1.page_id);
    assert_eq!(service::list_diary_dates(&db, 2026, 11).unwrap(), vec!["2026-11-18"]);
}

#[test]
fn test_at04_attached_pages_reorder_and_persistence() {
    let (dir, db) = open_temp_db();

    let link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://example.org/docs".into(),
            title: "Example Docs".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();

    let p1 = service::create_page(&db, &link.id, "ページ1").unwrap();
    let p2 = service::create_page(&db, &link.id, "ページ2").unwrap();
    let p3 = service::create_page(&db, &link.id, "ページ3").unwrap();

    service::reorder_pages(&db, &link.id, vec![p3.id.clone(), p1.id.clone(), p2.id.clone()])
        .unwrap();

    drop(db);
    let db = DbManager::open(dir.path().to_path_buf(), false).unwrap();
    let detail = service::get_web_link(&db, &link.id).unwrap();
    let ids: Vec<String> = detail.pages.into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![p3.id, p1.id, p2.id]);
}

#[test]
fn test_at05_at06_tags_and_and_filtering() {
    let (_dir, db) = open_temp_db();

    // AT-06: fullwidth vs halfwidth/case tag creation
    let tag_a = service::create_tag(&db, "Ｒｕｓｔ").unwrap();
    let tag_a_again = service::create_tag(&db, "rust").unwrap();
    assert_eq!(tag_a.id, tag_a_again.id);
    assert_eq!(tag_a.name, "Ｒｕｓｔ");

    let tag_b = service::create_tag(&db, "SQLite").unwrap();

    let link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://doc.rust-lang.org/".into(),
            title: "Rust Doc".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    let child = service::create_page(&db, &link.id, "メモ").unwrap();

    // AT-05: parent gets tag A, child gets tag B -> AND(A, B) matches neither
    service::set_web_link_tags(&db, &link.id, vec![tag_a.id.clone()]).unwrap();
    service::set_page_tags(&db, &child.id, vec![tag_b.id.clone()]).unwrap();

    let res = service::search(
        &db,
        "",
        None,
        Some(vec![tag_a.id.clone(), tag_b.id.clone()]),
        None,
        None,
    )
    .unwrap();
    assert!(res.items.is_empty());

    // Add tag B directly to parent -> now parent matches AND(A, B)
    service::set_web_link_tags(&db, &link.id, vec![tag_a.id.clone(), tag_b.id.clone()]).unwrap();
    let res2 = service::search(
        &db,
        "",
        None,
        Some(vec![tag_a.id.clone(), tag_b.id.clone()]),
        None,
        None,
    )
    .unwrap();
    assert_eq!(res2.items.len(), 1);
    assert_eq!(res2.items[0].id, link.id);

    // Rename & delete tag reflects on attached items
    let renamed = service::rename_tag(&db, &tag_a.id, "RustLang").unwrap();
    assert_eq!(renamed.name, "RustLang");
    let link_detail = service::get_web_link(&db, &link.id).unwrap();
    assert!(link_detail.tags.iter().any(|t| t.name == "RustLang"));

    service::delete_tag(&db, &tag_a.id).unwrap();
    let link_after = service::get_web_link(&db, &link.id).unwrap();
    assert_eq!(link_after.tags.len(), 1);
    assert_eq!(link_after.tags[0].id, tag_b.id);
}

#[test]
fn test_at07_at08_url_normalization_and_duplicates() {
    let (_dir, db) = open_temp_db();

    // AT-07
    let first = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://EXAMPLE.com:443".into(),
            title: "First".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();

    let err = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://example.com/".into(),
            title: "Second".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap_err();
    match err {
        AppError::DuplicateUrl { candidates } => {
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].id, first.id);
        }
        other => panic!("expected DuplicateUrl, got {other:?}"),
    }

    let second = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://example.com/".into(),
            title: "Second".into(),
            description: "".into(),
            thumbnail: None,
        },
        true,
    )
    .unwrap();
    assert_ne!(first.id, second.id);

    // AT-08: query/fragment differ -> distinct; matching other link -> excludes self & warns
    let distinct = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://example.com/?a=1#top".into(),
            title: "WithQuery".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();

    // Self-update keeping same normalized URL does not conflict
    let self_upd = service::update_web_link(
        &db,
        &first.id,
        UpdateWebLinkPatch {
            url: Some("https://example.com:443/".into()),
            title: Some("First Renamed".into()),
            description: None,
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    assert_eq!(self_upd.title, "First Renamed");

    // Changing `distinct` URL to match `first` triggers DuplicateUrl excluding `distinct`
    let upd_err = service::update_web_link(
        &db,
        &distinct.id,
        UpdateWebLinkPatch {
            url: Some("https://example.com/".into()),
            title: None,
            description: None,
            thumbnail: None,
        },
        false,
    )
    .unwrap_err();
    match upd_err {
        AppError::DuplicateUrl { candidates } => {
            assert_eq!(candidates.len(), 2);
            assert!(candidates.iter().all(|c| c.id != distinct.id));
        }
        other => panic!("expected DuplicateUrl, got {other:?}"),
    }
}

#[test]
fn test_at09_at10_at11_at12_search_ranking_fts5_and_paging() {
    let (_dir, db) = open_temp_db();

    // AT-09: Japanese 3+ chars, fullwidth alnum, quotes and asterisks
    let link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://notes.example.jp/guide".into(),
            title: "日本語の検索ガイド".into(),
            description: "引用符\"テスト\"とアスタリスク*記号*".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    let page = service::create_page(&db, &link.id, "ＡＢＣ設定手順").unwrap();
    service::save_page(
        &db,
        &page.id,
        "本文には全文検索や\"特殊記号\"と `*記号*` が含まれます。",
        page.revision,
    )
    .unwrap();

    let hit_jp = service::search(&db, "全文検索", None, None, None, None).unwrap();
    assert_eq!(hit_jp.items.len(), 1);
    assert_eq!(hit_jp.items[0].id, page.id);
    assert_eq!(hit_jp.items[0].matched_field, Some(MatchedField::Body));

    let hit_fullwidth = service::search(&db, "abc", None, None, None, None).unwrap();
    assert!(hit_fullwidth.items.iter().any(|h| h.id == page.id));

    let hit_quote = service::search(&db, "\"テスト\"", None, None, None, None).unwrap();
    assert_eq!(hit_quote.items.len(), 1);
    assert_eq!(hit_quote.items[0].id, link.id);
    assert_eq!(
        hit_quote.items[0].matched_field,
        Some(MatchedField::Description)
    );

    let hit_star = service::search(&db, "*記号*", None, None, None, None).unwrap();
    assert_eq!(hit_star.items.len(), 2);

    // AT-10: 1-2 chars prefix search on title, tag, host
    let short_tag = service::create_tag(&db, "メモ").unwrap();
    let diary = service::save_diary(&db, "2026-11-10", "短い検索テスト", None)
        .unwrap()
        .unwrap();
    service::set_page_tags(&db, &diary.page_id, vec![short_tag.id]).unwrap();

    let hit_1char_title = service::search(&db, "日", None, None, None, None).unwrap();
    assert!(hit_1char_title.items.iter().any(|h| h.id == link.id));

    let hit_2char_host = service::search(&db, "no", None, None, None, None).unwrap();
    assert!(hit_2char_host.items.iter().any(|h| h.id == link.id));

    let hit_2char_tag = service::search(&db, "メモ", None, None, None, None).unwrap();
    assert!(hit_2char_tag.items.iter().any(|h| h.id == diary.page_id));

    // AT-11: title exact match ranks ahead of body-only matches across 35 items, paging has no gaps/dups
    for i in 0..35 {
        let l = service::create_web_link(
            &db,
            CreateWebLinkInput {
                url: format!("https://batch.example.com/{i}"),
                title: format!("バッチ項目{i:02}"),
                description: "".into(),
                thumbnail: None,
            },
            false,
        )
        .unwrap();
        let p = service::create_page(&db, &l.id, &format!("補助{i:02}")).unwrap();
        service::save_page(&db, &p.id, "ターゲット語句が本文に存在します", p.revision).unwrap();
    }

    let exact_link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://exact.example.com/".into(),
            title: "ターゲット語句".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();

    let p1 = service::search(&db, "ターゲット語句", None, None, None, None).unwrap();
    assert_eq!(p1.items.len(), 30);
    assert_eq!(p1.items[0].id, exact_link.id);
    assert!(p1.next_cursor.is_some());

    let p2 = service::search(&db, "ターゲット語句", None, None, None, p1.next_cursor.as_deref())
        .unwrap();
    assert_eq!(p2.items.len(), 6);
    assert!(p2.next_cursor.is_none());

    let all_ids: HashSet<String> = p1
        .items
        .into_iter()
        .chain(p2.items.into_iter())
        .map(|h| h.id)
        .collect();
    assert_eq!(all_ids.len(), 36);

    // AT-12: update body, parent title, tag name -> old query stops matching, new query matches
    service::save_page(&db, &page.id, "新しい本文キーワード", page.revision + 1).unwrap();
    assert!(service::search(&db, "全文検索", None, None, None, None)
        .unwrap()
        .items
        .is_empty());
    assert!(service::search(&db, "本文キーワード", None, None, None, None)
        .unwrap()
        .items
        .iter()
        .any(|h| h.id == page.id));

    service::update_web_link(
        &db,
        &link.id,
        UpdateWebLinkPatch {
            url: None,
            title: Some("改名後の親タイトル".into()),
            description: None,
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    assert!(service::search(&db, "検索ガイド", None, None, None, None)
        .unwrap()
        .items
        .is_empty());
    let parent_title_hits =
        service::search(&db, "改名後の親", Some(SearchKind::AttachedPage), None, None, None)
            .unwrap();
    assert_eq!(parent_title_hits.items.len(), 1);
    assert_eq!(parent_title_hits.items[0].id, page.id);
    assert_eq!(
        parent_title_hits.items[0].matched_field,
        Some(MatchedField::ParentTitle)
    );
}

#[test]
fn test_at13_trash_hierarchy_and_tag_counts() {
    let (_dir, db) = open_temp_db();

    let tag = service::create_tag(&db, "整理").unwrap();
    let link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://trash-test.example/".into(),
            title: "親リンク".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    let child1 = service::create_page(&db, &link.id, "先に消す子").unwrap();
    let child2 = service::create_page(&db, &link.id, "残す子").unwrap();

    service::set_web_link_tags(&db, &link.id, vec![tag.id.clone()]).unwrap();
    service::set_page_tags(&db, &child1.id, vec![tag.id.clone()]).unwrap();
    service::set_page_tags(&db, &child2.id, vec![tag.id.clone()]).unwrap();

    assert_eq!(service::list_tags(&db, None).unwrap()[0].count, 3);

    // Delete child1 first
    service::soft_delete_page(&db, &child1.id).unwrap();
    assert_eq!(service::list_tags(&db, None).unwrap()[0].count, 2);

    // Delete parent
    service::soft_delete_web_link(&db, &link.id).unwrap();
    assert_eq!(service::list_tags(&db, None).unwrap()[0].count, 0);
    assert!(service::search(&db, "親リンク", None, None, None, None)
        .unwrap()
        .items
        .is_empty());
    assert!(service::search(&db, "残す子", None, None, None, None)
        .unwrap()
        .items
        .is_empty());

    // Trash shows only parent while parent is deleted
    let trash_while_parent_deleted = service::list_trash(&db).unwrap();
    assert_eq!(trash_while_parent_deleted.len(), 1);
    assert_eq!(trash_while_parent_deleted[0].kind, TrashKind::WebLink);

    // Restore parent -> child2 returns, child1 stays in trash
    service::restore_web_link(&db, &link.id).unwrap();
    assert_eq!(service::list_tags(&db, None).unwrap()[0].count, 2);
    let trash_after_parent_restore = service::list_trash(&db).unwrap();
    assert_eq!(trash_after_parent_restore.len(), 1);
    assert_eq!(trash_after_parent_restore[0].id, child1.id);
    assert_eq!(trash_after_parent_restore[0].kind, TrashKind::AttachedPage);

    // Restore child1 -> goes to end of parent's active pages
    service::restore_page(&db, &child1.id).unwrap();
    let detail = service::get_web_link(&db, &link.id).unwrap();
    let ids: Vec<String> = detail.pages.into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![child2.id, child1.id]);
}

#[test]
fn test_at16_at18_at22_markdown_revision_and_export() {
    let (dir, db) = open_temp_db();

    // AT-16: tables, tasklists, blockquotes, trailing spaces, CRLF -> LF export roundtrip
    let raw_crlf = "# 見出し\r\n\r\n> 引用文\r\n\r\n- [x] 完了タスク  \r\n- [ ] 未完了\r\n\r\n| 列1 | 列2 |\r\n|---|---|\r\n| A | B |\r\n";
    let expected_lf = raw_crlf.replace("\r\n", "\n");

    let saved = service::save_diary(&db, "2026-11-15", raw_crlf, None)
        .unwrap()
        .unwrap();
    let loaded = service::get_diary(&db, "2026-11-15").unwrap().unwrap();
    assert_eq!(loaded.body_markdown, expected_lf);
    let preview = service::get_page_preview(&db, &loaded.id).unwrap().unwrap();
    assert!(preview.contains("<table>"));
    assert!(preview.contains("type=\"checkbox\""));
    assert!(preview.contains("<blockquote>"));

    let export_path = dir.path().join("exported.md");
    service::export_page_markdown(&db, &saved.page_id, export_path.to_str().unwrap()).unwrap();
    let exported_bytes = fs::read_to_string(&export_path).unwrap();
    assert_eq!(exported_bytes, expected_lf);
    let database_path = db.status().db_path;
    let error = service::export_page_markdown(&db, &saved.page_id, &database_path).unwrap_err();
    assert_eq!(error.code(), "Validation");

    // AT-18: stale expectedRevision triggers RevisionConflict and preserves DB content
    let latest = service::save_diary(&db, "2026-11-15", "最新内容", Some(1))
        .unwrap()
        .unwrap();
    let retried = service::save_diary(&db, "2026-11-15", "最新内容", Some(1))
        .unwrap()
        .unwrap();
    assert_eq!(retried.revision, latest.revision);
    assert_eq!(retried.updated_at_ms, latest.updated_at_ms);
    let conflict = service::save_diary(&db, "2026-11-15", "古い下書き", Some(1)).unwrap_err();
    match conflict {
        AppError::RevisionConflict {
            current_revision, ..
        } => {
            assert_eq!(current_revision, Some(2));
        }
        other => panic!("expected RevisionConflict, got {other:?}"),
    }
    assert_eq!(
        service::get_diary(&db, "2026-11-15")
            .unwrap()
            .unwrap()
            .body_markdown,
        "最新内容"
    );

    // AT-22: raw HTML escaped as text, external images rendered as alt text only, unsafe links inert
    let unsafe_md = concat!(
        "<script>alert(1)</script>\n\n",
        "文中の<b>生HTML</b>です。\n\n",
        "![危険な外部画像](https://evil.example/track.png)\n\n",
        "[JavaScriptリンク](javascript:alert(1))\n\n",
        "[安全なリンク](https://example.com/ok)"
    );
    let html = render_markdown_preview(unsafe_md);
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<b>"));
    assert!(!html.contains("<img"));
    assert!(!html.contains("javascript:"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(html.contains("[画像: 危険な外部画像]"));
    assert!(html.contains("class=\"md-invalid-link\""));
    assert!(html.contains("data-external-url=\"https://example.com/ok\""));

    let search_txt = extract_search_text(unsafe_md);
    assert!(search_txt.contains("危険な外部画像"));
    assert!(search_txt.contains("安全なリンク"));
}

#[test]
fn test_at19_at20_at21_backup_restore_and_rebuild() {
    let (dir, db) = open_temp_db();

    let tag = service::create_tag(&db, "バックアップ対象").unwrap();
    let link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://backup.example/".into(),
            title: "元のリンク".into(),
            description: "元の説明".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    service::set_web_link_tags(&db, &link.id, vec![tag.id.clone()]).unwrap();
    let deleted_link = service::create_web_link(
        &db,
        CreateWebLinkInput {
            url: "https://deleted.example/".into(),
            title: "削除済みリンク".into(),
            description: "".into(),
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    service::soft_delete_web_link(&db, &deleted_link.id).unwrap();

    // AT-19: create backup, mutate DB, restore backup
    let backup = db.create_backup(None).unwrap();
    assert!(std::path::Path::new(&backup.path).is_file());
    assert!(db.list_backups().unwrap().iter().any(|b| b.path == backup.path));
    let second_backup = db.create_backup(None).unwrap();
    assert_ne!(backup.path, second_backup.path);

    // Overwriting existing backup or active DB path is rejected
    assert!(db.create_backup(Some(&backup.path)).is_err());
    assert!(db
        .create_backup(Some(db.db_path().to_str().unwrap()))
        .is_err());

    service::update_web_link(
        &db,
        &link.id,
        UpdateWebLinkPatch {
            url: None,
            title: Some("変更後のリンク".into()),
            description: None,
            thumbnail: None,
        },
        false,
    )
    .unwrap();
    service::save_diary(&db, "2026-11-18", "復元で消える日記", None).unwrap();

    let restore_info = db.restore_backup(&backup.path).unwrap();
    assert!(std::path::Path::new(&restore_info.pre_restore_backup_path).is_file());
    let second_restore = db.restore_backup(&second_backup.path).unwrap();
    assert_ne!(restore_info.pre_restore_backup_path, second_restore.pre_restore_backup_path);
    let previous = rusqlite::Connection::open(&restore_info.pre_restore_backup_path).unwrap();
    let previous_title: String = previous.query_row(
        "SELECT title FROM web_links WHERE id = ?1",
        [&link.id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(previous_title, "変更後のリンク");

    let restored_link = service::get_web_link(&db, &link.id).unwrap();
    assert_eq!(restored_link.title, "元のリンク");
    assert!(service::get_diary(&db, "2026-11-18").unwrap().is_none());
    assert_eq!(
        service::search(&db, "元の説明", None, None, None, None)
            .unwrap()
            .items
            .len(),
        1
    );

    // AT-20: incompatible backup rejected while preserving current DB
    let bad_backup = dir.path().join("bad.sqlite3");
    fs::write(&bad_backup, b"not a sqlite database").unwrap();
    let bad_err = db.restore_backup(bad_backup.to_str().unwrap()).unwrap_err();
    assert_eq!(bad_err.code(), "IncompatibleBackup");
    assert_eq!(service::get_web_link(&db, &link.id).unwrap().title, "元のリンク");

    // Wrong application_id SQLite DB rejected
    let foreign_db_path = dir.path().join("foreign.sqlite3");
    {
        let c = rusqlite::Connection::open(&foreign_db_path).unwrap();
        c.execute_batch("CREATE TABLE dummy (x INT);").unwrap();
    }
    let foreign_err = db
        .restore_backup(foreign_db_path.to_str().unwrap())
        .unwrap_err();
    assert_eq!(foreign_err.code(), "IncompatibleBackup");
    assert_eq!(service::get_web_link(&db, &link.id).unwrap().title, "元のリンク");

    // A backup with the right application ID and a legacy version is also incompatible.
    let legacy = dir.path().join("legacy.sqlite3");
    fs::copy(&backup.path, &legacy).unwrap();
    rusqlite::Connection::open(&legacy)
        .unwrap()
        .pragma_update(None, "user_version", 2)
        .unwrap();
    let legacy_error = db.restore_backup(legacy.to_str().unwrap()).unwrap_err();
    assert_eq!(legacy_error.code(), "IncompatibleBackup");
    assert_eq!(service::get_web_link(&db, &link.id).unwrap().title, "元のリンク");

    // AT-21: rebuild search index keeps active items and excludes deleted items
    let before = service::search(&db, "", None, None, Some(SearchSort::Updated), None).unwrap();
    let rebuilt = db.rebuild_search_index().unwrap();
    assert_eq!(rebuilt, 1);
    let after = service::search(&db, "", None, None, Some(SearchSort::Updated), None).unwrap();
    assert_eq!(before.items, after.items);
    assert!(service::search(&db, "削除済みリンク", None, None, None, None)
        .unwrap()
        .items
        .is_empty());

    // Confirm PRAGMA headers persisted
    db.with_reader(|conn| {
        let app_id: i32 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let user_ver: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(app_id, OBOROMI_APPLICATION_ID);
        assert_eq!(user_ver, OBOROMI_USER_VERSION);
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_explain_query_plan_indexes() {
    let (_dir, db) = open_temp_db();

    db.with_reader(|conn| {
        let explain = |sql: &str, params: &[&str]| -> String {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = stmt
                .query_map(rusqlite::params_from_iter(params.iter().copied()), |r| {
                    r.get::<_, String>(3)
                })
                .unwrap();
            rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
        };

        let title_plan = explain(
            "SELECT kind, target_id FROM search_documents WHERE title_key >= ?1 AND title_key < ?2",
            &["ab", "ac"],
        );
        assert!(
            title_plan.contains("idx_search_title"),
            "unexpected title plan: {title_plan}"
        );

        let host_plan = explain(
            "SELECT target_id FROM search_documents WHERE kind = 'web_link' AND host_key >= ?1 AND host_key < ?2",
            &["ex", "ey"],
        );
        assert!(
            host_plan.contains("idx_search_host"),
            "unexpected host plan: {host_plan}"
        );

        let tag_plan = explain(
            "SELECT id FROM tags WHERE name_key >= ?1 AND name_key < ?2",
            &["ru", "rv"],
        );
        assert!(
            tag_plan.contains("USING INDEX") || tag_plan.contains("USING COVERING INDEX"),
            "unexpected tag plan: {tag_plan}"
        );

        let url_plan = explain(
            "SELECT id FROM web_links WHERE normalized_url = ?1 AND deleted_at_ms IS NULL",
            &["https://example.com/"],
        );
        assert!(
            url_plan.contains("idx_web_links_active_url"),
            "unexpected url plan: {url_plan}"
        );

        let knowledge_plan = explain(
            "SELECT id FROM pages INDEXED BY idx_pages_knowledge_active_updated WHERE web_link_id IS NULL AND diary_date IS NULL AND deleted_at_ms IS NULL ORDER BY updated_at_ms DESC, id ASC LIMIT 31",
            &[],
        );
        assert!(
            knowledge_plan.contains("idx_pages_knowledge_active_updated")
                && !knowledge_plan.contains("TEMP B-TREE"),
            "unexpected knowledge plan: {knowledge_plan}"
        );
        let fts_plan = explain(
            "SELECT sd.target_id, sd.title_key, sd.updated_at_ms
             FROM search_fts f
             CROSS JOIN search_documents sd INDEXED BY idx_search_match ON sd.id = f.rowid
             WHERE search_fts MATCH ?1",
            &["\"日本語\""],
        );
        assert!(fts_plan.contains("USING COVERING INDEX idx_search_match"), "{fts_plan}");

        Ok(())
    })
    .unwrap();
}

#[test]
fn test_performance_10k_documents() {
    benchmark_search(10_000);
}

#[test]
#[ignore = "100,000-document release benchmark"]
fn test_performance_100k_documents() {
    benchmark_search(100_000);
}

fn benchmark_search(documents: usize) {
    let (_dir, db) = open_temp_db();

    let seed_start = Instant::now();
    let tag_alpha = service::create_tag(&db, "設計").unwrap();
    let tag_beta = service::create_tag(&db, "重要").unwrap();
    let links = documents * 3 / 10;
    let diary_days = 2_000;
    let attached = documents - links - diary_days;
    let body_paragraph = "設計時に確認した操作を記録する。カレンダーから日付を開き、ページの関連情報と更新履歴を確認する。\n\n";
    let long_body = body_paragraph.repeat(20);

    db.with_transaction(|tx| {
        let mut wl_stmt = tx.prepare(
            "INSERT INTO web_links (id, url, normalized_url, title, description, created_at_ms, updated_at_ms, deleted_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL)",
        )?;
        let mut sd_stmt = tx.prepare(
            "INSERT INTO search_documents (target_id, kind, title_key, host_key, url_text_key, description_key, body_key, parent_title_key, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        let mut pg_stmt = tx.prepare(
            "INSERT INTO pages (id, web_link_id, diary_date, title, body_markdown, position, revision, created_at_ms, updated_at_ms, deleted_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, NULL)",
        )?;
        let mut wtag_stmt =
            tx.prepare("INSERT INTO web_link_tags (web_link_id, tag_id) VALUES (?1, ?2)")?;

        let base_ts = 1_760_000_000_000i64;
        for i in 0..links {
            let wid = format!("wl-{i:05}");
            let title = if i == 1500 {
                "ターゲット設計文書".to_string()
            } else {
                format!("リファレンス資料 {i:04}")
            };
            let url = format!("https://docs{i:04}.example.com/item");
            let desc = format!("概要説明テキスト 番号{i} キャッシュと検索の検証");
            let ts = base_ts + i as i64;
            wl_stmt.execute(rusqlite::params![wid, url, url, title, desc, ts])?;
            sd_stmt.execute(rusqlite::params![
                wid,
                "web_link",
                normalize_search_key(&title),
                format!("docs{i:04}.example.com"),
                normalize_search_key(&url),
                normalize_search_key(&desc),
                "",
                "",
                ts
            ])?;
            if i % 10 == 0 {
                wtag_stmt.execute(rusqlite::params![wid, tag_alpha.id])?;
                if i % 20 == 0 {
                    wtag_stmt.execute(rusqlite::params![wid, tag_beta.id])?;
                }
            }
        }

        for i in 0..attached {
            let pid = format!("pg-{i:05}");
            let wid = format!("wl-{:05}", i % links);
            let title = format!("付属メモ {i:04}");
            let body = if i == 2500 {
                "# 詳細\n\nこれは特殊な日本語キーワード「朧見検索」を含む本文です。\n".to_string()
            } else {
                format!("# メモ {i}\n\n標準的な作業ログとメモ項目 {i} の本文テキスト。\n\n{long_body}")
            };
            let ts = base_ts + 10_000 + i as i64;
            pg_stmt.execute(rusqlite::params![
                pid,
                wid,
                Option::<String>::None,
                title,
                body,
                (i / links) as i64,
                ts
            ])?;
            sd_stmt.execute(rusqlite::params![
                pid,
                "attached_page",
                normalize_search_key(&title),
                "",
                "",
                "",
                normalize_search_key(&body),
                format!("リファレンス資料 {:04}", i % links),
                ts
            ])?;
        }

        let start_date = chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        for i in 0..diary_days {
            let did = format!("dy-{i:05}");
            let date = (start_date + chrono::Duration::days(i as i64)).format("%Y-%m-%d").to_string();
            let body = format!("今日の日記記録 {i}。進捗確認とメモ。");
            let ts = base_ts + 20_000 + i as i64;
            pg_stmt.execute(rusqlite::params![
                did,
                Option::<String>::None,
                Some(date.clone()),
                "",
                body,
                0,
                ts
            ])?;
            sd_stmt.execute(rusqlite::params![
                did,
                "diary",
                normalize_search_key(&format!("{date}の日記")),
                "",
                "",
                "",
                normalize_search_key(&body),
                "",
                ts
            ])?;
        }

        Ok(())
    })
    .unwrap();

    let seed_elapsed = seed_start.elapsed();
    // Warm up caches
    let _ = service::search(&db, "リファレンス", None, None, None, None).unwrap();

    // Measure warm FTS5 Japanese search
    let t_jp = Instant::now();
    let res_jp = service::search(&db, "朧見検索", None, None, None, None).unwrap();
    let elapsed_jp = t_jp.elapsed();
    assert_eq!(res_jp.items.len(), 1);
    assert_eq!(res_jp.items[0].id, "pg-02500");

    // Measure warm prefix + tag AND search
    let t_tag = Instant::now();
    let res_tag = service::search(
        &db,
        "リフ",
        None,
        Some(vec![tag_alpha.id, tag_beta.id]),
        None,
        None,
    )
    .unwrap();
    let elapsed_tag = t_tag.elapsed();
    assert_eq!(res_tag.items.len(), 30);

    // Measure preview load
    let t_prev = Instant::now();
    let preview = service::get_page_preview(&db, "pg-02500").unwrap().unwrap();
    let elapsed_prev = t_prev.elapsed();
    assert!(preview.contains("朧見検索"));

    let broad = service::search(&db, "カレンダー", None, None, None, None).unwrap();
    assert_eq!(broad.items.len(), 30);
    let broad_cursor = broad.next_cursor.unwrap();
    let measure = |query, cursor| {
        let mut runs: Vec<_> = (0..30)
            .map(|_| {
                let start = Instant::now();
                let result = service::search(&db, query, None, None, None, cursor).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(result.items.len(), 30);
                elapsed
            })
            .collect();
        runs.sort_unstable();
        runs[28]
    };
    let broad_p95 = measure("カレンダー", None);
    let broad_next_p95 = measure("カレンダー", Some(&broad_cursor));

    eprintln!(
        "[perf {documents}] seed={seed_elapsed:?}, warm_fts_jp={elapsed_jp:?}, \
         warm_tag_prefix={elapsed_tag:?}, preview={elapsed_prev:?}, \
         broad_body_p95={broad_p95:?}, broad_body_next_p95={broad_next_p95:?}"
    );

    assert!(
        elapsed_jp.as_millis() < 150,
        "warm Japanese FTS search exceeded 150ms target: {:?}",
        elapsed_jp
    );
    assert!(
        elapsed_tag.as_millis() < 150,
        "warm prefix+tag search exceeded 150ms target: {:?}",
        elapsed_tag
    );
    assert!(
        elapsed_prev.as_millis() < 100,
        "preview load exceeded 100ms target: {:?}",
        elapsed_prev
    );
}

#[test]
fn test_knowledge_page_lifecycle_search_trash_and_export() {
    let (dir, db) = open_temp_db();

    // Create untitled and titled knowledge pages
    let untitled = service::create_knowledge_page(&db, "  ", None).unwrap();
    assert_eq!(untitled.kind, PageKind::KnowledgePage);
    assert_eq!(untitled.display_title, "無題");
    assert!(untitled.parent.is_none());
    assert!(untitled.diary_date.is_none());

    let kp = service::create_knowledge_page(&db, "  設計メモ  ", Some("# 見出し\r\n初期本文")).unwrap();
    assert_eq!(kp.kind, PageKind::KnowledgePage);
    assert_eq!(kp.title, "設計メモ");
    assert_eq!(kp.display_title, "設計メモ");
    assert_eq!(kp.body_markdown, "# 見出し\n初期本文");
    assert_eq!(kp.revision, 1);

    // Save body + revision conflict check
    let receipt = service::save_page(&db, &kp.id, "知識ベース用の詳細テキスト", kp.revision).unwrap();
    assert_eq!(receipt.revision, 2);
    let retried = service::save_page(&db, &kp.id, "知識ベース用の詳細テキスト", kp.revision).unwrap();
    assert_eq!(retried.revision, receipt.revision);
    assert_eq!(retried.updated_at_ms, receipt.updated_at_ms);
    let conflict = service::save_page(&db, &kp.id, "古い競合下書き", 1).unwrap_err();
    assert_eq!(conflict.code(), "RevisionConflict");

    // Rename updates display title and search index
    let renamed = service::rename_page(&db, &kp.id, "アーキテクチャ知見").unwrap();
    assert_eq!(renamed.display_title, "アーキテクチャ知見");
    assert!(service::search(&db, "設計メモ", Some(SearchKind::KnowledgePage), None, None, None)
        .unwrap()
        .items
        .is_empty());

    // Tagging & tag-filtered search
    let tag_a = service::create_tag(&db, "知見").unwrap();
    let tag_b = service::create_tag(&db, "Rust").unwrap();
    service::set_page_tags(&db, &kp.id, vec![tag_a.id.clone(), tag_b.id.clone()]).unwrap();
    assert_eq!(service::list_tags(&db, Some("知見")).unwrap()[0].count, 1);

    let fts_hits = service::search(
        &db,
        "詳細テキスト",
        Some(SearchKind::KnowledgePage),
        Some(vec![tag_a.id.clone(), tag_b.id.clone()]),
        None,
        None,
    )
    .unwrap();
    assert_eq!(fts_hits.items.len(), 1);
    assert_eq!(fts_hits.items[0].id, kp.id);
    assert_eq!(fts_hits.items[0].kind, SearchKind::KnowledgePage);
    assert_eq!(fts_hits.items[0].matched_field, Some(MatchedField::Body));
    assert!(fts_hits.items[0].parent.is_none());

    // Export markdown
    let export_path = dir.path().join("knowledge.md");
    service::export_page_markdown(&db, &kp.id, export_path.to_str().unwrap()).unwrap();
    assert_eq!(fs::read_to_string(&export_path).unwrap(), "知識ベース用の詳細テキスト");

    // Soft-delete & Trash & Restore
    service::soft_delete_page(&db, &kp.id).unwrap();
    assert_eq!(service::list_tags(&db, Some("知見")).unwrap()[0].count, 0);
    assert!(service::search(&db, "アーキテクチャ知見", None, None, None, None)
        .unwrap()
        .items
        .is_empty());

    let trash = service::list_trash(&db).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, kp.id);
    assert_eq!(trash[0].kind, TrashKind::KnowledgePage);
    assert_eq!(trash[0].title, "アーキテクチャ知見");
    assert!(trash[0].parent_id.is_none());

    service::restore_page(&db, &kp.id).unwrap();
    assert_eq!(service::list_tags(&db, Some("知見")).unwrap()[0].count, 1);
    assert!(service::list_trash(&db).unwrap().is_empty());
    assert_eq!(
        service::search(&db, "アーキテクチャ知見", Some(SearchKind::KnowledgePage), None, None, None)
            .unwrap()
            .items
            .len(),
        1
    );

    // Cursor pagination over > 30 knowledge pages
    for i in 0..33 {
        service::create_knowledge_page(&db, &format!("ページ{i:02}"), None).unwrap();
    }
    let page1 = service::list_knowledge_pages(&db, None, Default::default()).unwrap();
    assert_eq!(page1.items.len(), 30);
    assert!(page1.next_cursor.is_some());
    let page2 = service::list_knowledge_pages(&db, page1.next_cursor.as_deref(), Default::default()).unwrap();
    assert_eq!(page2.items.len(), 5);
    assert!(page2.next_cursor.is_none());
    let unique_ids: HashSet<String> = page1
        .items
        .into_iter()
        .chain(page2.items.into_iter())
        .map(|p| p.id)
        .collect();
    assert_eq!(unique_ids.len(), 35);
}
