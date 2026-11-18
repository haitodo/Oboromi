<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { normalizeError } from './api';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import type { Tag, TagSummary } from './types';

  export let tags: Tag[];
  export let allTags: TagSummary[];
  export let loading: boolean;
  export let loadError: string;
  export let save: (ids: string[], name?: string) => Promise<void>;

  const dispatch = createEventDispatcher<{ close: void; reload: void }>();
  let query = '';
  let searchInput: HTMLInputElement;
  let results: HTMLDivElement;
  let selected = new Set(tags.map((tag) => tag.id));
  let saving = false;
  let composing = false;
  let error = '';

  $: queryKey = nameKey(query);
  $: matches = allTags.filter((tag) => tag.nameKey.includes(queryKey) || nameKey(tag.name).includes(queryKey));
  $: exactMatch = matches.some((tag) => tag.nameKey === queryKey || nameKey(tag.name) === queryKey);
  $: newName = queryKey && !loading && !loadError && !exactMatch ? query.trim() : '';
  $: changed = selected.size !== tags.length || tags.some((tag) => !selected.has(tag.id));

  function nameKey(value: string) {
    return value.normalize('NFKC').toLowerCase().trim().replace(/\s+/g, ' ');
  }

  function selectTag(id: string, checked: boolean) {
    const next = new Set(selected);
    if (checked) next.add(id);
    else next.delete(id);
    selected = next;
  }

  function moveFocus(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229 || composing || saving) return;
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const target = event.target as HTMLElement;
    if (target !== searchInput && !results.contains(target)) return;
    const options = Array.from(results.querySelectorAll<HTMLElement>('input:not(:disabled), button:not(:disabled)'));
    const index = options.indexOf(target);
    const next = event.key === 'ArrowDown' ? options[index + 1] : options[index - 1] ?? searchInput;
    if (!next || next === target) return;
    event.preventDefault();
    next.focus();
  }

  async function submit(name?: string) {
    if (saving || loading || loadError || composing || (!changed && !name)) return;
    saving = true;
    error = '';
    try {
      await save([...selected], name);
      dispatch('close');
    } catch (issue) {
      error = normalizeError(issue).message;
    } finally {
      saving = false;
    }
  }
</script>

<Modal label="タグを追加" className="tag-picker-card" closeDisabled={saving}
  on:close={() => dispatch('close')} on:keydown={moveFocus}>
  <div class="modal-header">
    <span class="row-icon tone-tag"><Icon name="tag" size={17} /></span>
    <h2>タグを追加</h2>
    <button type="button" class="ghost icon-button" aria-label="閉じる" disabled={saving} on:click={() => dispatch('close')}>
      <Icon name="close" size={17} />
    </button>
  </div>

  <div class="tag-picker-body">
    <div class="tag-picker-filter">
      <Icon name="search" size={17} />
      <input type="search" aria-label="タグを検索" placeholder="タグ名で検索" data-initial-focus
        bind:this={searchInput} bind:value={query} disabled={saving}
        on:compositionstart={() => (composing = true)} on:compositionend={() => (composing = false)} />
    </div>

    <div class="tag-picker-results" bind:this={results} aria-busy={loading}>
      {#if loading}
        <p class="tag-picker-empty" role="status">タグを読み込み中…</p>
      {:else if loadError}
        <div class="notice-box error" role="alert">
          {loadError} <button type="button" class="small-button" on:click={() => dispatch('reload')}>再試行</button>
        </div>
      {:else}
        <div class="tag-picker-options">
          {#each matches as tag (tag.id)}
            <label class="tag-picker-option" class:selected={selected.has(tag.id)}>
              <Icon name="tag" size={16} />
              <span class="tag-picker-name">{tag.name}</span>
              <span class="tag-count">{tag.count}件</span>
              <input type="checkbox" aria-label={tag.name} checked={selected.has(tag.id)} disabled={saving}
                on:change={(event) => selectTag(tag.id, event.currentTarget.checked)} />
            </label>
          {:else}
            <p class="tag-picker-empty">{queryKey ? '一致するタグがありません' : 'タグ名を入力して新しいタグを作成できます'}</p>
          {/each}
        </div>
        {#if newName}
          <button type="button" class="ghost tag-picker-create" title={newName} disabled={saving || composing} on:click={() => void submit(newName)}>
            <Icon name="plus" size={16} /> <span>「{newName}」を作成して適用</span>
          </button>
        {/if}
      {/if}
    </div>
    {#if error}<div class="notice-box error" role="alert">{error}</div>{/if}
  </div>

  <div class="tag-picker-footer">
    <span class="tag-count" role="status">{selected.size} 件選択中</span>
    <button type="button" disabled={saving} on:click={() => dispatch('close')}>キャンセル</button>
    <button type="button" class="primary" disabled={!changed || loading || Boolean(loadError) || saving}
      on:click={() => void submit()}>{saving ? '保存中…' : '適用'}</button>
  </div>
</Modal>

<style>
  .tag-picker-body { display: flex; flex-direction: column; min-height: 0; gap: 8px; padding: 12px 16px; }
  .tag-picker-filter {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-panel);
    color: var(--text-secondary);
  }
  .tag-picker-filter:focus-within { border-color: var(--accent); }
  .tag-picker-filter input { flex: 1; padding-inline: 4px; border: 0; background: transparent; color: var(--text-primary); }
  .tag-picker-filter input:focus-visible { outline: none; }
  .tag-picker-results { display: flex; flex-direction: column; min-height: 0; min-width: 0; gap: 4px; }
  .tag-picker-options {
    max-height: min(42dvh, 340px);
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: var(--border-strong) transparent;
  }
  .tag-picker-empty { margin: 0; padding: 12px 4px; color: var(--text-secondary); font-size: 13px; }
  .tag-picker-option {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 5px 9px;
    border-radius: 7px;
    cursor: pointer;
  }
  .tag-picker-option:hover { background: var(--bg-hover); }
  .tag-picker-option.selected { background: var(--bg-selected); }
  .tag-picker-option :global(svg) { color: var(--text-secondary); }
  .tag-picker-name { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .tag-picker-option .tag-count { flex: none; font-size: 12px; }
  .tag-picker-option input { flex: none; width: 16px; height: 16px; margin: 0; accent-color: var(--action-bg); }
  .tag-picker-create { flex: none; width: 100%; justify-content: flex-start; text-align: start; }
  .tag-picker-create span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag-picker-footer {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 7px;
    padding: 11px 16px;
    border-top: 1px solid var(--border);
    background: var(--bg-panel);
  }
  .tag-picker-footer .tag-count { margin-inline-end: auto; font-size: 12px; }
</style>
