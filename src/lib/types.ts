export type PageKind = 'attached_page' | 'knowledge_page' | 'diary';
export type SearchKind = 'web_link' | 'attached_page' | 'knowledge_page' | 'diary';
export type SearchSort = 'relevance' | 'updated';
export type MatchedField =
  | 'title'
  | 'url'
  | 'tag'
  | 'parentTitle'
  | 'description'
  | 'body';

export interface Tag {
  id: string;
  name: string;
  nameKey: string;
}

export interface TagSummary {
  id: string;
  name: string;
  nameKey: string;
  count: number;
}

export interface ParentRef {
  id: string;
  title: string;
}

export interface PageSummary {
  id: string;
  webLinkId: string;
  title: string;
  displayTitle: string;
  position: number;
  revision: number;
  updatedAtMs: number;
}

export interface KnowledgePageSummary {
  id: string;
  title: string;
  displayTitle: string;
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface WebLinkSummary {
  id: string;
  url: string;
  normalizedUrl: string;
  title: string;
  displayTitle: string;
  description: string;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface WebLinkDetail {
  id: string;
  url: string;
  normalizedUrl: string;
  title: string;
  displayTitle: string;
  description: string;
  createdAtMs: number;
  updatedAtMs: number;
  tags: Tag[];
  pages: PageSummary[];
}

export interface WebLinkPreview {
  url: string;
  description: string;
  pageCount: number;
}

export interface PageView {
  id: string;
  kind: PageKind;
  title: string;
  displayTitle: string;
  bodyMarkdown: string;
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
  tags: Tag[];
  parent?: ParentRef;
  diaryDate?: string;
}

export interface SaveReceipt {
  pageId: string;
  revision: number;
  updatedAtMs: number;
}

export interface SearchHit {
  id: string;
  kind: SearchKind;
  title: string;
  updatedAtMs: number;
  url?: string;
  parent?: ParentRef;
  diaryDate?: string;
  matchedField?: MatchedField;
}

export interface ResultPage<T> {
  items: T[];
  nextCursor?: string;
}

export interface TrashEntry {
  id: string;
  kind: 'web_link' | 'attached_page' | 'knowledge_page';
  title: string;
  url?: string;
  parentId?: string;
  parentTitle?: string;
  deletedAtMs: number;
}

export interface BackupInfo {
  fileName: string;
  path: string;
  sizeBytes: number;
  createdAtMs: number;
}

export interface RestoreInfo {
  source: string;
  preRestoreBackupPath: string;
  rebuiltDocuments: number;
}

export interface AppStatus {
  dataDir: string;
  dbPath: string;
  backupsDir: string;
  startupError?: string;
}

export interface AppErrorPayload {
  code:
    | 'Validation'
    | 'NotFound'
    | 'DuplicateUrl'
    | 'RevisionConflict'
    | 'StorageError'
    | 'IncompatibleBackup'
    | 'Unknown';
  message: string;
  candidates?: WebLinkSummary[];
  currentRevision?: number;
  currentUpdatedAtMs?: number;
}
