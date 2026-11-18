<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import Icon from './Icon.svelte';
  import { formatTimestamp } from './jst';
  import type { KnowledgePageSummary, WebLinkSummary } from './types';

  export let kind: 'web_links' | 'knowledge';
  export let items: (WebLinkSummary | KnowledgePageSummary)[];
  export let selectedId: string | undefined;
  export let loading: boolean;
  export let error: string;
  export let nextCursor: string | undefined = undefined;
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    select: string;
    add: void;
    external: string;
    more: void;
    retry: void;
  }>();

  $: isLinks = kind === 'web_links';
  $: label = isLinks ? 'Webリンク' : 'ナレッジ';
</script>

<div class="pane-header">
  <h2>{label}</h2>
  <button type="button" class="small-button" aria-label={`${label}を追加`} {disabled} on:click={() => dispatch('add')}>
    <Icon name="plus" size={15} /> 追加
  </button>
</div>

<div class="list-scroll" aria-busy={loading}>
  {#if error}
    <div class="notice-box error" role="alert">
      {error}
      <button type="button" class="small-button" on:click={() => dispatch('retry')}>再試行</button>
    </div>
  {/if}
  {#if items.length === 0 && !error}
    <p class="list-empty" role="status">{loading ? '読み込み中…' : `${label}はまだありません。`}</p>
  {/if}
  {#each items as item (item.id)}
    <div class="collection-row" class:selected={item.id === selectedId}>
      <button type="button" class="list-row" class:selected={item.id === selectedId}
        aria-current={item.id === selectedId ? 'true' : undefined} on:click={() => dispatch('select', item.id)}>
        <span class="row-icon" class:tone-link={isLinks} class:tone-knowledge={!isLinks}>
          <Icon name={isLinks ? 'link' : 'note'} size={16} />
        </span>
        <span class="row-labels">
          <span class="row-title">{item.displayTitle}</span>
          <span class="row-sub" title={'url' in item ? item.url : undefined}>
            {'url' in item ? item.url : formatTimestamp(item.updatedAtMs)}
          </span>
        </span>
      </button>
      {#if 'normalizedUrl' in item}
        <button type="button" class="ghost icon-button collection-external"
          title="ブラウザで開く" aria-label={`${item.displayTitle}をブラウザで開く`}
          on:click={() => dispatch('external', item.normalizedUrl)}>
          <Icon name="external" size={15} />
        </button>
      {/if}
    </div>
  {/each}
  {#if nextCursor}
    <button type="button" class="list-more" disabled={loading} on:click={() => dispatch('more')}>
      {loading ? '読み込み中…' : 'さらに表示'}
    </button>
  {/if}
</div>
