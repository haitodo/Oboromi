use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use rusqlite::{
    backup::{Backup, StepResult},
    Connection, OpenFlags, OptionalExtension, Transaction,
};
use uuid::Uuid;

use crate::error::AppError;
use crate::markdown::extract_search_text;
use crate::models::{AppStatus, BackupInfo, RestoreInfo};
use crate::normalize::{
    display_diary_title, display_page_title, display_web_link_title, normalize_search_key,
    now_utc_ms,
};

pub const OBOROMI_APPLICATION_ID: i32 = 0x4F42_524D; // "OBRM"
pub const OBOROMI_USER_VERSION: i32 = 3;
const CORE_TABLES: [&str; 5] = ["web_links", "pages", "tags", "web_link_tags", "page_tags"];

// Fetch match fields and ordering keys for an FTS row without reading its body.
const SEARCH_MATCH_INDEX_SQL: &str = "
CREATE INDEX idx_search_match ON search_documents(
    id, kind, target_id, updated_at_ms, title_key, parent_title_key, url_text_key, description_key
);";

pub const SEARCH_SCHEMA_SQL: &str = r#"
CREATE TABLE search_documents (
    id               INTEGER PRIMARY KEY,
    target_id        TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('web_link', 'attached_page', 'knowledge_page', 'diary')),
    title_key        TEXT NOT NULL,
    host_key         TEXT NOT NULL DEFAULT '',
    url_text_key     TEXT NOT NULL DEFAULT '',
    description_key  TEXT NOT NULL DEFAULT '',
    body_key         TEXT NOT NULL DEFAULT '',
    parent_title_key TEXT NOT NULL DEFAULT '',
    updated_at_ms    INTEGER NOT NULL,
    UNIQUE (kind, target_id)
);
CREATE INDEX idx_search_title ON search_documents(title_key);
CREATE INDEX idx_search_host
    ON search_documents(kind, host_key) WHERE kind = 'web_link';
CREATE INDEX idx_search_updated ON search_documents(updated_at_ms DESC, kind, target_id);
CREATE INDEX idx_search_kind_updated
    ON search_documents(kind, updated_at_ms DESC, target_id);

CREATE VIRTUAL TABLE search_fts USING fts5(
    title_key, url_text_key, description_key, body_key, parent_title_key,
    content='search_documents',
    content_rowid='id',
    tokenize='trigram case_sensitive 1'
);

