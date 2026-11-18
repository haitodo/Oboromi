<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import logo from '../src-tauri/icons/128x128.png?url';
  import { api, normalizeError } from './lib/api';
  import { createCollection } from './lib/collection';
  import { currentJstDate, formatDiaryDate, formatDiaryWeekday, formatTimestamp, parseYmd, shiftYearMonth, shiftYmd } from './lib/jst';
  import { isShortQuery, searchKindOptions, searchKinds } from './lib/search';
  import CollectionList from './lib/CollectionList.svelte';
  import DiaryCalendar from './lib/DiaryCalendar.svelte';
  import DocumentEditor from './lib/DocumentEditor.svelte';
  import Icon from './lib/Icon.svelte';
  import KnowledgePageModal from './lib/KnowledgePageModal.svelte';
  import Modal from './lib/Modal.svelte';
  import PaletteModal from './lib/PaletteModal.svelte';
  import SettingsView from './lib/SettingsView.svelte';
  import TagEditor from './lib/TagEditor.svelte';
  import WebLinkModal from './lib/WebLinkModal.svelte';
  import type { AppStatus, PageView, SaveReceipt, SearchHit, SearchKind, SearchSort, TagSummary, WebLinkDetail } from './lib/types';

  type Section = 'diary' | 'web_links' | 'knowledge' | 'tags';
  type EditablePage = Omit<PageView, 'createdAtMs'>;
  type View =
    | { kind: 'empty' | 'settings' }
    | { kind: 'diary'; date: string; page: EditablePage | null }
    | { kind: 'page'; page: EditablePage }
    | { kind: 'web_link'; link: WebLinkDetail };
  type Target =
    | { kind: 'empty' | 'settings' }
    | { kind: 'diary'; date: string }
    | { kind: 'page' | 'web_link'; id: string };
  type Dialog = { kind: 'search' | 'knowledge' } | { kind: 'web_link'; link?: WebLinkDetail } | { kind: 'export'; id: string };

  const sections: Record<Section, { label: string; icon: string; tone: string }> = {
    diary: { label: '日記', icon: 'calendar', tone: 'diary' },
    web_links: { label: 'Webリンク', icon: 'link', tone: 'link' },
    knowledge: { label: 'ナレッジ', icon: 'note', tone: 'knowledge' },
    tags: { label: 'タグ', icon: 'tag', tone: 'tag' },
  };
  const sectionEntries = Object.entries(sections) as [Section, typeof sections[Section]][];

  let appStatus: AppStatus | null = null;
  let initializing = true;
  let startupError = '';
  let globalError = '';
  let section: Section = 'diary';
  let view: View = { kind: 'empty' };
  let beforeSettings: Target | undefined;
  let dialog: Dialog | null = null;
  let editor: DocumentEditor | null = null;
  let detailRequest = 0;
  let detailLoading = false;
  let retryDetail: (() => void) | null = null;
  let settingsBusy = false;
  let deleting = false;
  let theme: 'light' | 'dark' = 'light';

  let width = window.innerWidth;
  let drawerOpen = false;
  let drawerButton: HTMLButtonElement;
  let mobileDetail = false;
  let middlePane: HTMLElement;
  let detailPane: HTMLElement;
  let listScroll = 0;
  let listFocus: HTMLElement | null = null;
  let newMenu: HTMLDetailsElement | undefined;
  let detailMenu: HTMLDetailsElement | undefined;

  let today = currentJstDate();
  let selectedDate = today;
  let month = parseYmd(today);
  let diaryDates: string[] = [];
  let calendarRequest = 0;
  let calendarLoading = false;
  let calendarError = '';

  const links = createCollection(api.listWebLinks);
  const knowledge = createCollection(api.listKnowledgePages);
  let addingAttached = false;
  let attachedTitle = '';
  let reordering = false;
  let renameTitle: string | null = null;
  let renameInput: HTMLInputElement;
  let renaming = false;

  let allTags: TagSummary[] = [];
  let matchingTags: TagSummary[] = [];
  let tagsLoaded = false;
  let tagListLoading = false;
  let tagListError = '';
  let tagListRequest = 0;
  let matchingTagsRequest = 0;
  let tagQuery = '';
  let tagName = '';
  let tagRename: string | null = null;
  let tagError = '';
  let tagsBusy = false;
  let tagIds: string[] = [];
  let tagSearch = '';
  let tagKind: SearchKind | '' = '';
  let tagSort: SearchSort = 'relevance';
  let tagTimer: ReturnType<typeof setTimeout> | undefined;
  const tagResults = createCollection((cursor) => api.search(tagSearch, tagKind || undefined, tagIds, tagSort, cursor));

  let returnSearchFocus: HTMLElement | null = null;
  let exportPath = '';
  let exportMessage = '';
  let exportError = '';
  let exporting = false;
  let pickingExport = false;

  $: unavailable = !appStatus || Boolean(appStatus.startupError);
  $: blocked = unavailable || settingsBusy || deleting;
  $: isMultiPane = width >= 760;
  $: drawerVisible = drawerOpen && width < 1000;
  $: documentPage = view.kind === 'page' || view.kind === 'diary' ? view.page : null;
  $: documentKey = view.kind === 'diary' ? `diary:${view.date}` : view.kind === 'page' ? `page:${view.page.id}` : '';
  $: viewTitle = view.kind === 'diary' ? formatDiaryDate(view.date, false)
    : view.kind === 'page' ? view.page.displayTitle : view.kind === 'web_link' ? view.link.displayTitle : '';
  $: visibleTags = tagQuery.trim() ? matchingTags : allTags;
  $: singleTag = tagIds.length === 1 ? allTags.find((tag) => tag.id === tagIds[0]) : undefined;
  $: document.documentElement.dataset.theme = theme;

  onMount(() => {
    let stopped = false;
    let unlisten: (() => void) | undefined;
    let closing = false;
    try { theme = localStorage.getItem('oboromi-theme') === 'dark' ? 'dark' : 'light'; } catch { /* Use the default. */ }
    void initialize();
    try {
      void getCurrentWindow().onCloseRequested(async (event) => {
        if (closing || exporting || pickingExport || settingsBusy || deleting) {
          event.preventDefault();
          return;
        }
        closing = true;
        if (!(await flushEditor())) {
          closing = false;
          event.preventDefault();
        }
      }).then((remove) => {
        if (stopped) remove();
        else unlisten = remove;
      }).catch(() => {});
    } catch {
      // The app can also be rendered without a Tauri window.
    }
    return () => {
      stopped = true;
      unlisten?.();
      clearTimeout(tagTimer);
    };
  });

  function reportError(error: unknown) {
    globalError = normalizeError(error).message;
  }

  function changeTheme(next: 'light' | 'dark') {
    theme = next;
    try { localStorage.setItem('oboromi-theme', next); } catch { /* Keep the session preference. */ }
  }

  async function initialize() {
    initializing = true;
    startupError = '';
    try {
      appStatus = await api.getAppStatus();
      if (appStatus.startupError) {
        view = { kind: 'settings' };
        mobileDetail = true;
        return;
      }
      await Promise.all([
        navigate({ kind: 'diary', date: selectedDate }, 'diary', false),
        loadCalendar(),
      ]);
    } catch (error) {
      startupError = normalizeError(error).message;
    } finally {
      initializing = false;
    }
  }

  function flushEditor() {
    return editor?.flush() ?? Promise.resolve(true);
  }

  function currentTarget(): Target {
    switch (view.kind) {
      case 'diary': return { kind: 'diary', date: view.date };
      case 'web_link': return { kind: 'web_link', id: view.link.id };
      case 'page': return { kind: 'page', id: view.page.id };
      default: return { kind: view.kind };
    }
  }

  function isOpen(target: Target): boolean {
    const current = currentTarget();
    if (target.kind !== current.kind) return false;
    if ('id' in target) return 'id' in current && target.id === current.id;
    if ('date' in target) return 'date' in current && target.date === current.date;
    return true;
  }

  async function readView(target: Target): Promise<View> {
    switch (target.kind) {
      case 'diary': return { kind: 'diary', date: target.date, page: await api.getDiary(target.date) };
      case 'web_link': return { kind: 'web_link', link: await api.getWebLink(target.id) };
      case 'page': return { kind: 'page', page: await api.getPage(target.id) };
      default: return { kind: target.kind };
    }
  }

  async function navigate(target: Target, nextSection = section, showDetail = true, loaded?: View, focusHeading = Boolean(loaded)) {
    if (settingsBusy || deleting) return false;
    const request = ++detailRequest;
    const focus = document.activeElement as HTMLElement | null;
    try {
      if (!(await flushEditor()) || request !== detailRequest) return false;
      retryDetail = null;
      const same = isOpen(target);
      detailLoading = !loaded && !same && (target.kind === 'diary' || 'id' in target);
      const nextView = loaded ?? (same ? view : await readView(target));
      if (request !== detailRequest) return false;

      const switched = nextSection !== section;
      const openingFromList = !mobileDetail && showDetail;
      if (switched) {
        listScroll = 0;
        listFocus = null;
        middlePane.scrollTop = 0;
      } else if (openingFromList) {
        listScroll = middlePane.scrollTop;
        listFocus = focus && middlePane.contains(focus) ? focus : null;
      }
      section = nextSection;
      view = nextView;
      drawerOpen = false;
      mobileDetail = showDetail;
      if (!same) {
        detailPane.scrollTop = 0;
        attachedTitle = '';
        renameTitle = null;
      }

      if (nextView.kind === 'diary') {
        selectedDate = nextView.date;
        const nextMonth = parseYmd(nextView.date);
        if (nextSection === 'diary' && (nextMonth.year !== month.year || nextMonth.month !== month.month)) {
          moveCalendar(nextMonth);
        }
      }
      if (nextSection === 'web_links') links.ensureLoaded();
      if (nextSection === 'knowledge') knowledge.ensureLoaded();
      if (nextSection === 'tags' && switched) {
        ensureTags();
        void tagResults.load();
      }
      if (!isMultiPane && !showDetail) {
        await tick();
        if (request === detailRequest) middlePane.scrollTop = listScroll;
      }
      if (showDetail && (!isMultiPane || focusHeading)) {
        await tick();
        if (request === detailRequest) detailPane.querySelector<HTMLElement>(isMultiPane ? 'h1' : '.mobile-back-btn')?.focus();
      }
      return true;
    } catch (error) {
      if (request === detailRequest) {
        reportError(error);
        retryDetail = () => void navigate(target, nextSection, showDetail, loaded);
      }
      return false;
    } finally {
      if (request === detailRequest) detailLoading = false;
    }
  }

  async function switchSection(next: Section) {
    const target = next === section && view.kind !== 'settings' ? currentTarget()
      : next === 'diary' ? { kind: 'diary' as const, date: selectedDate } : { kind: 'empty' as const };
    await navigate(target, next, false);
  }

  async function showList() {
    if (view.kind === 'settings' && !unavailable) {
      await closeSettings();
      return;
    }
    if (await navigate(currentTarget(), section, false)) {
      listFocus?.focus();
    }
  }

  function openToday() {
    today = currentJstDate();
    closeMenus();
    return navigate({ kind: 'diary', date: today }, 'diary');
  }

  async function openSettings() {
    if (view.kind === 'settings') {
      drawerOpen = false;
      mobileDetail = true;
      return;
    }
    const previous = currentTarget();
    if (await navigate({ kind: 'settings' })) beforeSettings = previous;
  }

  function closeSettings() {
    return navigate(beforeSettings ?? (section === 'diary' ? { kind: 'diary', date: selectedDate } : { kind: 'empty' }), section, false);
  }

  async function loadCalendar() {
    const request = ++calendarRequest;
    calendarLoading = true;
    calendarError = '';
    try {
      const dates = await api.listDiaryDates(month.year, month.month);
      if (request === calendarRequest) diaryDates = dates;
    } catch (error) {
      if (request === calendarRequest) calendarError = normalizeError(error).message;
    } finally {
      if (request === calendarRequest) calendarLoading = false;
    }
  }

  function moveCalendar(next: { year: number; month: number }) {
    if (next.year < 1 || next.year > 9999) return;
    month = { ...next, day: 1 };
    diaryDates = [];
    void loadCalendar();
  }

  function refreshTagResults() {
    if (section !== 'tags') return;
    clearTimeout(tagTimer);
    void tagResults.load();
  }

  function updatePageAfterSave(receipt: SaveReceipt, markdown: string) {
    const saved = { id: receipt.pageId, bodyMarkdown: markdown, revision: receipt.revision, updatedAtMs: receipt.updatedAtMs };
    if (view.kind === 'diary') {
      const { date, page } = view;
      view = { ...view, page: { kind: 'diary', title: '', displayTitle: `${date}の日記`, diaryDate: date, tags: [], ...page, ...saved } };
      const dateParts = parseYmd(date);
      const hasEntry = Boolean(markdown.trim());
      if (dateParts.year === month.year && dateParts.month === month.month && hasEntry !== Boolean(page?.bodyMarkdown.trim())) {
        if (calendarLoading || calendarError) void loadCalendar();
        else diaryDates = hasEntry ? [...diaryDates, date].sort() : diaryDates.filter((day) => day !== date);
      }
    } else if (view.kind === 'page' && view.page.id === receipt.pageId) {
      view = { ...view, page: { ...view.page, ...saved } };
      if (view.page.kind === 'knowledge_page') void knowledge.load();
    }
    refreshTagResults();
  }

  function updateReloadedPage(page: PageView | null) {
    if (view.kind === 'diary') {
      view = { ...view, page };
      void loadCalendar();
    } else if (view.kind === 'page' && page?.id === view.page.id) {
      view = { kind: 'page', page };
    }
    refreshTagResults();
  }

  function openExternal(url: string) {
    void api.openExternalUrl(url).catch(reportError);
  }

  async function addAttachedPage() {
    if (view.kind !== 'web_link' || addingAttached) return;
    const id = view.link.id;
    addingAttached = true;
    try {
      const page = await api.createPage(id, attachedTitle);
      refreshTagResults();
      if (isOpen({ kind: 'web_link', id })) {
        attachedTitle = '';
        await navigate({ kind: 'page', id: page.id }, section, true, { kind: 'page', page });
      }
    } catch (error) {
      reportError(error);
    } finally {
      addingAttached = false;
    }
  }

  async function moveAttachedPage(index: number, offset: number) {
    if (view.kind !== 'web_link' || reordering) return;
    const { id, pages } = view.link;
    const destination = index + offset;
    if (!pages[destination]) return;
    const ids = pages.map((page) => page.id);
    [ids[index], ids[destination]] = [ids[destination], ids[index]];
    reordering = true;
    try {
      await api.reorderPages(id, ids);
      if (view.kind === 'web_link' && view.link.id === id) {
        const reordered = [...pages];
        [reordered[index], reordered[destination]] = [reordered[destination], reordered[index]];
        view = { kind: 'web_link', link: { ...view.link, pages: reordered.map((page, position) => ({ ...page, position })) } };
      }
    } catch (error) {
      reportError(error);
    } finally {
      reordering = false;
    }
  }

  async function startRename() {
    if (view.kind !== 'page') return;
    closeMenus();
    renameTitle = view.page.title;
    await tick();
    renameInput?.focus();
    renameInput?.select();
  }

  async function renamePage() {
    if (view.kind !== 'page' || renameTitle === null || renaming) return;
    renaming = true;
    const id = view.page.id;
    try {
      const { title, displayTitle, updatedAtMs, kind } = await api.renamePage(id, renameTitle);
      if (view.kind === 'page' && view.page.id === id) {
        view = { ...view, page: { ...view.page, title, displayTitle, updatedAtMs: Math.max(updatedAtMs, view.page.updatedAtMs) } };
        renameTitle = null;
      }
      if (kind === 'knowledge_page') void knowledge.load();
      refreshTagResults();
    } catch (error) {
      reportError(error);
    } finally {
      renaming = false;
    }
  }

  async function deleteRecord() {
    if (deleting || (view.kind !== 'page' && view.kind !== 'web_link')) return;
    const current = view;
    const title = current.kind === 'page' ? current.page.displayTitle : current.link.displayTitle;
    closeMenus();
    if (!window.confirm(`「${title}」をごみ箱へ移動しますか？`)) return;
    if (!(await flushEditor()) || deleting) return;
    const target: Target = current.kind === 'page' ? { kind: 'page', id: current.page.id } : { kind: 'web_link', id: current.link.id };
    if (!isOpen(target)) return;
    detailRequest += 1;
    detailLoading = false;
    deleting = true;
    const parent = current.kind === 'page' ? current.page.parent : undefined;
    try {
      if (current.kind === 'page') await api.softDeletePage(current.page.id);
      else await api.softDeleteWebLink(current.link.id);
      view = { kind: 'empty' };
      mobileDetail = false;
      void loadTags();
      void links.load();
      void knowledge.load();
      refreshTagResults();
    } catch (error) {
      reportError(error);
      return;
    } finally {
      deleting = false;
    }
    await tick();
    if (parent) await navigate({ kind: 'web_link', id: parent.id });
  }

  function ensureTags() {
    if (!tagsLoaded && !tagListLoading) void loadTags();
  }

  async function loadTags() {
    const request = ++tagListRequest;
    tagListLoading = true;
    tagListError = '';
    try {
      const tags = await api.listTags();
      if (request !== tagListRequest) return;
      allTags = tags;
      tagsLoaded = true;
      if (tagQuery.trim()) void filterTags();
    } catch (error) {
      if (request === tagListRequest) tagListError = normalizeError(error).message;
    } finally {
      if (request === tagListRequest) tagListLoading = false;
    }
  }

  async function filterTags() {
    const request = ++matchingTagsRequest;
    if (!tagQuery.trim()) return;
    try {
      const tags = await api.listTags(tagQuery.trim());
      if (request === matchingTagsRequest) matchingTags = tags;
    } catch (error) {
      if (request === matchingTagsRequest) reportError(error);
    }
  }

  async function saveRecordTags({ ids, name }: { ids: string[]; name?: string }) {
    if (tagsBusy) return;
    const target = view;
    if (target.kind !== 'web_link' && (!('page' in target) || !target.page)) return;
    tagsBusy = true;
    try {
      if (name) {
        const created = await api.createTag(name);
        if (ids.includes(created.id)) return;
        ids = [...ids, created.id];
      }
      if (target.kind === 'web_link') {
        const { id } = target.link;
        await api.setWebLinkTags(id, ids);
        const refreshed = await api.getWebLink(id);
        if (view.kind === 'web_link' && view.link.id === id) {
          view = { ...view, link: { ...view.link, tags: refreshed.tags, updatedAtMs: refreshed.updatedAtMs } };
        }
        void links.load();
      } else if (target.page) {
        const { id } = target.page;
        await api.setPageTags(id, ids);
        const { tags, updatedAtMs } = await api.getPage(id);
        if ((view.kind === 'page' || view.kind === 'diary') && view.page?.id === id) {
          const page = { ...view.page, tags, updatedAtMs: Math.max(updatedAtMs, view.page.updatedAtMs) };
          view = view.kind === 'diary' ? { ...view, page } : { kind: 'page', page };
        }
        if (target.page.kind === 'knowledge_page') void knowledge.load();
      }
      await loadTags();
      refreshTagResults();
    } catch (error) {
      if (name) void loadTags();
      reportError(error);
    } finally {
      tagsBusy = false;
    }
  }

  function scheduleTagSearch(immediate = false) {
    clearTimeout(tagTimer);
    tagResults.invalidate();
    if (immediate) void tagResults.load();
    else tagTimer = setTimeout(() => void tagResults.load(), 50);
  }

  function changeTagKind(kind: string) {
    tagKind = kind as SearchKind | '';
    scheduleTagSearch(true);
  }

  function changeTagSort(sort: string) {
    tagSort = sort as SearchSort;
    scheduleTagSearch(true);
  }

  function toggleTag(id: string) {
    tagRename = null;
    tagIds = tagIds.includes(id) ? tagIds.filter((tag) => tag !== id) : [...tagIds, id];
    scheduleTagSearch(true);
  }

  async function manageTag(action: 'create' | 'rename' | 'delete', tag?: TagSummary) {
    if (tagsBusy || (action === 'create' && !tagName.trim())) return;
    if (action === 'delete' && tag && !window.confirm(`タグ「${tag.name}」を削除しますか？`)) return;
    tagsBusy = true;
    tagError = '';
    try {
      if (action === 'create') {
        const created = await api.createTag(tagName);
        tagName = '';
        if (!tagIds.includes(created.id)) tagIds = [...tagIds, created.id];
      } else if (action === 'rename' && tag && tagRename !== null) {
        const updated = await api.renameTag(tag.id, tagRename);
        tagRename = null;
        syncTagInView(tag.id, updated);
      } else if (tag) {
        await api.deleteTag(tag.id);
        tagIds = tagIds.filter((id) => id !== tag.id);
        syncTagInView(tag.id);
      }
      await loadTags();
      scheduleTagSearch(true);
    } catch (error) {
      tagError = normalizeError(error).message;
    } finally {
      tagsBusy = false;
    }
  }

  function syncTagInView(id: string, next?: TagSummary | EditablePage['tags'][number]) {
    const update = (tags: EditablePage['tags']) => tags.flatMap((tag) => tag.id === id ? next ? [next] : [] : [tag]);
    if (view.kind === 'web_link') view = { ...view, link: { ...view.link, tags: update(view.link.tags) } };
    if (view.kind === 'diary' && view.page) view = { ...view, page: { ...view.page, tags: update(view.page.tags) } };
    if (view.kind === 'page') view = { ...view, page: { ...view.page, tags: update(view.page.tags) } };
  }

  function targetForHit(hit: SearchHit): Target | null {
    if (hit.kind === 'diary') return hit.diaryDate ? { kind: 'diary', date: hit.diaryDate } : null;
    return { kind: hit.kind === 'web_link' ? 'web_link' : 'page', id: hit.id };
  }

  function openHit(hit: SearchHit, fromSearch = false) {
    const target = targetForHit(hit);
    if (!target) return;
    const hitSection: Section = hit.kind === 'diary' ? 'diary' : hit.kind === 'knowledge_page' ? 'knowledge' : 'web_links';
    if (fromSearch) clearSearch();
    void navigate(target, fromSearch ? hitSection : section, true, undefined, fromSearch);
  }

  async function openDialog(next: Dialog) {
    if (dialog || blocked) return;
    const focus = document.activeElement as HTMLElement | null;
    if (!(await flushEditor()) || dialog || blocked) return;
    detailRequest += 1;
    detailLoading = false;
    closeMenus();
    drawerOpen = false;
    if (next.kind === 'search') {
      returnSearchFocus = focus;
      ensureTags();
    }
    dialog = next;
  }

  function clearSearch() {
    dialog = null;
    returnSearchFocus = null;
  }

  async function cancelSearch() {
    const focus = returnSearchFocus;
    clearSearch();
    await tick();
    if (focus?.getClientRects().length) focus.focus();
    else drawerButton?.focus();
  }

  function quickAdd(kind: 'web_link' | 'knowledge') {
    clearSearch();
    dialog = { kind };
  }

  function onLinkSaved(link: WebLinkDetail) {
    const editing = dialog?.kind === 'web_link' && dialog.link;
    dialog = null;
    void links.load();
    refreshTagResults();
    void navigate({ kind: 'web_link', id: link.id }, editing ? section : 'web_links', true, { kind: 'web_link', link });
  }

  function onKnowledgeCreated(page: PageView) {
    dialog = null;
    void knowledge.load();
    refreshTagResults();
    void navigate({ kind: 'page', id: page.id }, 'knowledge', true, { kind: 'page', page });
  }

  async function refreshLibrary(restored = false) {
    if (restored) {
      beforeSettings = undefined;
      tagIds = [];
      appStatus = await api.getAppStatus();
      links.clear();
      knowledge.clear();
    }
    refreshTagResults();
    if (restored) {
      if (section === 'web_links') links.ensureLoaded();
      if (section === 'knowledge') knowledge.ensureLoaded();
      await Promise.all([loadCalendar(), loadTags()]);
    } else {
      await Promise.all([loadCalendar(), loadTags(), links.load(), knowledge.load()]);
    }
  }

  function openExport() {
    if (!documentPage) return;
    const title = (documentPage.diaryDate ?? documentPage.displayTitle).replace(/[\\/:*?"<>|]/g, '_');
    exportPath = `${appStatus?.dataDir ? `${appStatus.dataDir}\\exports\\` : ''}${title}.md`;
    exportMessage = '';
    exportError = '';
    void openDialog({ kind: 'export', id: documentPage.id });
  }

  async function chooseExportPath() {
    if (pickingExport || exporting) return;
    pickingExport = true;
    exportError = '';
    try {
      const path = await api.pickSavePath({ title: 'Markdownの保存先', defaultPath: exportPath,
        filters: [{ name: 'Markdown', extensions: ['md'] }] });
      if (path) exportPath = path;
    } catch (error) {
      exportError = normalizeError(error).message;
    } finally {
      pickingExport = false;
    }
  }

  async function exportMarkdown() {
    if (dialog?.kind !== 'export' || exporting) return;
    exporting = true;
    exportError = '';
    exportMessage = '';
    try {
      await api.exportPageMarkdown(dialog.id, exportPath);
      exportMessage = 'Markdownを書き出しました。';
    } catch (error) {
      exportError = normalizeError(error).message;
    } finally {
      exporting = false;
    }
  }

  function closeMenus() {
    if (newMenu) newMenu.open = false;
    if (detailMenu) detailMenu.open = false;
  }

  function dismissOutside(event: PointerEvent) {
    if (newMenu && !newMenu.contains(event.target as Node)) newMenu.open = false;
    if (detailMenu && !detailMenu.contains(event.target as Node)) detailMenu.open = false;
  }

  async function setDrawer(open: boolean) {
    drawerOpen = open;
    await tick();
    if (drawerVisible) document.querySelector<HTMLButtonElement>('.drawer-close')?.focus();
    else drawerButton?.focus();
  }

  function globalKeydown(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape' && !dialog) {
      if (drawerVisible) void setDrawer(false);
      const menu = newMenu?.open ? newMenu : detailMenu?.open ? detailMenu : null;
      if (menu) {
        menu.open = false;
        menu.querySelector('summary')?.focus();
      }
    }
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
    const key = event.key.toLowerCase();
    if (key === 'p') {
      event.preventDefault();
      void openDialog({ kind: 'search' });
    } else if (key === 's' && editor && !dialog) {
      event.preventDefault();
      void editor.flush(true);
    }
  }
</script>

<svelte:window bind:innerWidth={width} on:keydown={globalKeydown} on:pointerdown={dismissOutside} on:focus={() => (today = currentJstDate())} />

<div class="app-shell" class:drawer-visible={drawerVisible}>
  {#if drawerVisible}
    <button type="button" class="nav-backdrop" aria-label="ナビゲーションを閉じる" tabindex="-1"
      on:click={() => void setDrawer(false)}></button>
  {/if}

  <aside id="sidebar" class="nav-pane" aria-label="サイドバー">
    <div class="brand">
      <img src={logo} alt="" width="34" height="34" />
      <span>Oboromi</span>
      <button type="button" class="ghost icon-button drawer-close" aria-label="ナビゲーションを閉じる"
        on:click={() => void setDrawer(false)}><Icon name="close" size={17} /></button>
    </div>
    <button type="button" class="search-trigger" disabled={blocked} on:click={() => void openDialog({ kind: 'search' })}>
      <Icon name="search" size={16} /> <span>検索</span> <kbd>Ctrl+P</kbd>
    </button>

    <nav class="nav-links" aria-label="ライブラリ">
      <p class="nav-label">ライブラリ</p>
      {#each sectionEntries as [key, entry] (key)}
        <button type="button" class="nav-item" class:active={section === key && view.kind !== 'settings'}
          aria-current={section === key && view.kind !== 'settings' ? 'page' : undefined}
          disabled={blocked} on:click={() => void switchSection(key)}>
          <Icon name={entry.icon} /> <span>{entry.label}</span>
        </button>
      {/each}
    </nav>

    <div class="nav-bottom">
      <button type="button" class="nav-item" class:active={view.kind === 'settings'} aria-current={view.kind === 'settings' ? 'page' : undefined}
        disabled={settingsBusy || deleting || initializing} on:click={() => void openSettings()}>
        <Icon name="settings" /> <span>設定</span>
      </button>
      {#if !unavailable}
        <div class="storage-label" title="ライブラリはSQLiteでこの端末に保存されます"><span class="storage-dot" aria-hidden="true" /> この端末に保存</div>
      {/if}
    </div>
  </aside>

  <div class="workspace-shell" inert={drawerVisible}>
    <header class="top-bar">
      <div class="top-bar-left">
        <button type="button" class="ghost icon-button mobile-menu-btn" bind:this={drawerButton} aria-label="ナビゲーションを開く"
          aria-expanded={drawerVisible} aria-controls="sidebar" on:click={() => void setDrawer(!drawerVisible)}>
          <Icon name="panel" />
        </button>
        <div class="breadcrumbs" aria-label="現在の場所">
          {#if view.kind === 'settings'}
            <Icon name="settings" size={16} /> <span class="breadcrumb-root">設定</span>
          {:else}
            <Icon name={sections[section].icon} size={16} /> <span class="breadcrumb-root">{sections[section].label}</span>
            {#if viewTitle}
              <span class="breadcrumb-leaf"><Icon name="right" size={13} /> <span>{viewTitle}</span></span>
            {/if}
          {/if}
        </div>
      </div>
      <div class="top-bar-actions">
        <button type="button" class="ghost icon-button mobile-search-btn" aria-label="検索 (Ctrl+P)" disabled={blocked}
          on:click={() => void openDialog({ kind: 'search' })}><Icon name="search" /></button>
        {#if blocked}
          <button type="button" class="primary new-button" disabled><Icon name="plus" size={17} /> 新規</button>
        {:else}
          <details class="menu" bind:this={newMenu}>
            <summary class="primary new-button"><Icon name="plus" size={17} /> 新規 <Icon name="down" size={13} /></summary>
            <div class="menu-panel">
              <button type="button" on:click={() => void openDialog({ kind: 'web_link' })}><Icon name="link" size={16} /> Webリンクを追加</button>
              <button type="button" on:click={() => void openDialog({ kind: 'knowledge' })}><Icon name="note" size={16} /> ナレッジを追加</button>
              <button type="button" on:click={() => void openToday()}><Icon name="calendar" size={16} /> 今日の日記を書く</button>
            </div>
          </details>
        {/if}
      </div>
    </header>

    {#if startupError || appStatus?.startupError}
      <div class="startup-banner" role="alert">
        <span>{startupError || appStatus?.startupError}</span>
        {#if appStatus?.startupError}
          <button type="button" on:click={() => void openSettings()}>保存場所を確認</button>
        {:else}
          <button type="button" disabled={initializing} on:click={() => void initialize()}>再試行</button>
        {/if}
      </div>
    {/if}
    {#if globalError}
      <div class="startup-banner" role="alert">
        <span>{globalError}</span>
        <div class="notice-actions">
          {#if retryDetail}<button type="button" on:click={() => { globalError = ''; retryDetail?.(); }}>再試行</button>{/if}
          <button type="button" class="ghost icon-button" aria-label="エラーを閉じる" on:click={() => (globalError = '')}><Icon name="close" size={17} /></button>
        </div>
      </div>
    {/if}

    <div class="workspace" class:mobile-detail={mobileDetail} inert={unavailable && view.kind !== 'settings'}>
      <aside class="middle-pane" aria-label={sections[section].label + 'の一覧'} bind:this={middlePane} inert={unavailable || settingsBusy || deleting}>
        {#if section === 'diary'}
          <DiaryCalendar year={month.year} month={month.month} {today} selected={selectedDate} entries={diaryDates}
            loading={calendarLoading} error={calendarError}
            on:select={(event) => void navigate({ kind: 'diary', date: event.detail })}
            on:month={(event) => moveCalendar(shiftYearMonth(month.year, month.month, event.detail))}
            on:today={() => void openToday()} on:retry={() => void loadCalendar()} />
        {:else if section === 'web_links'}
          <CollectionList kind="web_links" {...$links}
            selectedId={view.kind === 'web_link' ? view.link.id : view.kind === 'page' ? view.page.parent?.id : undefined}
            on:select={(event) => void navigate({ kind: 'web_link', id: event.detail })}
            on:add={() => void openDialog({ kind: 'web_link' })} on:external={(event) => openExternal(event.detail)}
            on:retry={() => void links.load()} on:more={() => void links.load(false)} />
        {:else if section === 'knowledge'}
          <CollectionList kind="knowledge" {...$knowledge} selectedId={view.kind === 'page' ? view.page.id : undefined}
            on:select={(event) => void navigate({ kind: 'page', id: event.detail })}
            on:add={() => void openDialog({ kind: 'knowledge' })} on:retry={() => void knowledge.load()} on:more={() => void knowledge.load(false)} />
        {:else}
          <div class="pane-header"><h2>タグ</h2></div>
          <div class="tag-controls">
            <form class="inline-form" on:submit|preventDefault={() => void manageTag('create')}>
              <input type="text" placeholder="新しいタグ名" aria-label="新しいタグ名" bind:value={tagName} disabled={tagsBusy} />
              <button type="submit" class="small-button" disabled={tagsBusy || !tagName.trim()}><Icon name="plus" size={15} /> 追加</button>
            </form>
            {#if tagError}<div class="notice-box error" role="alert">{tagError}</div>{/if}
            {#if tagListError}
              <div class="notice-box error" role="alert">
                {tagListError}
                <button type="button" class="small-button" on:click={() => void loadTags()}>再試行</button>
              </div>
            {/if}
            {#if allTags.length}
              <input type="search" placeholder="タグ名で絞り込む…" aria-label="タグ名で絞り込む" value={tagQuery}
                on:input={(event) => { tagQuery = event.currentTarget.value; matchingTags = []; void filterTags(); }} />
              <div class="tag-bar">
                {#each visibleTags as tag (tag.id)}
                  <button type="button" class="tag-chip" class:active={tagIds.includes(tag.id)} aria-pressed={tagIds.includes(tag.id)}
                    on:click={() => toggleTag(tag.id)}>{tag.name}<span class="tag-count">{tag.count}</span></button>
                {/each}
              </div>
              {#if tagIds.length}
                <div class="tag-selection-actions">
                  <button type="button" class="ghost compact-button" on:click={() => { tagIds = []; tagRename = null; scheduleTagSearch(true); }}>選択を解除</button>
                  {#if singleTag && tagRename === null}
                    <button type="button" class="ghost compact-button" disabled={tagsBusy} on:click={() => (tagRename = singleTag?.name ?? null)}>改名</button>
                    <button type="button" class="ghost compact-button" disabled={tagsBusy} on:click={() => void manageTag('delete', singleTag)}>削除</button>
                  {/if}
                </div>
                {#if singleTag && tagRename !== null}
                  <form class="inline-form" on:submit|preventDefault={() => void manageTag('rename', singleTag)}>
                    <input type="text" aria-label="タグの新しい名前" bind:value={tagRename} required disabled={tagsBusy} />
                    <button type="submit" class="small-button" disabled={tagsBusy}>保存</button>
                    <button type="button" class="ghost icon-button" aria-label="改名を取り消す" disabled={tagsBusy} on:click={() => (tagRename = null)}><Icon name="close" size={15} /></button>
                  </form>
                {/if}
              {/if}
            {:else if !tagListError}
              <p class="inline-hint" role="status">{tagListLoading ? 'タグを読み込み中…' : 'タグはまだありません。'}</p>
            {/if}
          </div>
          <div class="tag-controls tag-search-controls">
            <h3>項目を絞り込む</h3>
            <input type="search" placeholder="キーワードを入力…" aria-label="タグ検索キーワード" bind:value={tagSearch} on:input={() => scheduleTagSearch()} />
            <div class="inline-form">
              <select aria-label="対象種別" value={tagKind}
                on:change={(event) => changeTagKind(event.currentTarget.value)}>
                <option value="">すべての種別</option>
                {#each searchKindOptions as [key, entry] (key)}<option value={key}>{entry.label}</option>{/each}
              </select>
              <select aria-label="並び順" value={tagSort}
                on:change={(event) => changeTagSort(event.currentTarget.value)}>
                <option value="relevance">一致優先</option><option value="updated">更新順</option>
              </select>
            </div>
            {#if isShortQuery(tagSearch)}<p class="inline-hint">1〜2文字ではタイトル・タグ・ホスト名の先頭から検索</p>{/if}
          </div>
          <div class="list-scroll" aria-busy={$tagResults.loading}>
            {#if $tagResults.error}
              <div class="notice-box error" role="alert">{$tagResults.error}
                <button type="button" class="small-button" on:click={() => void tagResults.load()}>再試行</button>
              </div>
            {:else if !$tagResults.items.length}
              <p class="list-empty" role="status">{$tagResults.loading ? '検索中…' : '条件に一致する項目はありません。'}</p>
            {/if}
            {#each $tagResults.items as hit (`${hit.kind}:${hit.id}`)}
              {@const appearance = searchKinds[hit.kind]}
              {@const selected = (view.kind === 'web_link' && hit.id === view.link.id) || (documentPage?.id === hit.id)}
              <button type="button" class="list-row" class:selected aria-current={selected ? 'true' : undefined} on:click={() => openHit(hit)}>
                <span class={'row-icon tone-' + appearance.tone}><Icon name={appearance.icon} size={16} /></span>
                <span class="row-labels"><span class="row-title">{hit.title}</span>
                  <span class="row-sub">{appearance.label} · {hit.parent?.title ?? formatTimestamp(hit.updatedAtMs)}</span></span>
              </button>
            {/each}
            {#if $tagResults.nextCursor}
              <button type="button" class="list-more" disabled={$tagResults.loading} on:click={() => void tagResults.load(false)}>さらに表示</button>
            {/if}
          </div>
        {/if}
      </aside>

      <main class="detail-pane" bind:this={detailPane} aria-busy={detailLoading || initializing}>
        {#if detailLoading}<div class="loading-line" aria-hidden="true"></div>{/if}
        <button type="button" class="ghost mobile-back-btn" on:click={() => void showList()} disabled={unavailable}>
          <Icon name="left" size={16} /> 一覧へ戻る
        </button>

        <div class="detail-content" inert={detailLoading || deleting}>
          {#if view.kind === 'settings'}
            <SettingsView status={appStatus} {theme} flushBeforeMutation={flushEditor} bind:busy={settingsBusy} on:theme={(event) => changeTheme(event.detail)}
              on:restored={() => void refreshLibrary(true).catch(reportError)} on:trashRestored={() => void refreshLibrary()}
              on:close={() => void closeSettings()} />
          {:else if view.kind === 'web_link'}
            {@const link = view.link}
            <article class="detail-container">
              <header class="document-header">
                <div class="document-icon tone-link"><Icon name="link" size={27} /></div>
                <div class="document-context">Webリンク</div>
                <div class="detail-title-row">
                  <h1 class="detail-title" tabindex="-1">{link.displayTitle}</h1>
                  <div class="detail-actions">
                    <button type="button" class="primary compact-button open-browser" aria-label="ブラウザで開く" title="ブラウザで開く" on:click={() => openExternal(link.normalizedUrl)}>
                      <Icon name="external" size={15} /> <span>ブラウザで開く</span>
                    </button>
                    <button type="button" class="ghost compact-button" on:click={() => void openDialog({ kind: 'web_link', link })}><Icon name="pen" size={16} /> 編集</button>
                    <details class="menu" bind:this={detailMenu}>
                      <summary class="ghost icon-button" aria-label="Webリンクのその他の操作"><Icon name="dots" /></summary>
                      <div class="menu-panel"><button type="button" class="menu-danger" on:click={() => void deleteRecord()}><Icon name="trash" size={16} /> ごみ箱へ移動</button></div>
                    </details>
                  </div>
                </div>
                <dl class="document-properties">
                  <div><dt><Icon name="link" size={15} /> URL</dt><dd class="url-property">
                    <button type="button" class="url-link-btn" on:click={() => openExternal(link.normalizedUrl)} title={link.url}>{link.url}</button>
                  </dd></div>
                  <div><dt><Icon name="tag" size={15} /> タグ</dt><dd>
                    {#key link.id}
                      <TagEditor tags={link.tags} {allTags} disabled={tagsBusy} on:focus={ensureTags} on:change={(event) => void saveRecordTags(event.detail)} />
                    {/key}
                  </dd></div>
                  <div><dt><Icon name="clock" size={15} /> 更新</dt><dd>{formatTimestamp(link.updatedAtMs)}</dd></div>
                </dl>
              </header>

              {#if link.description}<p class="link-description">{link.description}</p>{/if}

              <section class="attached-section" aria-label="付属ページ">
                <h2><Icon name="note" size={18} /> 付属ページ <span>{link.pages.length}</span></h2>
                {#if link.pages.length}
                  <div class="attached-list">
                    {#each link.pages as page, index (page.id)}
                      <div class="attached-row">
                        <button type="button" class="attached-row-title" on:click={() => void navigate({ kind: 'page', id: page.id })}>
                          <Icon name="note" size={16} /><span>{page.displayTitle}</span>
                        </button>
                        <span class="attached-timestamp">{formatTimestamp(page.updatedAtMs)}</span>
                        <button type="button" class="ghost icon-button" aria-label={`${page.displayTitle}を上へ移動`} title="上へ移動"
                          disabled={reordering || index === 0} on:click={() => void moveAttachedPage(index, -1)}><Icon name="up" size={15} /></button>
                        <button type="button" class="ghost icon-button" aria-label={`${page.displayTitle}を下へ移動`} title="下へ移動"
                          disabled={reordering || index === link.pages.length - 1} on:click={() => void moveAttachedPage(index, 1)}><Icon name="down" size={15} /></button>
                      </div>
                    {/each}
                  </div>
                {/if}
                <form class="inline-form attached-form" on:submit|preventDefault={() => void addAttachedPage()}>
                  <input type="text" placeholder="ページのタイトル（任意）" aria-label="新しい付属ページのタイトル" bind:value={attachedTitle} disabled={addingAttached || reordering} />
                  <button type="submit" disabled={addingAttached || reordering}><Icon name="plus" size={16} /> ページを追加</button>
                </form>
              </section>
            </article>
          {:else if view.kind === 'diary' || view.kind === 'page'}
            {@const date = view.kind === 'diary' ? view.date : ''}
            {@const page = view.page}
            {@const appearance = date ? searchKinds.diary : page?.kind === 'attached_page' ? searchKinds.attached_page : searchKinds.knowledge_page}
            <article class="detail-container">
              <header class="document-header">
                <div class="document-emblem-row">
                  <div class={'document-icon tone-' + appearance.tone}><Icon name={date ? 'book' : appearance.icon} size={27} /></div>
                  {#if date}
                    <div class="day-step-row" role="group" aria-label="日付を移動">
                      <button type="button" class="ghost icon-button" aria-label="前日" title="前日" on:click={() => void navigate({ kind: 'diary', date: shiftYmd(date, -1) })}><Icon name="left" size={16} /></button>
                      <button type="button" class="ghost compact-button" on:click={() => void openToday()}>今日</button>
                      <button type="button" class="ghost icon-button" aria-label="翌日" title="翌日" on:click={() => void navigate({ kind: 'diary', date: shiftYmd(date, 1) })}><Icon name="right" size={16} /></button>
                    </div>
                  {/if}
                </div>
                <div class="document-context">
                  {#if date}
                    <span>{formatDiaryWeekday(date)}</span>
                    {#if date === today}<span class="quiet-pill today-pill">今日</span>{/if}
                  {:else if page?.parent}
                    {@const parent = page.parent}
                    <button type="button" class="ghost breadcrumb-btn" title={parent.title}
                      on:click={() => void navigate({ kind: 'web_link', id: parent.id })}><Icon name="left" size={15} /> <span>{parent.title}</span></button>
                  {:else}
                    {appearance.label}
                  {/if}
                </div>
                <div class="detail-title-row">
                  {#if date}
                    <h1 class="detail-title diary-title" tabindex="-1"><time datetime={date}>{formatDiaryDate(date)}</time></h1>
                  {:else if renameTitle !== null}
                    <form class="inline-form title-form" on:submit|preventDefault={() => void renamePage()}>
                      <input type="text" aria-label="ページ名" bind:this={renameInput} bind:value={renameTitle} disabled={renaming} />
                      <button type="submit" class="primary" disabled={renaming}>保存</button>
                      <button type="button" disabled={renaming} on:click={() => (renameTitle = null)}>取消</button>
                    </form>
                  {:else}
                    <h1 class="detail-title" tabindex="-1">{page?.displayTitle}</h1>
                    <details class="menu" bind:this={detailMenu}>
                      <summary class="ghost icon-button" aria-label="ページの操作"><Icon name="dots" /></summary>
                      <div class="menu-panel">
                        <button type="button" on:click={() => void startRename()}><Icon name="pen" size={16} /> ページ名を変更</button>
                        <button type="button" class="menu-danger" on:click={() => void deleteRecord()}><Icon name="trash" size={16} /> ごみ箱へ移動</button>
                      </div>
                    </details>
                  {/if}
                </div>
                <dl class="document-properties">
                  <div><dt><Icon name="tag" size={15} /> タグ</dt><dd>
                    {#if page}
                      {#key page.id}
                        <TagEditor tags={page.tags} {allTags} disabled={tagsBusy} on:focus={ensureTags} on:change={(event) => void saveRecordTags(event.detail)} />
                      {/key}
                    {:else}
                      <span class="property-placeholder">初回保存後に追加できます</span>
                    {/if}
                  </dd></div>
                </dl>
              </header>

              {#key documentKey}
                <DocumentEditor bind:this={editor} {page} diaryDate={date} exportAvailable={Boolean(page)}
                  on:committed={(event) => updatePageAfterSave(event.detail.receipt, event.detail.markdown)} on:reloaded={(event) => updateReloadedPage(event.detail)}
                  on:openExternalUrl={(event) => openExternal(event.detail)} on:export={openExport} />
              {/key}
            </article>
          {:else if initializing || detailLoading}
            <div class="detail-container workspace-empty" role="status">ライブラリを読み込み中…</div>
          {:else if !unavailable}
            {@const appearance = sections[section]}
            {@const emptyCollection = section === 'web_links' ? !$links.items.length : section === 'knowledge' ? !$knowledge.items.length : !$tagResults.items.length}
            <div class="detail-container workspace-empty">
              <div class={'document-icon tone-' + appearance.tone}><Icon name={appearance.icon} size={27} /></div>
              <h1 class="detail-title" tabindex="-1">{appearance.label}</h1>
              {#if section === 'web_links' || section === 'knowledge'}
                <p>{emptyCollection ? (section === 'web_links' ? '気になるWebサイトや記事を保存できます。' : '調べたことやあとで読み返したいメモをまとめられます。') : '一覧から開きたい項目を選んでください。'}</p>
                <button type="button" class="primary" on:click={() => void openDialog({ kind: section === 'web_links' ? 'web_link' : 'knowledge' })}>
                  <Icon name="plus" size={17} /> {appearance.label}を追加
                </button>
              {:else if section === 'tags'}
                <p>{emptyCollection ? 'Webリンクや日記に付けたタグから、関連する項目を探せます。' : '一覧から開きたい項目を選んでください。'}</p>
              {:else}
                <button type="button" class="primary" on:click={() => void openToday()}>今日の日記を開く</button>
              {/if}
            </div>
          {/if}
        </div>
      </main>
    </div>
  </div>
</div>

{#if dialog?.kind === 'search'}
  <PaletteModal {allTags} {isMultiPane} {tagListLoading} {tagListError} on:reloadTags={() => void loadTags()} on:select={(event) => openHit(event.detail, true)}
    on:cancel={() => void cancelSearch()} on:openTodayDiary={() => { clearSearch(); void openToday(); }}
    on:openAddKnowledgePage={() => quickAdd('knowledge')} on:openAddWebLink={() => quickAdd('web_link')} />
{:else if dialog?.kind === 'web_link'}
  <WebLinkModal link={dialog.link ?? null} on:close={() => (dialog = null)} on:saved={(event) => onLinkSaved(event.detail)}
    on:openExisting={(event) => { dialog = null; void navigate({ kind: 'web_link', id: event.detail }, 'web_links'); }} />
{:else if dialog?.kind === 'knowledge'}
  <KnowledgePageModal on:close={() => (dialog = null)} on:created={(event) => onKnowledgeCreated(event.detail)} />
{:else if dialog?.kind === 'export'}
  <Modal label="Markdown書き出し" closeDisabled={exporting || pickingExport} on:close={() => (dialog = null)}>
    <div class="modal-header">
      <span class="row-icon tone-knowledge"><Icon name="download" size={18} /></span><h2>Markdown書き出し</h2>
      <button type="button" class="ghost icon-button" aria-label="閉じる" disabled={exporting || pickingExport} on:click={() => (dialog = null)}><Icon name="close" /></button>
    </div>
    <form class="modal-body" on:submit|preventDefault={() => void exportMarkdown()}>
      <div class="form-field"><label for="export-path">保存先 (.md)</label>
        <div class="inline-form">
          <input id="export-path" type="text" bind:value={exportPath} data-initial-focus required disabled={exporting || pickingExport} />
          <button type="button" on:click={() => void chooseExportPath()} disabled={exporting || pickingExport}>参照…</button>
        </div>
      </div>
      {#if exportMessage}<div class="notice-box success" role="status">{exportMessage}</div>{/if}
      {#if exportError}<div class="notice-box error" role="alert">{exportError}</div>{/if}
      <div class="form-actions">
        <button type="button" disabled={exporting || pickingExport} on:click={() => (dialog = null)}>閉じる</button>
        <button type="submit" class="primary" disabled={exporting || pickingExport}>{exporting ? '書き出し中…' : '書き出す'}</button>
      </div>
    </form>
  </Modal>
{/if}
