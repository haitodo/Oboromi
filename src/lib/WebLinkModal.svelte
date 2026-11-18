<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { api, normalizeError } from './api';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import type { WebLinkDetail, WebLinkSummary } from './types';

  export let link: WebLinkDetail | null = null;
  const dispatch = createEventDispatcher<{ saved: WebLinkDetail; openExisting: string; close: void }>();

  let url = link?.url ?? '';
  let title = link?.title ?? '';
  let description = link?.description ?? '';
  let saving = false;
  let error = '';
  let candidates: WebLinkSummary[] = [];

  $: label = link ? 'Webリンクを編集' : 'Webリンクを追加';

  async function submit(allowDuplicate = false) {
    if (saving) return;
    saving = true;
    error = '';
    if (!allowDuplicate) candidates = [];
    try {
      const input = { url, title, description };
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
      <input id="link-url" type="url" placeholder="https://example.com/" bind:value={url} data-initial-focus
        on:input={() => { candidates = []; error = ''; }} required disabled={saving} />
    </div>
    <div class="form-field">
      <label for="link-title">タイトル</label>
      <input id="link-title" type="text" placeholder="空欄でホスト名を表示" bind:value={title} disabled={saving} />
    </div>
    <div class="form-field">
      <label for="link-description">説明</label>
      <textarea id="link-description" class="link-description-input" rows="4" placeholder="用途やメモ（任意）"
        bind:value={description} disabled={saving} />
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
      <button type="submit" class="primary" disabled={saving}>{saving ? '保存中…' : '保存'}</button>
    </div>
  </form>
</Modal>