CREATE TRIGGER search_documents_ai AFTER INSERT ON search_documents BEGIN
    INSERT INTO search_fts(rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES (new.id, new.title_key, new.url_text_key, new.description_key, new.body_key, new.parent_title_key);
END;

CREATE TRIGGER search_documents_ad AFTER DELETE ON search_documents BEGIN
    INSERT INTO search_fts(search_fts, rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES ('delete', old.id, old.title_key, old.url_text_key, old.description_key, old.body_key, old.parent_title_key);
END;

CREATE TRIGGER search_documents_au AFTER UPDATE ON search_documents
WHEN old.title_key <> new.title_key
    OR old.url_text_key <> new.url_text_key
    OR old.description_key <> new.description_key
    OR old.body_key <> new.body_key
    OR old.parent_title_key <> new.parent_title_key
BEGIN
    INSERT INTO search_fts(search_fts, rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES ('delete', old.id, old.title_key, old.url_text_key, old.description_key, old.body_key, old.parent_title_key);
    INSERT INTO search_fts(rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES (new.id, new.title_key, new.url_text_key, new.description_key, new.body_key, new.parent_title_key);
END;
"#;

const LIBRARY_SCHEMA_SQL: &str = r#"
CREATE TABLE web_links (
    id             TEXT PRIMARY KEY NOT NULL,
    url            TEXT NOT NULL,
    normalized_url TEXT NOT NULL,
    title          TEXT NOT NULL DEFAULT '',
    description    TEXT NOT NULL DEFAULT '',
    created_at_ms  INTEGER NOT NULL,
    updated_at_ms  INTEGER NOT NULL,
    deleted_at_ms  INTEGER
);
CREATE INDEX idx_web_links_active_url
    ON web_links(normalized_url) WHERE deleted_at_ms IS NULL;
CREATE INDEX idx_web_links_active_updated
    ON web_links(updated_at_ms DESC, id) WHERE deleted_at_ms IS NULL;

CREATE TABLE pages (
    id            TEXT PRIMARY KEY NOT NULL,
    web_link_id   TEXT REFERENCES web_links(id) ON DELETE CASCADE,
    diary_date    TEXT UNIQUE
        CHECK (diary_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    title         TEXT NOT NULL DEFAULT '',
    body_markdown TEXT NOT NULL DEFAULT '',
    position      INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    revision      INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    deleted_at_ms INTEGER,
    CHECK (
        (web_link_id IS NOT NULL AND diary_date IS NULL)
        OR (web_link_id IS NULL AND diary_date IS NOT NULL
            AND title = '' AND position = 0 AND deleted_at_ms IS NULL)
        OR (web_link_id IS NULL AND diary_date IS NULL AND position = 0)
    )
);
CREATE INDEX idx_pages_web_link_position ON pages(web_link_id, position, id);
CREATE INDEX idx_pages_knowledge_active_updated
    ON pages(updated_at_ms DESC, id)
    WHERE web_link_id IS NULL AND diary_date IS NULL AND deleted_at_ms IS NULL;

CREATE TABLE tags (
    id       TEXT PRIMARY KEY NOT NULL,
    name     TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE CHECK (name_key <> '')
);
CREATE TABLE web_link_tags (
    web_link_id TEXT NOT NULL REFERENCES web_links(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (web_link_id, tag_id)
);
CREATE INDEX idx_web_link_tags_tag ON web_link_tags(tag_id, web_link_id);

CREATE TABLE page_tags (
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (page_id, tag_id)
);
CREATE INDEX idx_page_tags_tag ON page_tags(tag_id, page_id);
"#;

pub struct DbManager {
    data_dir: PathBuf,
    db_path: PathBuf,
    backups_dir: PathBuf,
    maintenance_lock: RwLock<()>,
    writer: Mutex<Option<Connection>>,
    reader: Mutex<Option<Connection>>,
    search_reader: Mutex<Option<Connection>>,
    startup_error: RwLock<Option<String>>,
    _instance_lock: Option<File>,
}

impl DbManager {
    /// Open or initialize a library in `data_dir`.
    /// When `acquire_instance_lock` is true, holds an exclusive lock on `instance.lock`.
    pub fn open(data_dir: PathBuf, acquire_instance_lock: bool) -> Result<Self, AppError> {
        fs::create_dir_all(&data_dir)?;
        let backups_dir = data_dir.join("backups");
        fs::create_dir_all(&backups_dir)?;

        let instance_lock = if acquire_instance_lock {
            Some(Self::acquire_lock(&data_dir.join("instance.lock"))?)
        } else {
            None
        };

        let db_path = data_dir.join("library.sqlite3");

        let manager = Self {
            data_dir,
            db_path,
            backups_dir,
            maintenance_lock: RwLock::new(()),
            writer: Mutex::new(None),
            reader: Mutex::new(None),
            search_reader: Mutex::new(None),
            startup_error: RwLock::new(None),
            _instance_lock: instance_lock,
        };

        if let Err(err) = manager.open_connections(true) {
            let msg = format!(
                "データベースの初期化に失敗しました ({}): {err}",
                manager.db_path.display()
            );
            *manager.startup_error.write().unwrap() = Some(msg);
        }

        Ok(manager)
    }

    #[cfg(windows)]
    fn acquire_lock(lock_path: &Path) -> Result<File, AppError> {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .share_mode(0)
            .open(lock_path)
            .map_err(|e| {
                AppError::StorageError(format!(
                    "Oboromi の別インスタンスがすでに起動しているか、ロックファイルを開けません: {e}"
                ))
            })
    }

    #[cfg(not(windows))]
    fn acquire_lock(lock_path: &Path) -> Result<File, AppError> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| AppError::StorageError(format!("ロックファイルを開けません: {e}")))
    }

    fn configure_writer(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;",
        )?;
        Ok(())
    }

    fn configure_reader(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA query_only = ON;
             PRAGMA mmap_size = 268435456;",
        )?;
        Ok(())
    }

    fn open_connections(&self, initialize: bool) -> Result<(), AppError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut writer = Connection::open_with_flags(
            &self.db_path,
            if initialize { flags | OpenFlags::SQLITE_OPEN_CREATE } else { flags },
        )?;
        writer.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        Self::initialize_or_validate(&mut writer)?;
        Self::configure_writer(&writer)?;

        let reader = self.open_reader()?;
        let search_reader = self.open_reader()?;
        *self.writer.lock().unwrap() = Some(writer);
        *self.reader.lock().unwrap() = Some(reader);
        *self.search_reader.lock().unwrap() = Some(search_reader);
        *self.startup_error.write().unwrap() = None;
        Ok(())
    }

    fn open_reader(&self) -> Result<Connection, AppError> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_reader(&conn)?;
        Ok(conn)
    }

    fn initialize_or_validate(conn: &mut Connection) -> Result<(), AppError> {
        let (application_id, version) = Self::read_header(conn)?;
        if application_id == 0 && version == 0 {
            let has_objects = conn.prepare(
                "SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' LIMIT 1",
            )?.exists([])?;
            if !has_objects {
                let tx = conn.transaction()?;
                tx.execute_batch(LIBRARY_SCHEMA_SQL)?;
                create_search_schema(&tx)?;
                tx.pragma_update(None, "application_id", OBOROMI_APPLICATION_ID)?;
                tx.pragma_update(None, "user_version", OBOROMI_USER_VERSION)?;
                tx.commit()?;
                return Ok(());
            }
        }

        Self::validate_header(application_id, version)?;
        Self::require_tables(conn, &CORE_TABLES)?;
        Self::require_tables(conn, &["search_documents", "search_fts"])
    }

    fn read_header(conn: &Connection) -> Result<(i32, i32), AppError> {
        Ok((
            conn.pragma_query_value(None, "application_id", |row| row.get(0))?,
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?,
        ))
    }

    fn validate_header(application_id: i32, version: i32) -> Result<(), AppError> {
        if application_id != OBOROMI_APPLICATION_ID {
            return Err(AppError::IncompatibleBackup("Oboromiのデータベース形式ではありません".into()));
        }
        if version != OBOROMI_USER_VERSION {
            return Err(AppError::IncompatibleBackup(format!(
                "このバージョンのOboromiでは開けないデータベースです (user_version={version})"
            )));
        }
        Ok(())
    }

    fn require_tables(conn: &Connection, tables: &[&str]) -> Result<(), AppError> {
        let mut stmt = conn.prepare(
            "SELECT 1 FROM sqlite_master WHERE name = ?1 AND type = 'table'",
        )?;
        for table in tables {
            if !stmt.exists([table])? {
                return Err(AppError::IncompatibleBackup(format!(
                    "必要なテーブル {table} が見つかりません"
                )));
            }
        }
        Ok(())
    }

    pub fn status(&self) -> AppStatus {
        AppStatus {
            data_dir: self.data_dir.to_string_lossy().into_owned(),
            db_path: self.db_path.to_string_lossy().into_owned(),
            backups_dir: self.backups_dir.to_string_lossy().into_owned(),
            startup_error: self.startup_error.read().unwrap().clone(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    fn ensure_healthy(&self) -> Result<(), AppError> {
        if let Some(err) = self.startup_error.read().unwrap().as_ref() {
            return Err(AppError::StorageError(err.clone()));
        }
        Ok(())
    }

    pub fn with_reader<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError>,
    {
        self.read_snapshot(&self.reader, f)
    }

    pub fn with_search_reader<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError>,
    {
        self.read_snapshot(&self.search_reader, f)
    }

    fn read_snapshot<F, R>(&self, reader: &Mutex<Option<Connection>>, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError>,
    {
        let _guard = self.maintenance_lock.read().unwrap();
        self.ensure_healthy()?;
        let mut guard = reader.lock().unwrap();
        let conn = guard.as_mut().ok_or_else(|| {
            AppError::StorageError("読み取り用データベース接続が利用できません".into())
        })?;
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn with_writer<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&mut Connection) -> Result<R, AppError>,
    {
        let _guard = self.maintenance_lock.read().unwrap();
        self.ensure_healthy()?;
        let mut guard = self.writer.lock().unwrap();
        let conn = guard.as_mut().ok_or_else(|| {
            AppError::StorageError("書き込み用データベース接続が利用できません".into())
        })?;
        f(conn)
    }

    pub fn with_transaction<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Transaction<'_>) -> Result<R, AppError>,
    {
        self.with_writer(|conn| {
            let tx = conn.transaction()?;
            let result = f(&tx)?;
            tx.commit()?;
            Ok(result)
        })
    }

    pub fn rebuild_search_index(&self) -> Result<i64, AppError> {
        let _maint = self.maintenance_lock.write().unwrap();
        self.ensure_healthy()?;
        let mut guard = self.writer.lock().unwrap();
        let conn = guard.as_mut().ok_or_else(|| {
            AppError::StorageError("書き込み用データベース接続が利用できません".into())
        })?;
        let tx = conn.transaction()?;
        let count = rebuild_search_index_tx(&tx)?;
        tx.commit()?;
        Ok(count)
    }

    pub fn create_backup(&self, destination: Option<&str>) -> Result<BackupInfo, AppError> {
        let _maint = self.maintenance_lock.write().unwrap();
        self.ensure_healthy()?;

        let dest_path = match destination.map(str::trim).filter(|s| !s.is_empty()) {
            Some(path_str) => PathBuf::from(path_str),
            None => self.backups_dir.join(backup_name("oboromi-backup")),
        };

        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        if Self::paths_refer_to_same_location(&dest_path, &self.db_path) {
            return Err(AppError::Validation(
                "現在使用中のデータベースと同じ場所にはバックアップを作成できません".into(),
            ));
        }

        if dest_path.exists() {
            return Err(AppError::Validation(format!(
                "既存のファイルには上書きできません: {}",
                dest_path.display()
            )));
        }

        let guard = self.writer.lock().unwrap();
        let src_conn = guard.as_ref().ok_or_else(|| {
            AppError::StorageError("書き込み用データベース接続が利用できません".into())
        })?;

        Self::backup_connection_to_single_file(src_conn, &dest_path)?;

        let metadata = fs::metadata(&dest_path)?;
        let file_name = dest_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(BackupInfo {
            file_name,
            path: dest_path.to_string_lossy().into_owned(),
            size_bytes: metadata.len(),
            created_at_ms: now_utc_ms(),
        })
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>, AppError> {
        let mut items = Vec::new();
        if !self.backups_dir.exists() {
            return Ok(items);
        }

        for entry in fs::read_dir(&self.backups_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("sqlite3"));
            if !ext_ok {
                continue;
            }
            let meta = entry.metadata()?;
            let created_at_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            items.push(BackupInfo {
                file_name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                size_bytes: meta.len(),
                created_at_ms,
            });
        }

        items.sort_by(|a, b| {
            b.created_at_ms
                .cmp(&a.created_at_ms)
                .then_with(|| b.file_name.cmp(&a.file_name))
        });
        Ok(items)
    }

    pub fn restore_backup(&self, source: &str) -> Result<RestoreInfo, AppError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(AppError::Validation(
                "復元元のバックアップファイルを指定してください".into(),
            ));
        }

        let source_path = PathBuf::from(trimmed);
        if !source_path.is_file() {
            return Err(AppError::IncompatibleBackup(format!(
                "バックアップファイルが見つかりません: {}",
                source_path.display()
            )));
        }

        if Self::paths_refer_to_same_location(&source_path, &self.db_path) {
            return Err(AppError::Validation(
                "現在使用中のデータベース自体は復元元に指定できません".into(),
            ));
        }

        let _maint = self.maintenance_lock.write().unwrap();

        // Step 1: Copy source read-only into a temporary working DB and validate thoroughly
        let work_path = self
            .data_dir
            .join(format!(".restore-work-{}.sqlite3", Uuid::now_v7().simple()));
        let _work_cleanup = TempFileGuard::new(work_path.clone());

        let (work_conn, rebuilt_documents) =
            Self::prepare_and_validate_working_db(&source_path, &work_path, &self.db_path)?;

        // Step 2: Back up the current DB before touching it (Spec 10.3 step 3)
        let pre_restore_path = self.backups_dir.join(backup_name("pre-restore"));

        {
            let writer_guard = self.writer.lock().unwrap();
            if let Some(current_conn) = writer_guard.as_ref() {
                Self::backup_connection_to_single_file(current_conn, &pre_restore_path).map_err(
                    |e| {
                        AppError::StorageError(format!(
                            "復元前の退避バックアップ作成に失敗したため、復元を中止しました: {e}"
                        ))
                    },
                )?;
            } else if self.db_path.exists() {
                fs::copy(&self.db_path, &pre_restore_path).map_err(|e| {
                    AppError::StorageError(format!(
                        "復元前の退避コピー作成に失敗したため、復元を中止しました: {e}"
                    ))
                })?;
            }
        }

        // Step 3: Close normal connections, apply validated working DB via Online Backup API
        {
            *self.reader.lock().unwrap() = None;
            *self.search_reader.lock().unwrap() = None;
            *self.writer.lock().unwrap() = None;
        }

        let apply_res = (|| -> Result<(), AppError> {
            let mut target_conn = Connection::open_with_flags(
                &self.db_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            Self::configure_writer(&target_conn)?;

            copy_database(&work_conn, &mut target_conn)?;
            Ok(())
        })();

        drop(work_conn);

        // Step 4: Reopen normal connections regardless, so state stays consistent or reports startup_error
        let reopen_res = self.open_connections(false);

        if let Err(apply_err) = apply_res {
            if let Err(reopen_err) = reopen_res {
                let msg = format!(
                    "復元に失敗し、再接続にも失敗しました。退避先: {} / エラー: {apply_err} / {reopen_err}",
                    pre_restore_path.display()
                );
                *self.startup_error.write().unwrap() = Some(msg.clone());
                return Err(AppError::StorageError(msg));
            }
            return Err(AppError::StorageError(format!(
                "復元の反映に失敗しました（元のデータベースは保持されています。退避先: {}）: {apply_err}",
                pre_restore_path.display()
            )));
        }

        if let Err(reopen_err) = reopen_res {
            let msg = format!(
                "復元後の再読み込みに失敗しました。退避先: {} / エラー: {reopen_err}",
                pre_restore_path.display()
            );
            *self.startup_error.write().unwrap() = Some(msg.clone());
            return Err(AppError::StorageError(msg));
        }

        Ok(RestoreInfo {
            source: source_path.to_string_lossy().into_owned(),
            pre_restore_backup_path: pre_restore_path.to_string_lossy().into_owned(),
            rebuilt_documents,
        })
    }

    fn prepare_and_validate_working_db(
        source_path: &Path,
        work_path: &Path,
        current_db_path: &Path,
    ) -> Result<(Connection, i64), AppError> {
        let src_conn = Connection::open_with_flags(
            source_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            AppError::IncompatibleBackup(format!("バックアップファイルを開けません: {e}"))
        })?;

        let mut work_conn = Connection::open_with_flags(
            work_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        // Match destination page size if current DB is readable
        if current_db_path.is_file() {
            if let Ok(cur) = Connection::open_with_flags(
                current_db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                if let (Ok(cur_ps), Ok(src_ps)) = (
                    cur.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0)),
                    src_conn.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0)),
                ) {
                    if cur_ps != src_ps {
                        return Err(AppError::IncompatibleBackup(format!(
                            "ページサイズが互換ではありません (現在: {cur_ps}, バックアップ: {src_ps})"
                        )));
                    }
                }
            }
        }

        copy_database(&src_conn, &mut work_conn).map_err(|e| {
            AppError::IncompatibleBackup(format!("SQLiteバックアップの取り込みに失敗しました: {e}"))
        })?;
        drop(src_conn);

        work_conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        let (application_id, version) = Self::read_header(&work_conn)
            .map_err(|e| AppError::IncompatibleBackup(format!("ヘッダ検証に失敗しました: {e}")))?;
        Self::validate_header(application_id, version)?;
        Self::require_tables(&work_conn, &CORE_TABLES)?;

        let integrity_rows: Vec<String> = {
            let mut stmt = work_conn.prepare("PRAGMA integrity_check")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if integrity_rows != vec!["ok".to_string()] {
            return Err(AppError::IncompatibleBackup(format!(
                "整合性チェックに失敗しました: {}",
                integrity_rows.join("; ")
            )));
        }

        let fk_errors: i64 = {
            let mut stmt = work_conn.prepare("PRAGMA foreign_key_check")?;
            let mut rows = stmt.query([])?;
            let mut count = 0;
            while rows.next()?.is_some() {
                count += 1;
            }
            count
        };
        if fk_errors > 0 {
            return Err(AppError::IncompatibleBackup(
                "外部キー制約チェックに失敗しました".into(),
            ));
        }

        let rebuilt_documents = {
            let tx = work_conn.transaction()?;
            let count = rebuild_search_index_tx(&tx)?;
            tx.commit()?;
            count
        };

        Ok((work_conn, rebuilt_documents))
    }

    fn backup_connection_to_single_file(
        src_conn: &Connection,
        dest_path: &Path,
    ) -> Result<(), AppError> {
        let parent = dest_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temp_path = tempfile::Builder::new()
            .prefix(".oboromi-backup-")
            .suffix(".sqlite3")
            .tempfile_in(parent)?
            .into_temp_path();

        {
            let mut dest_conn = Connection::open_with_flags(
                &temp_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            copy_database(src_conn, &mut dest_conn)?;
            dest_conn.execute_batch("PRAGMA journal_mode = DELETE;")?;
        }

        temp_path.persist_noclobber(dest_path).map_err(|err| err.error)?;
        Ok(())
    }

    fn paths_refer_to_same_location(a: &Path, b: &Path) -> bool {
        if let (Ok(ca), Ok(cb)) = (fs::canonicalize(a), fs::canonicalize(b)) {
            return ca == cb;
        }
        let abs_a = Self::normalize_abs_path(a);
        let abs_b = Self::normalize_abs_path(b);
        #[cfg(windows)]
        {
            abs_a
                .to_string_lossy()
                .eq_ignore_ascii_case(&abs_b.to_string_lossy())
        }
        #[cfg(not(windows))]
        {
            abs_a == abs_b
        }
    }

    fn normalize_abs_path(p: &Path) -> PathBuf {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
        if let (Some(parent), Some(name)) = (abs.parent(), abs.file_name()) {
            if let Ok(cp) = fs::canonicalize(parent) {
                return cp.join(name);
            }
        }
        abs
    }
}

fn backup_name(prefix: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let id = Uuid::now_v7().simple();
    format!("{prefix}-{timestamp}-{id}.sqlite3")
}

fn copy_database(source: &Connection, destination: &mut Connection) -> Result<(), AppError> {
    let backup = Backup::new(source, destination)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // Copy in one step so SQLite keeps a single source snapshot for the entire copy.
        if backup.step(-1)? == StepResult::Done {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(AppError::StorageError(
                "データベースが使用中のためコピーできませんでした".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct TempFileGuard(PathBuf);

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(PathBuf::from(format!("{}-wal", self.0.display())));
        let _ = fs::remove_file(PathBuf::from(format!("{}-shm", self.0.display())));
    }
}

/// Upsert or remove `search_documents` row for a WebLink inside an open transaction.
pub fn sync_web_link_search_doc(conn: &Connection, web_link_id: &str) -> Result<(), AppError> {
    let row: Option<(String, String, String, String, i64, Option<i64>)> = conn
        .query_row(
            "SELECT url, normalized_url, title, description, updated_at_ms, deleted_at_ms
             FROM web_links WHERE id = ?1",
            [web_link_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;

    match row {
        Some((url, normalized_url, title, description, updated_at_ms, None)) => {
            let host = url::Url::parse(&normalized_url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default();
            let display_title = display_web_link_title(&title, &host);
            let title_key = normalize_search_key(&display_title);
            let host_key = normalize_search_key(&host);
            let url_text_key = normalize_search_key(&url);
            let description_key = normalize_search_key(&description);

            conn.execute(
                "INSERT INTO search_documents (
                    target_id, kind, title_key, host_key, url_text_key,
                    description_key, body_key, parent_title_key, updated_at_ms
                 ) VALUES (?1, 'web_link', ?2, ?3, ?4, ?5, '', '', ?6)
                 ON CONFLICT(kind, target_id) DO UPDATE SET
                    title_key = excluded.title_key,
                    host_key = excluded.host_key,
                    url_text_key = excluded.url_text_key,
                    description_key = excluded.description_key,
                    body_key = excluded.body_key,
                    parent_title_key = excluded.parent_title_key,
                    updated_at_ms = excluded.updated_at_ms",
                rusqlite::params![
                    web_link_id,
                    title_key,
                    host_key,
                    url_text_key,
                    description_key,
                    updated_at_ms
                ],
            )?;
        }
        _ => {
            conn.execute(
                "DELETE FROM search_documents WHERE kind = 'web_link' AND target_id = ?1",
                [web_link_id],
            )?;
        }
    }

    Ok(())
}

/// Upsert or remove `search_documents` row for a Page (attached_page, knowledge_page, or diary).
pub fn sync_page_search_doc(conn: &Connection, page_id: &str) -> Result<(), AppError> {
    let row: Option<(
        Option<String>,
        Option<String>,
        String,
        String,
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<i64>,
    )> = conn
        .query_row(
            "SELECT p.web_link_id, p.diary_date, p.title, p.body_markdown, p.updated_at_ms, p.deleted_at_ms,
                    w.title, w.normalized_url, w.deleted_at_ms
             FROM pages p
             LEFT JOIN web_links w ON w.id = p.web_link_id
             WHERE p.id = ?1",
            [page_id],
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
        updated_at_ms,
        page_deleted,
        parent_title,
        parent_norm_url,
        parent_deleted,
    )) = row
    else {
        conn.execute(
            "DELETE FROM search_documents
             WHERE target_id = ?1 AND kind IN ('attached_page', 'knowledge_page', 'diary')",
            [page_id],
        )?;
        return Ok(());
    };

    if let Some(date) = diary_date {
        if page_deleted.is_some() {
            conn.execute(
                "DELETE FROM search_documents WHERE kind = 'diary' AND target_id = ?1",
                [page_id],
            )?;
            return Ok(());
        }
        let title_key = normalize_search_key(&display_diary_title(&date));
        let body_key = normalize_search_key(&extract_search_text(&body_markdown));
        conn.execute(
            "INSERT INTO search_documents (
                target_id, kind, title_key, host_key, url_text_key,
                description_key, body_key, parent_title_key, updated_at_ms
             ) VALUES (?1, 'diary', ?2, '', '', '', ?3, '', ?4)
             ON CONFLICT(kind, target_id) DO UPDATE SET
                title_key = excluded.title_key,
                host_key = excluded.host_key,
                url_text_key = excluded.url_text_key,
                description_key = excluded.description_key,
                body_key = excluded.body_key,
                parent_title_key = excluded.parent_title_key,
                updated_at_ms = excluded.updated_at_ms",
            rusqlite::params![page_id, title_key, body_key, updated_at_ms],
        )?;
        return Ok(());
    }

    if web_link_id.is_some() {
        let is_active = page_deleted.is_none() && parent_deleted.is_none();
        if !is_active {
            conn.execute(
                "DELETE FROM search_documents WHERE kind = 'attached_page' AND target_id = ?1",
                [page_id],
            )?;
            return Ok(());
        }

        let parent_host = parent_norm_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        let parent_disp =
            display_web_link_title(parent_title.as_deref().unwrap_or(""), &parent_host);
        let parent_title_key = normalize_search_key(&parent_disp);
        let title_key = normalize_search_key(&display_page_title(&title));
        let body_key = normalize_search_key(&extract_search_text(&body_markdown));

        conn.execute(
            "INSERT INTO search_documents (
                target_id, kind, title_key, host_key, url_text_key,
                description_key, body_key, parent_title_key, updated_at_ms
             ) VALUES (?1, 'attached_page', ?2, '', '', '', ?3, ?4, ?5)
             ON CONFLICT(kind, target_id) DO UPDATE SET
                title_key = excluded.title_key,
                host_key = excluded.host_key,
                url_text_key = excluded.url_text_key,
                description_key = excluded.description_key,
                body_key = excluded.body_key,
                parent_title_key = excluded.parent_title_key,
                updated_at_ms = excluded.updated_at_ms",
            rusqlite::params![page_id, title_key, body_key, parent_title_key, updated_at_ms],
        )?;
        return Ok(());
    }

    // Standalone knowledge page (web_link_id IS NULL AND diary_date IS NULL)
    if page_deleted.is_some() {
        conn.execute(
            "DELETE FROM search_documents WHERE kind = 'knowledge_page' AND target_id = ?1",
            [page_id],
        )?;
        return Ok(());
    }

    let title_key = normalize_search_key(&display_page_title(&title));
    let body_key = normalize_search_key(&extract_search_text(&body_markdown));

    conn.execute(
        "INSERT INTO search_documents (
            target_id, kind, title_key, host_key, url_text_key,
            description_key, body_key, parent_title_key, updated_at_ms
         ) VALUES (?1, 'knowledge_page', ?2, '', '', '', ?3, '', ?4)
         ON CONFLICT(kind, target_id) DO UPDATE SET
            title_key = excluded.title_key,
            host_key = excluded.host_key,
            url_text_key = excluded.url_text_key,
            description_key = excluded.description_key,
            body_key = excluded.body_key,
            parent_title_key = excluded.parent_title_key,
            updated_at_ms = excluded.updated_at_ms",
        rusqlite::params![page_id, title_key, body_key, updated_at_ms],
    )?;

    Ok(())
}

/// Sync all child attached pages of a given WebLink in `search_documents`.
pub fn sync_children_search_docs(conn: &Connection, web_link_id: &str) -> Result<(), AppError> {
    let mut stmt = conn.prepare("SELECT id FROM pages WHERE web_link_id = ?1")?;
    let ids: Vec<String> = stmt
        .query_map([web_link_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        sync_page_search_doc(conn, &id)?;
    }
    Ok(())
}

fn create_search_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SEARCH_SCHEMA_SQL)?;
    conn.execute_batch(SEARCH_MATCH_INDEX_SQL)?;
    Ok(())
}

