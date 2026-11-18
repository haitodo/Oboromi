use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub name_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub id: String,
    pub name: String,
    pub name_key: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParentRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    pub id: String,
    pub web_link_id: String,
    pub title: String,
    pub display_title: String,
    pub position: i64,
    pub revision: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePageSummary {
    pub id: String,
    pub title: String,
    pub display_title: String,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebLinkSummary {
    pub id: String,
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub display_title: String,
    pub description: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebLinkDetail {
    pub id: String,
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub display_title: String,
    pub description: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub tags: Vec<Tag>,
    pub pages: Vec<PageSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebLinkPreview {
    pub url: String,
    pub description: String,
    pub page_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    pub id: String,
    pub kind: PageKind,
    pub title: String,
    pub display_title: String,
    pub body_markdown: String,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub tags: Vec<Tag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diary_date: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PageKind {
    #[serde(rename = "attached_page")]
    AttachedPage,
    #[serde(rename = "knowledge_page")]
    KnowledgePage,
    #[serde(rename = "diary")]
    Diary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveReceipt {
    pub page_id: String,
    pub revision: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchKind {
    #[serde(rename = "web_link")]
    WebLink,
    #[serde(rename = "attached_page")]
    AttachedPage,
    #[serde(rename = "knowledge_page")]
    KnowledgePage,
    #[serde(rename = "diary")]
    Diary,
}

impl SearchKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchKind::WebLink => "web_link",
            SearchKind::AttachedPage => "attached_page",
            SearchKind::KnowledgePage => "knowledge_page",
            SearchKind::Diary => "diary",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "web_link" => Some(SearchKind::WebLink),
            "attached_page" => Some(SearchKind::AttachedPage),
            "knowledge_page" => Some(SearchKind::KnowledgePage),
            "diary" => Some(SearchKind::Diary),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchSort {
    #[serde(rename = "relevance")]
    Relevance,
    #[serde(rename = "updated")]
    Updated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchedField {
    #[serde(rename = "title")]
    Title,
    #[serde(rename = "url")]
    Url,
    #[serde(rename = "tag")]
    Tag,
    #[serde(rename = "parentTitle")]
    ParentTitle,
    #[serde(rename = "description")]
    Description,
    #[serde(rename = "body")]
    Body,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub kind: SearchKind,
    pub title: String,
    pub updated_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diary_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_field: Option<MatchedField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResultPage<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebLinkInput {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebLinkPatch {
    pub url: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrashKind {
    #[serde(rename = "web_link")]
    WebLink,
    #[serde(rename = "attached_page")]
    AttachedPage,
    #[serde(rename = "knowledge_page")]
    KnowledgePage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub kind: TrashKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    pub deleted_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreInfo {
    pub source: String,
    pub pre_restore_backup_path: String,
    pub rebuilt_documents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub data_dir: String,
    pub db_path: String,
    pub backups_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_error: Option<String>,
}
