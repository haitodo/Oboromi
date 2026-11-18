<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { CollectionLayout } from './collection';
  import Icon from './Icon.svelte';
  import { formatTimestamp } from './jst';
  import { searchKinds } from './search';
  import type { SearchHit } from './types';
  import WebLinkThumbnail from './WebLinkThumbnail.svelte';

  export let layout: CollectionLayout;
  export let items: SearchHit[];
  export let loading: boolean;
  export let error: string;
  export let nextCursor: string | undefined = undefined;
  export let selectedId: string | undefined;

  const dispatch = createEventDispatcher<{ open: SearchHit; retry: void; more: void }>();
</script>

<div class="collection-body overview" aria-busy={loading}>
  {#if error}
    <div class="notice-box error" role="alert">{error}
      <button type="button" class="small-button" on:click={() => dispatch('retry')}>再試行</button>
    </div>
  {:else if !items.length}
    <p class="list-empty" role="status">{loading ? '検索中…' : '条件に一致する項目はありません。'}</p>
  {/if}

  {#if layout === 'table' && items.length}
    <div class="collection-table-scroll">
      <table class="collection-table tag-results-table" aria-label="タグの関連項目">
        <thead><tr><th scope="col">タイトル</th><th scope="col">種別</th><th scope="col">更新</th></tr></thead>
        <tbody>
          {#each items as hit (`${hit.kind}:${hit.id}`)}
            {@const appearance = searchKinds[hit.kind]}
            <tr class:selected={hit.id === selectedId}>
              <td>
                <button type="button" class="ghost collection-table-title" title={hit.title}
                  aria-current={hit.id === selectedId ? 'true' : undefined} on:click={() => dispatch('open', hit)}>
                  <Icon name={appearance.icon} size={16} /> <span>{hit.title}</span>
                </button>
              </td>
              <td>{appearance.label}</td>
              <td class="collection-time">{formatTimestamp(hit.updatedAtMs)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class:collection-gallery={layout === 'gallery'} class:tag-results-gallery={layout === 'gallery'}>
      {#each items as hit (`${hit.kind}:${hit.id}`)}
        {@const appearance = searchKinds[hit.kind]}
        {@const selected = hit.id === selectedId}
        {#if layout === 'gallery'}
          <article class="gallery-card" class:selected>
            <button type="button" class="gallery-open" title={hit.title} aria-label={hit.title}
              aria-current={selected ? 'true' : undefined} on:click={() => dispatch('open', hit)}>
              <span class="tag-result-kind"><Icon name={appearance.icon} size={15} /> {appearance.label}</span>
              {#if hit.kind === 'web_link'}
                <WebLinkThumbnail saved={{ id: hit.id, hasThumbnail: true, updatedAtMs: hit.updatedAtMs }} />
              {/if}
              <span class="gallery-title">{hit.title}</span>
              <span class="tag-result-caption">{hit.parent?.title ?? formatTimestamp(hit.updatedAtMs)}</span>
            </button>
          </article>
        {:else}
          <button type="button" class="list-row" class:selected aria-current={selected ? 'true' : undefined}
            on:click={() => dispatch('open', hit)}>
            <span class={'row-icon tone-' + appearance.tone}><Icon name={appearance.icon} size={16} /></span>
            <span class="row-labels"><span class="row-title">{hit.title}</span>
              <span class="row-sub">{appearance.label} · {hit.parent?.title ?? formatTimestamp(hit.updatedAtMs)}</span></span>
          </button>
        {/if}
      {/each}
    </div>
  {/if}
  {#if nextCursor}
    <button type="button" class="list-more" disabled={loading} on:click={() => dispatch('more')}>
      {loading ? '読み込み中…' : 'さらに表示'}
    </button>
  {/if}
</div>