/// Rebuild `search_documents`, `search_fts`, and triggers from primary tables in a transaction (Spec 6.3).
pub fn rebuild_search_index_tx(tx: &Transaction<'_>) -> Result<i64, AppError> {
    tx.execute_batch(
        "DROP TRIGGER IF EXISTS search_documents_ai;
         DROP TRIGGER IF EXISTS search_documents_ad;
         DROP TRIGGER IF EXISTS search_documents_au;
         DROP TABLE IF EXISTS search_fts;
         DROP TABLE IF EXISTS search_documents;",
    )?;
    create_search_schema(tx)?;

    let web_link_ids: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM web_links WHERE deleted_at_ms IS NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for id in &web_link_ids {
        sync_web_link_search_doc(tx, id)?;
    }

    let page_ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT p.id
             FROM pages p
             LEFT JOIN web_links w ON w.id = p.web_link_id
             WHERE p.deleted_at_ms IS NULL
               AND (p.web_link_id IS NULL OR w.deleted_at_ms IS NULL)",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for id in &page_ids {
        sync_page_search_doc(tx, id)?;
    }

    // FTS5 integrity check against external content table search_documents (Spec 6.3)
    tx.execute(
        "INSERT INTO search_fts(search_fts, rank) VALUES('integrity-check', 1)",
        [],
    )?;

    let total: i64 = tx.query_row("SELECT count(*) FROM search_documents", [], |r| r.get(0))?;
    Ok(total)
}
