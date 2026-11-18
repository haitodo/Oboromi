import { invoke } from '@tauri-apps/api/core';
import type {
  AppErrorPayload,
  AppStatus,
  BackupInfo,
  KnowledgePageSummary,
  PageView,
  RestoreInfo,
  ResultPage,
  SaveReceipt,
  SearchHit,
  SearchKind,
  SearchSort,
  Tag,
  TagSummary,
  TrashEntry,
  WebLinkDetail,
  WebLinkPreview,
  WebLinkSummary,
} from './types';

export class ApiError extends Error {
  readonly payload: AppErrorPayload;

  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.payload = payload;
  }
}

export function normalizeError(err: unknown): AppErrorPayload {
  if (err instanceof ApiError) {
    return err.payload;
  }
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    return err as AppErrorPayload;
  }
  if (typeof err === 'string') {
    try {
      const parsed = JSON.parse(err);
      if (parsed && typeof parsed === 'object' && 'code' in parsed && 'message' in parsed) {
        return parsed as AppErrorPayload;
      }
    } catch {
      // ignore JSON parse failure
    }
    return { code: 'Unknown', message: err };
  }
  if (err instanceof Error) {
    return { code: 'Unknown', message: err.message };
  }
  return { code: 'Unknown', message: '不明なエラーが発生しました' };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw new ApiError(normalizeError(err));
  }
}

interface FileDialogOptions {
  title: string;
  defaultPath?: string;
  filters: { name: string; extensions: string[] }[];
}

export const api = {
  getAppStatus: () => call<AppStatus>('get_app_status'),
  rebuildSearchIndex: () => call<number>('rebuild_search_index'),

  getDiary: (date: string) => call<PageView | null>('get_diary', { date }),
  saveDiary: (date: string, markdown: string, expectedRevision: number | null) =>
    call<SaveReceipt | null>('save_diary', { date, markdown, expectedRevision }),
  listDiaryDates: (year: number, month: number) =>
    call<string[]>('list_diary_dates', { year, month }),

  createWebLink: (
    input: { url: string; title: string; description: string },
    allowDuplicate = false
  ) => call<WebLinkDetail>('create_web_link', { input, allowDuplicate }),
  updateWebLink: (
    id: string,
    patch: { url?: string; title?: string; description?: string },
    allowDuplicate = false
  ) => call<WebLinkDetail>('update_web_link', { id, patch, allowDuplicate }),
  findWebLinkDuplicates: (url: string, excludeId?: string) =>
    call<WebLinkSummary[]>('find_web_link_duplicates', { url, excludeId }),
  listWebLinks: (cursor?: string) =>
    call<ResultPage<WebLinkSummary>>('list_web_links', { cursor }),
  getWebLink: (id: string) => call<WebLinkDetail>('get_web_link', { id }),
  getWebLinkPreview: (id: string) => call<WebLinkPreview>('get_web_link_preview', { id }),
  openExternalUrl: (url: string) => call<void>('open_external_url', { url }),

  createPage: (webLinkId: string, title: string) =>
    call<PageView>('create_page', { webLinkId, title }),
  createKnowledgePage: (title: string, bodyMarkdown?: string) =>
    call<PageView>('create_knowledge_page', { title, bodyMarkdown }),
  listKnowledgePages: (cursor?: string) =>
    call<ResultPage<KnowledgePageSummary>>('list_knowledge_pages', { cursor }),
  getPage: (id: string) => call<PageView>('get_page', { id }),
  getPagePreview: (id: string) => call<string | null>('get_page_preview', { id }),
  savePage: (id: string, markdown: string, expectedRevision: number) =>
    call<SaveReceipt>('save_page', { id, markdown, expectedRevision }),
  renamePage: (id: string, title: string) => call<PageView>('rename_page', { id, title }),
  reorderPages: (webLinkId: string, pageIds: string[]) =>
    call<void>('reorder_pages', { webLinkId, pageIds }),

  listTags: (query?: string) => call<TagSummary[]>('list_tags', { query }),
  createTag: (name: string) => call<Tag>('create_tag', { name }),
  renameTag: (id: string, name: string) => call<Tag>('rename_tag', { id, name }),
  deleteTag: (id: string) => call<void>('delete_tag', { id }),
  setWebLinkTags: (id: string, tagIds: string[]) =>
    call<void>('set_web_link_tags', { id, tagIds }),
  setPageTags: (id: string, tagIds: string[]) => call<void>('set_page_tags', { id, tagIds }),

  search: (
    query: string,
    kind?: SearchKind,
    tagIds?: string[],
    sort?: SearchSort,
    cursor?: string
  ) => call<ResultPage<SearchHit>>('search', { query, kind, tagIds, sort, cursor }),

  softDeleteWebLink: (id: string) => call<void>('soft_delete_web_link', { id }),
  restoreWebLink: (id: string) => call<void>('restore_web_link', { id }),
  softDeletePage: (id: string) => call<void>('soft_delete_page', { id }),
  restorePage: (id: string) => call<void>('restore_page', { id }),
  listTrash: () => call<TrashEntry[]>('list_trash'),

  createBackup: (destination?: string) => call<BackupInfo>('create_backup', { destination }),
  listBackups: () => call<BackupInfo[]>('list_backups'),
  restoreBackup: (source: string) => call<RestoreInfo>('restore_backup', { source }),
  exportPageMarkdown: (id: string, destination: string) =>
    call<void>('export_page_markdown', { id, destination }),
  pickFile: (options: FileDialogOptions) => call<string | null>('plugin:dialog|open', { options }),
  pickSavePath: (options: FileDialogOptions) => call<string | null>('plugin:dialog|save', { options }),
};
