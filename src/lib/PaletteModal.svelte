<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from 'svelte';
  import { api, normalizeError } from './api';
  import { formatDiaryDate, formatTimestamp } from './jst';
  import { isShortQuery, searchKindOptions, searchKinds } from './search';
  import type { MatchedField, SearchHit, SearchKind, SearchSort, TagSummary, WebLinkPreview } from './types';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  export let allTags: TagSummary[] = [];
  export let isMultiPane = true;
  export let tagListLoading = false;
  export let tagListError = '';

  const dispatch = createEventDispatcher<{
    select: SearchHit;
    reloadTags: void;
    openTodayDiary: void;
    openAddKnowledgePage: void;
    openAddWebLink: void;
    cancel: void;
  }>();

  const matchLabels: Record<MatchedField, string> = {
    title: 'タイトル', url: 'URL', tag: 'タグ', parentTitle: '親リンク', description: '説明', body: '本文',
  };
  type Preview = { link: WebLinkPreview } | { renderedHtml: string | null };

  let query = '';
  let kind: SearchKind | '' = '';
  let tagIds: string[] = [];
  let sort: SearchSort = 'relevance';
  let hits: SearchHit[] = [];
  let cursor: string | undefined;
  let loading = true;
  let error = '';
  let highlighted = -1;
  let composing = false;
  let input: HTMLInputElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;
  let searchInFlight = false;
  let searchQueued = false;
  let previewHit: SearchHit | undefined;
  let preview: Preview | null = null;
  let previewError = '';
  let previewLinkError = '';
  let previewRequest = 0;
  let previewInFlight = false;

  $: quickActions = !query.trim() && !kind && !tagIds.length;
  $: selectPreview(isMultiPane ? hits[highlighted] : undefined);

  onMount(() => { void runSearch(); });
  onDestroy(() => {
    request += 1;
    searchQueued = false;
    previewRequest += 1;
    previewHit = undefined;
    clearTimeout(timer);
  });

  function scheduleSearch() {
    clearTimeout(timer);
    request += 1;
    searchQueued = false;
    hits = [];
    cursor = undefined;
    highlighted = -1;
    error = '';
    loading = true;
    if (!composing) timer = setTimeout(() => void runSearch(), 50);
  }

  async function runSearch(reset = true) {
    if (!reset && (loading || !cursor)) return;
    if (searchInFlight) {
      searchQueued = true;
      return;
    }
    searchInFlight = true;
    const sequence = ++request;
    loading = true;
    error = '';
    try {
      const result = await api.search(query, kind || undefined, tagIds, sort, reset ? undefined : cursor);
      if (sequence !== request) return;
      hits = reset ? result.items : [...hits, ...result.items];
      cursor = result.nextCursor;
      if (reset) highlighted = hits.length ? 0 : -1;
    } catch (issue) {
      if (sequence === request) error = normalizeError(issue).message;
    } finally {
      searchInFlight = false;
      if (sequence === request) loading = false;
      if (searchQueued) {
        searchQueued = false;
        void runSearch();
      }
    }
  }

  function selectPreview(hit: SearchHit | undefined) {
    if (hit === previewHit) return;
    previewRequest += 1;
    previewHit = hit;
    preview = null;
    previewLinkError = '';
    previewError = '';
    void loadPreview();
  }

  async function loadPreview() {
    const hit = previewHit;
    if (!hit || previewInFlight) return;
    const sequence = previewRequest;
    previewInFlight = true;
    previewError = '';
    try {
      const result = hit.kind === 'web_link'
        ? { link: await api.getWebLinkPreview(hit.id) }
        : { renderedHtml: await api.getPagePreview(hit.id) };
      if (sequence === previewRequest) preview = result;
    } catch (issue) {
      if (sequence === previewRequest) previewError = normalizeError(issue).message;
    } finally {
      previewInFlight = false;
      if (sequence !== previewRequest) void loadPreview();
    }
  }

  function openPreviewLink(event: MouseEvent) {
    if (!(event.target instanceof Element)) return;
    const url = event.target.closest<HTMLButtonElement>('button[data-external-url]')?.dataset.externalUrl;
    if (!url) return;
    const sequence = previewRequest;
    previewLinkError = '';
    void api.openExternalUrl(url).catch((issue) => {
      if (sequence === previewRequest) previewLinkError = normalizeError(issue).message;
    });
  }

  function setKind(next: SearchKind | '') {
    if (kind === next) return;
    kind = next;
    scheduleSearch();
  }

  function toggleTag(id: string) {
    tagIds = tagIds.includes(id) ? tagIds.filter((tag) => tag !== id) : [...tagIds, id];
    scheduleSearch();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.target !== input || event.isComposing || composing || event.keyCode === 229 || !hits.length) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const offset = event.key === 'ArrowDown' ? 1 : -1;
      highlighted = (highlighted + offset + hits.length) % hits.length;
      document.getElementById(`palette-hit-${highlighted}`)?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter' && hits[highlighted]) {
      event.preventDefault();
      dispatch('select', hits[highlighted]);
    }
  }
