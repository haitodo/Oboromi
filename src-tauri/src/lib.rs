pub mod db;
pub mod error;
pub mod markdown;
pub mod models;
pub mod normalize;
pub mod service;
mod web_metadata;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{Manager, State};

use crate::db::DbManager;
use crate::error::AppError;
use crate::models::{
    AppStatus, BackupInfo, CollectionSort, CreateWebLinkInput, KnowledgePageSummary, PageView, RestoreInfo,
    ResultPage, SaveReceipt, SearchHit, SearchKind, SearchSort, Tag, TagSummary, TrashEntry,
    UpdateWebLinkPatch, WebLinkDetail, WebLinkMetadata, WebLinkPreview, WebLinkSummary,
};

struct AppState {
    data_dir: PathBuf,
    db: Mutex<Option<Arc<DbManager>>>,
}

impl AppState {
    fn database(&self) -> Result<Arc<DbManager>, AppError> {
        let mut connection = self.db.lock().unwrap();
        if let Some(db) = connection.as_ref() {
            return Ok(Arc::clone(db));
        }
        let db = DbManager::open(self.data_dir.clone(), true).map_err(|err| {
            AppError::StorageError(format!("{}\n保存場所: {}", err, self.data_dir.display()))
        })?;
        let db = Arc::new(db);
        *connection = Some(Arc::clone(&db));
        Ok(db)
    }
}

async fn run_blocking<T: Send + 'static>(
    op: &str,
    action: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let result = tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|_| AppError::StorageError("バックグラウンド処理を完了できませんでした".into()))
        .and_then(|result| result);
    if let Err(ref err) = result {
        eprintln!("[oboromi] op={op} error_code={}", err.code());
    }
    result
}

async fn with_db<T: Send + 'static>(
    state: State<'_, Arc<AppState>>,
    op: &str,
    action: impl FnOnce(&DbManager) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let state = Arc::clone(state.inner());
    run_blocking(op, move || {
        let db = state.database()?;
        action(&db)
    })
    .await
}

#[tauri::command]
async fn get_app_status(state: State<'_, Arc<AppState>>) -> Result<AppStatus, AppError> {
    with_db(state, "get_app_status", |db| Ok(db.status())).await
}

#[tauri::command]
async fn rebuild_search_index(state: State<'_, Arc<AppState>>) -> Result<i64, AppError> {
    with_db(state, "rebuild_search_index", move |db| {
        db.rebuild_search_index()
    })
    .await
}

#[tauri::command]
async fn get_diary(
    state: State<'_, Arc<AppState>>,
    date: String,
) -> Result<Option<PageView>, AppError> {
    with_db(state, "get_diary", move |db| service::get_diary(db, &date)).await
}

#[tauri::command]
async fn save_diary(
    state: State<'_, Arc<AppState>>,
    date: String,
    markdown: String,
    expected_revision: Option<i64>,
) -> Result<Option<SaveReceipt>, AppError> {
    with_db(state, "save_diary", move |db| {
        service::save_diary(db, &date, &markdown, expected_revision)
    })
    .await
}

#[tauri::command]
async fn list_diary_dates(
    state: State<'_, Arc<AppState>>,
    year: i32,
    month: u32,
) -> Result<Vec<String>, AppError> {
    with_db(state, "list_diary_dates", move |db| {
        service::list_diary_dates(db, year, month)
    })
    .await
}

#[tauri::command]
async fn create_web_link(
    state: State<'_, Arc<AppState>>,
    input: CreateWebLinkInput,
    allow_duplicate: Option<bool>,
) -> Result<WebLinkDetail, AppError> {
    with_db(state, "create_web_link", move |db| {
        service::create_web_link(db, input, allow_duplicate.unwrap_or(false))
    })
    .await
}

#[tauri::command]
async fn update_web_link(
    state: State<'_, Arc<AppState>>,
    id: String,
    patch: UpdateWebLinkPatch,
    allow_duplicate: Option<bool>,
) -> Result<WebLinkDetail, AppError> {
    with_db(state, "update_web_link", move |db| {
        service::update_web_link(db, &id, patch, allow_duplicate.unwrap_or(false))
    })
    .await
}

#[tauri::command]
async fn find_web_link_duplicates(
    state: State<'_, Arc<AppState>>,
    url: String,
    exclude_id: Option<String>,
) -> Result<Vec<WebLinkSummary>, AppError> {
    with_db(state, "find_web_link_duplicates", move |db| {
        service::find_web_link_duplicates(db, &url, exclude_id.as_deref())
    })
    .await
}

