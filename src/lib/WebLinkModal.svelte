<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { api, normalizeError } from './api';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import WebLinkThumbnail from './WebLinkThumbnail.svelte';
  import type { WebLinkDetail, WebLinkSummary } from './types';

  export let link: WebLinkDetail | null = null;
  const dispatch = createEventDispatcher<{ saved: WebLinkDetail; openExisting: string; close: void }>();

  let url = link?.url ?? '';
  let title = link?.title ?? '';
  let description = link?.description ?? '';
  let showFields = Boolean(link);
  const initialUrl = urlKey(url);
  let currentUrl = initialUrl;
  let titleEdited = Boolean(title);
  let descriptionEdited = Boolean(description);
  let thumbnail: string | null = null;
  let metadataLoading = false;
  let metadataUnavailable = false;
  let attemptedUrl: string | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;
  let inFlight: { url: string; request: number; promise: Promise<void> } | undefined;
  let saving = false;
  let error = '';
  let candidates: WebLinkSummary[] = [];

  $: label = link ? 'Webリンクを編集' : 'Webリンクを追加';
  $: validUrl = urlKey(url);
  $: savedThumbnail = validUrl === initialUrl && link?.hasThumbnail ? link : undefined;

  onDestroy(() => {
    clearTimeout(timer);
    request += 1;
  });

  function urlKey(value: string) {
    const trimmed = value.trim();
    if (!/^https?:\/\//i.test(trimmed)) return undefined;
    try { return new URL(trimmed).href; } catch { return undefined; }
  }

  function handleUrlInput(value: string) {
    candidates = [];
    error = '';
    const key = urlKey(value);
    if (key === currentUrl) return;
    currentUrl = key;
    clearTimeout(timer);
    request += 1;
    attemptedUrl = undefined;
    thumbnail = null;
    metadataLoading = false;
    metadataUnavailable = false;
    if (!titleEdited) title = link?.title ?? '';
    if (!descriptionEdited) description = link?.description ?? '';
    if (key && key !== initialUrl) timer = setTimeout(() => void getMetadata(key), 500);
  }

  async function readMetadata(key: string, sequence: number) {
    const metadata = await api.fetchWebLinkMetadata(key).catch(() => null);
    if (sequence !== request) return;
    if (metadata?.title && !titleEdited) title = metadata.title;
    if (metadata?.description && !descriptionEdited) description = metadata.description;
    if (metadata?.thumbnail) thumbnail = metadata.thumbnail;
    metadataUnavailable = !metadata?.title && !metadata?.description && !metadata?.thumbnail;
    metadataLoading = false;
    attemptedUrl = key;
  }

  function getMetadata(key: string) {
    clearTimeout(timer);
    if (inFlight?.url === key && inFlight.request === request && metadataLoading) return inFlight.promise;
    metadataLoading = true;
    metadataUnavailable = false;
    const sequence = ++request;
    const promise = readMetadata(key, sequence);
    inFlight = { url: key, request: sequence, promise };
    return promise;
  }

  async function submit(allowDuplicate = false) {
    const key = urlKey(url);
    if (!key || saving) return;
    saving = true;
    error = '';
    if (!allowDuplicate) candidates = [];
    try {
      if (metadataLoading || (key !== initialUrl && attemptedUrl !== key)) await getMetadata(key);
      const input = { url, title, description, thumbnail: thumbnail ?? undefined };
      const saved = link
        ? await api.updateWebLink(link.id, input, allowDuplicate)
        : await api.createWebLink(input, allowDuplicate);
      dispatch('saved', saved);
    } catch (err) {
      const payload = normalizeError(err);
      if (payload.code === 'DuplicateUrl' && payload.candidates) candidates = payload.candidates;
      else error = payload.message;
    } finally {
      saving = false;
    }
  }
</script>

<Modal {label} className="link-form-card" closeDisabled={saving} on:close={() => dispatch('close')}>
  <div class="modal-header">
    <span class="row-icon tone-link"><Icon name="link" size={18} /></span>
    <h2>{label}</h2>
    <button type="button" class="ghost icon-button" aria-label="閉じる" disabled={saving} on:click={() => dispatch('close')}>
      <Icon name="close" size={18} />
    </button>
  </div>

  <form class="modal-body" on:submit|preventDefault={() => void submit()}>
    <div class="form-field">
      <label for="link-url">URL</label>
      <div class="inline-form capture-url">
        <input id="link-url" type="url" placeholder="URLを貼り付け" bind:value={url} data-initial-focus
          on:input={(event) => handleUrlInput(event.currentTarget.value)} required disabled={saving} />
        <button type="button" class="compact-button" disabled={!validUrl || metadataLoading || saving}
          on:click={() => { if (validUrl) void getMetadata(validUrl); }}>情報を取得</button>
      </div>
    </div>

    {#if metadataLoading || metadataUnavailable}
      <p class="inline-hint" role="status">
        {metadataLoading ? 'リンク情報を取得中…' : 'リンク情報を取得できませんでした。入力した内容で保存できます。'}
      </p>
    {/if}

    {#if !showFields && (thumbnail || savedThumbnail || title || description)}
      <div class="capture-preview" class:with-preview={Boolean(thumbnail || savedThumbnail)}>
        {#if thumbnail || savedThumbnail}<WebLinkThumbnail source={thumbnail} saved={savedThumbnail} />{/if}
        <div class="capture-summary">
          <strong>{title || (validUrl ? new URL(validUrl).hostname : '')}</strong>
          {#if description}<p>{description}</p>{/if}
        </div>
      </div>
    {/if}

    <button type="button" class="ghost compact-button capture-toggle" aria-expanded={showFields} aria-controls="link-edit-fields"
      disabled={saving} on:click={() => (showFields = !showFields)}>
      <Icon name={showFields ? 'up' : 'down'} size={14} /> タイトル・説明を編集
    </button>
    <div id="link-edit-fields" class="capture-details" class:with-preview={Boolean(thumbnail || savedThumbnail)} hidden={!showFields}>
      {#if showFields && (thumbnail || savedThumbnail)}
        <WebLinkThumbnail source={thumbnail} saved={savedThumbnail} />
      {/if}
      <div class="capture-fields">
        <div class="form-field">
          <label for="link-title">タイトル</label>
          <input id="link-title" type="text" placeholder="空欄でホスト名を表示" bind:value={title} disabled={saving}
            on:input={() => (titleEdited = true)} />
        </div>
        <div class="form-field">
          <label for="link-description">説明</label>
          <textarea id="link-description" class="link-description-input" rows="4" placeholder="用途やメモ（任意）"
            bind:value={description} disabled={saving} on:input={() => (descriptionEdited = true)} />
        </div>
      </div>
    </div>

    {#if error}<div class="notice-box error" role="alert">{error}</div>{/if}
    {#if candidates.length}
      <section class="notice-box warn" aria-label="同じURLの候補">
        <strong>同じURLが登録されています。</strong>
        <div class="duplicate-list">
          {#each candidates as candidate (candidate.id)}
            <div class="attached-row">
              <span class="row-labels">
                <span class="row-title">{candidate.displayTitle}</span>
                <span class="row-sub">{candidate.url}</span>
              </span>
              <button type="button" disabled={saving} on:click={() => dispatch('openExisting', candidate.id)}>既存項目を開く</button>
            </div>
          {/each}
        </div>
        <button type="button" disabled={saving} on:click={() => void submit(true)}>別件として保存</button>
      </section>
    {/if}

    <div class="form-actions">
      <button type="button" on:click={() => dispatch('close')} disabled={saving}>キャンセル</button>
      <button type="submit" class="primary" disabled={saving || !validUrl}>{saving ? '保存中…' : link ? '保存' : '追加'}</button>
    </div>
  </form>
</Modal>

<style>
  .capture-url { gap: 8px; }
  .capture-details, .capture-preview { display: grid; gap: 12px; min-width: 0; }
  .with-preview { grid-template-columns: minmax(0, 160px) minmax(0, 1fr); align-items: start; }
  .capture-preview { padding: 9px; border: 1px solid var(--border); border-radius: 9px; background: var(--bg-panel); }
  .capture-summary { min-width: 0; overflow-wrap: anywhere; }
  .capture-summary strong, .capture-summary p { display: -webkit-box; overflow: hidden; -webkit-box-orient: vertical; }
  .capture-summary strong { -webkit-line-clamp: 2; line-clamp: 2; line-height: 1.4; }
  .capture-summary p { -webkit-line-clamp: 3; line-clamp: 3; margin: 4px 0 0; color: var(--text-secondary); font-size: 12px; }
  .capture-toggle { justify-self: start; padding-inline: 0 6px; }
  .capture-fields { display: grid; gap: 12px; min-width: 0; }
  @media (max-width: 520px) {
    .capture-details.with-preview { grid-template-columns: minmax(0, 1fr); }
    .capture-details :global(.web-thumbnail) { max-width: 185px; }
    .capture-preview.with-preview { grid-template-columns: minmax(0, 105px) minmax(0, 1fr); }
  }
</style>
