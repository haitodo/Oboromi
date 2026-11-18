<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { api, normalizeError } from './api';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import type { PageView } from './types';

  const dispatch = createEventDispatcher<{ created: PageView; close: void }>();
  let title = '';
  let saving = false;
  let error = '';

  async function submit() {
    if (saving) return;
    saving = true;
    error = '';
    try {
      dispatch('created', await api.createKnowledgePage(title));
    } catch (err) {
      error = normalizeError(err).message;
    } finally {
      saving = false;
    }
  }
</script>

<Modal label="ナレッジページを追加" closeDisabled={saving} on:close={() => dispatch('close')}>
  <div class="modal-header">
    <span class="row-icon tone-knowledge"><Icon name="note" size={18} /></span>
    <h2>ナレッジページを追加</h2>
    <button type="button" class="ghost icon-button" aria-label="閉じる" disabled={saving} on:click={() => dispatch('close')}>
      <Icon name="close" size={18} />
    </button>
  </div>
  <form class="modal-body" on:submit|preventDefault={() => void submit()}>
    <div class="form-field">
      <label for="new-knowledge-title">タイトル</label>
      <input id="new-knowledge-title" type="text" placeholder="空欄で「無題」" bind:value={title} data-initial-focus disabled={saving} />
    </div>
    {#if error}<div class="notice-box error" role="alert">{error}</div>{/if}
    <div class="form-actions">
      <button type="button" on:click={() => dispatch('close')} disabled={saving}>キャンセル</button>
      <button type="submit" class="primary" disabled={saving}>{saving ? '作成中…' : '作成して開く'}</button>
    </div>
  </form>
</Modal>