</script>

<Modal label="統合検索" palette on:close={() => dispatch('cancel')} on:keydown={handleKeydown}>
  <div class="palette-input">
    <Icon name="search" size={21} />
    <input type="search" placeholder="タイトル・URL・本文・タグを検索…" aria-label="検索語を入力"
      role="combobox" aria-autocomplete="list" aria-controls="palette-results" aria-expanded={hits.length > 0}
      aria-activedescendant={highlighted >= 0 ? `palette-hit-${highlighted}` : undefined}
      bind:this={input} bind:value={query} data-initial-focus
      on:input={scheduleSearch}
      on:compositionstart={() => (composing = true)}
      on:compositionend={() => { composing = false; scheduleSearch(); }} />
    <button type="button" class="ghost escape-key" aria-label="検索を閉じる" on:click={() => dispatch('cancel')}>Esc</button>
  </div>

  <div class="palette-layout" class:with-preview={isMultiPane}>
  <div class="modal-body palette-body">
    <div class="palette-filters">
      <div class="filter-tabs" role="group" aria-label="対象種別">
        <button type="button" aria-pressed={!kind} on:click={() => setKind('')}>すべて</button>
        {#each searchKindOptions as [value, appearance] (value)}
          <button type="button" aria-pressed={kind === value} on:click={() => setKind(value)}>{appearance.label}</button>
        {/each}
      </div>
      <select aria-label="並び順" bind:value={sort} on:change={scheduleSearch}>
        <option value="relevance">一致優先</option>
        <option value="updated">更新順</option>
      </select>
    </div>

    {#if tagListError}
      <p class="inline-hint" role="status">
        タグを読み込めませんでした。
        <button type="button" class="small-button" on:click={() => dispatch('reloadTags')}>再試行</button>
      </p>
    {:else if tagListLoading && !allTags.length}
      <p class="inline-hint" role="status">タグを読み込み中…</p>
    {/if}
    {#if allTags.length > 0}
      <div class="tag-bar palette-tags" role="group" aria-label="タグで絞り込む">
        {#each allTags as tag (tag.id)}
          <button type="button" class="tag-chip" class:active={tagIds.includes(tag.id)}
            aria-pressed={tagIds.includes(tag.id)} on:click={() => toggleTag(tag.id)}>
            {tag.name}<span class="tag-count">({tag.count})</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if isShortQuery(query)}
      <p class="inline-hint">1〜2文字ではタイトル・タグ・ホスト名の先頭から検索</p>
    {/if}

    {#if quickActions}
      <div class="palette-quick-actions">
        <button type="button" on:click={() => dispatch('openTodayDiary')}><Icon name="calendar" size={15} /> 今日の日記</button>
        <button type="button" on:click={() => dispatch('openAddWebLink')}><Icon name="link" size={15} /> Webリンクを追加</button>
        <button type="button" on:click={() => dispatch('openAddKnowledgePage')}><Icon name="note" size={15} /> ナレッジを追加</button>
      </div>
    {/if}

    {#if error}
      <div class="notice-box error" role="alert">
        {error}
        <button type="button" class="small-button" on:click={() => void runSearch()}>再試行</button>
      </div>
    {:else if hits.length === 0}
      <p class="palette-empty" role="status">
        {loading ? '検索中…' : quickActions ? '最近更新した項目はありません。' : '一致する項目が見つかりませんでした。'}
      </p>
    {:else if quickActions}
      <p class="palette-list-heading">最近の項目</p>
    {/if}

    <div id="palette-results" class="palette-results" role="listbox" aria-label="検索候補" aria-busy={loading}>
      {#each hits as hit, index (`${hit.kind}:${hit.id}`)}
        {@const appearance = searchKinds[hit.kind]}
        <button id={`palette-hit-${index}`} type="button" role="option" aria-selected={index === highlighted}
          class="list-row" class:selected={index === highlighted} tabindex="-1" on:click={() => dispatch('select', hit)}>
          <span class={'row-icon tone-' + appearance.tone}><Icon name={appearance.icon} size={16} /></span>
          <span class="row-labels">
            <span class="row-title">{hit.title}</span>
            <span class="row-sub">{hit.parent?.title ?? hit.url ?? formatTimestamp(hit.updatedAtMs)}</span>
          </span>
          <span class="result-kind">{appearance.label}{hit.matchedField ? ` · ${matchLabels[hit.matchedField]}一致` : ''}</span>
        </button>
      {/each}
    </div>
    {#if cursor}
      <button type="button" class="list-more" disabled={loading} on:click={() => void runSearch(false)}>
        {loading ? '読み込み中…' : 'さらに表示'}
      </button>
    {/if}
  </div>
  {#if isMultiPane}
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
    <section class="palette-preview" aria-label="プレビュー" tabindex={previewHit ? 0 : undefined}>
      {#if previewHit}
        {@const appearance = searchKinds[previewHit.kind]}
        <header class="palette-preview-header">
          <span class={'row-icon tone-' + appearance.tone}><Icon name={appearance.icon} size={17} /></span>
          <div>
            <p class="inline-hint">{appearance.label}</p>
            <h2>{previewHit.diaryDate ? formatDiaryDate(previewHit.diaryDate) : previewHit.title}</h2>
          </div>
        </header>
        {#if previewHit.parent}<p class="inline-hint">{previewHit.parent.title}</p>{/if}
        {#if previewError}
          <div class="notice-box error" role="alert">
            {previewError}
            <button type="button" class="small-button" on:click={() => void loadPreview()}>再試行</button>
          </div>
        {:else if !preview}
          <p class="inline-hint" role="status">プレビューを読み込み中…</p>
        {:else if 'link' in preview}
          <p class="palette-preview-url">{preview.link.url}</p>
          {#if preview.link.description}<p class="link-description">{preview.link.description}</p>{/if}
          <p class="inline-hint">付属ページ {preview.link.pageCount} 件</p>
        {:else if preview.renderedHtml !== null}
          {#if previewLinkError}<div class="notice-box error" role="alert">{previewLinkError}</div>{/if}
          <!-- The Rust renderer emits escaped HTML and buttons for validated external links. -->
          <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
          <div class="markdown-preview" on:click={openPreviewLink}>{@html preview.renderedHtml}</div>
        {:else}
          <p class="inline-hint">本文はまだありません。</p>
        {/if}
      {/if}
    </section>
  {/if}
  </div>
  <div class="palette-footer"><span><kbd>↑</kbd><kbd>↓</kbd> 選択</span><span><kbd>Enter</kbd> 開く</span></div>
</Modal>