#[tauri::command]
async fn list_web_links(
    state: State<'_, Arc<AppState>>,
    cursor: Option<String>,
    sort: Option<CollectionSort>,
    query: Option<String>,
) -> Result<ResultPage<WebLinkSummary>, AppError> {
    with_db(state, "list_web_links", move |db| {
        service::list_web_links(db, cursor.as_deref(), sort.unwrap_or_default(), query.as_deref())
    })
    .await
}

#[tauri::command]
async fn get_web_link(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<WebLinkDetail, AppError> {
    with_db(state, "get_web_link", move |db| {
        service::get_web_link(db, &id)
    })
    .await
}

#[tauri::command]
async fn get_web_link_preview(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<WebLinkPreview, AppError> {
    with_db(state, "get_web_link_preview", move |db| {
        service::get_web_link_preview(db, &id)
    })
    .await
}

#[tauri::command]
async fn fetch_web_link_metadata(url: String) -> Result<WebLinkMetadata, AppError> {
    web_metadata::fetch(&url).await
}

#[tauri::command]
async fn get_web_link_thumbnail(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Option<String>, AppError> {
    with_db(state, "get_web_link_thumbnail", move |db| service::get_web_link_thumbnail(db, &id)).await
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), AppError> {
    run_blocking("open_external_url", move || {
        service::open_external_url(&url)
    })
    .await
}

#[tauri::command]
async fn create_page(
    state: State<'_, Arc<AppState>>,
    web_link_id: String,
    title: String,
) -> Result<PageView, AppError> {
    with_db(state, "create_page", move |db| {
        service::create_page(db, &web_link_id, &title)
    })
    .await
}

#[tauri::command]
async fn create_knowledge_page(
    state: State<'_, Arc<AppState>>,
    title: String,
    body_markdown: Option<String>,
) -> Result<PageView, AppError> {
    with_db(state, "create_knowledge_page", move |db| {
        service::create_knowledge_page(db, &title, body_markdown.as_deref())
    })
    .await
}

#[tauri::command]
async fn list_knowledge_pages(
    state: State<'_, Arc<AppState>>,
    cursor: Option<String>,
    sort: Option<CollectionSort>,
    query: Option<String>,
) -> Result<ResultPage<KnowledgePageSummary>, AppError> {
    with_db(state, "list_knowledge_pages", move |db| {
        service::list_knowledge_pages(db, cursor.as_deref(), sort.unwrap_or_default(), query.as_deref())
    })
    .await
}

#[tauri::command]
async fn get_page(state: State<'_, Arc<AppState>>, id: String) -> Result<PageView, AppError> {
    with_db(state, "get_page", move |db| service::get_page(db, &id)).await
}

#[tauri::command]
async fn get_page_preview(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Option<String>, AppError> {
    with_db(state, "get_page_preview", move |db| {
        service::get_page_preview(db, &id)
    })
    .await
}

#[tauri::command]
async fn save_page(
    state: State<'_, Arc<AppState>>,
    id: String,
    markdown: String,
    expected_revision: i64,
) -> Result<SaveReceipt, AppError> {
    with_db(state, "save_page", move |db| {
        service::save_page(db, &id, &markdown, expected_revision)
    })
    .await
}

#[tauri::command]
async fn rename_page(
    state: State<'_, Arc<AppState>>,
    id: String,
    title: String,
) -> Result<PageView, AppError> {
    with_db(state, "rename_page", move |db| {
        service::rename_page(db, &id, &title)
    })
    .await
}

#[tauri::command]
async fn reorder_pages(
    state: State<'_, Arc<AppState>>,
    web_link_id: String,
    page_ids: Vec<String>,
) -> Result<(), AppError> {
    with_db(state, "reorder_pages", move |db| {
        service::reorder_pages(db, &web_link_id, page_ids)
    })
    .await
}

#[tauri::command]
async fn list_tags(
    state: State<'_, Arc<AppState>>,
    query: Option<String>,
) -> Result<Vec<TagSummary>, AppError> {
    with_db(state, "list_tags", move |db| {
        service::list_tags(db, query.as_deref())
    })
    .await
}

#[tauri::command]
async fn create_tag(state: State<'_, Arc<AppState>>, name: String) -> Result<Tag, AppError> {
    with_db(state, "create_tag", move |db| {
        service::create_tag(db, &name)
    })
    .await
}

#[tauri::command]
async fn rename_tag(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> Result<Tag, AppError> {
    with_db(state, "rename_tag", move |db| {
        service::rename_tag(db, &id, &name)
    })
    .await
}

#[tauri::command]
async fn delete_tag(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    with_db(state, "delete_tag", move |db| service::delete_tag(db, &id)).await
}

#[tauri::command]
async fn set_web_link_tags(
    state: State<'_, Arc<AppState>>,
    id: String,
    tag_ids: Vec<String>,
) -> Result<(), AppError> {
    with_db(state, "set_web_link_tags", move |db| {
        service::set_web_link_tags(db, &id, tag_ids)
    })
    .await
}

#[tauri::command]
async fn set_page_tags(
    state: State<'_, Arc<AppState>>,
    id: String,
    tag_ids: Vec<String>,
) -> Result<(), AppError> {
    with_db(state, "set_page_tags", move |db| {
        service::set_page_tags(db, &id, tag_ids)
    })
    .await
}

#[tauri::command]
async fn search(
    state: State<'_, Arc<AppState>>,
    query: String,
    kind: Option<SearchKind>,
    tag_ids: Option<Vec<String>>,
    sort: Option<SearchSort>,
    cursor: Option<String>,
) -> Result<ResultPage<SearchHit>, AppError> {
    with_db(state, "search", move |db| {
        service::search(db, &query, kind, tag_ids, sort, cursor.as_deref())
    })
    .await
}

#[tauri::command]
async fn soft_delete_web_link(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    with_db(state, "soft_delete_web_link", move |db| {
        service::soft_delete_web_link(db, &id)
    })
    .await
}

#[tauri::command]
async fn restore_web_link(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    with_db(state, "restore_web_link", move |db| {
        service::restore_web_link(db, &id)
    })
    .await
}

#[tauri::command]
async fn soft_delete_page(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    with_db(state, "soft_delete_page", move |db| {
        service::soft_delete_page(db, &id)
    })
    .await
}

#[tauri::command]
async fn restore_page(state: State<'_, Arc<AppState>>, id: String) -> Result<(), AppError> {
    with_db(state, "restore_page", move |db| {
        service::restore_page(db, &id)
    })
    .await
}

#[tauri::command]
async fn list_trash(state: State<'_, Arc<AppState>>) -> Result<Vec<TrashEntry>, AppError> {
    with_db(state, "list_trash", move |db| service::list_trash(db)).await
}

#[tauri::command]
async fn create_backup(
    state: State<'_, Arc<AppState>>,
    destination: Option<String>,
) -> Result<BackupInfo, AppError> {
    with_db(state, "create_backup", move |db| {
        db.create_backup(destination.as_deref())
    })
    .await
}

#[tauri::command]
async fn list_backups(state: State<'_, Arc<AppState>>) -> Result<Vec<BackupInfo>, AppError> {
    with_db(state, "list_backups", move |db| db.list_backups()).await
}

#[tauri::command]
async fn restore_backup(
    state: State<'_, Arc<AppState>>,
    source: String,
) -> Result<RestoreInfo, AppError> {
    with_db(state, "restore_backup", move |db| {
        db.restore_backup(&source)
    })
    .await
}

#[tauri::command]
async fn export_page_markdown(
    state: State<'_, Arc<AppState>>,
    id: String,
    destination: String,
) -> Result<(), AppError> {
    with_db(state, "export_page_markdown", move |db| {
        service::export_page_markdown(db, &id, &destination)
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_local_data_dir()
                .map_err(|e| format!("ローカルデータディレクトリを解決できません: {e}"))?;

            app.manage(Arc::new(AppState {
                data_dir,
                db: Mutex::new(None),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            rebuild_search_index,
            get_diary,
            save_diary,
            list_diary_dates,
            create_web_link,
            update_web_link,
            find_web_link_duplicates,
            list_web_links,
            get_web_link,
            get_web_link_preview,
            fetch_web_link_metadata,
            get_web_link_thumbnail,
            open_external_url,
            create_page,
            create_knowledge_page,
            list_knowledge_pages,
            get_page,
            get_page_preview,
            save_page,
            rename_page,
            reorder_pages,
            list_tags,
            create_tag,
            rename_tag,
            delete_tag,
            set_web_link_tags,
            set_page_tags,
            search,
            soft_delete_web_link,
            restore_web_link,
            soft_delete_page,
            restore_page,
            list_trash,
            create_backup,
            list_backups,
            restore_backup,
            export_page_markdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Oboromi application");
}
