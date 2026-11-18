<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { collectionLayouts, collectionSorts } from './collection';
  import type { CollectionKind, CollectionLayout, CollectionPreferences } from './collection';
  import CollectionTagLabels from './CollectionTagLabels.svelte';
  import Icon from './Icon.svelte';
  import { formatTimestamp } from './jst';
  import type { CollectionSort, KnowledgePageSummary, WebLinkSummary } from './types';
  import WebLinkThumbnail from './WebLinkThumbnail.svelte';

  export let kind: CollectionKind;
  export let preferences: CollectionPreferences;
  export let inbox = false;
  export let query = '';
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
    layout: CollectionLayout;
    sort: CollectionSort;
    query: string;
    tag: string;
  }>();

  $: isLinks = kind === 'web_links';
  $: label = isLinks ? 'Webリンク' : 'ナレッジ';
  $: icon = isLinks ? 'link' : 'note';
  $: layout = inbox ? 'list' : preferences.layout;

  function changeLayout(event: Event & { currentTarget: HTMLSelectElement }) {
    dispatch('layout', event.currentTarget.value as CollectionLayout);
  }

  function changeSort(event: Event & { currentTarget: HTMLSelectElement }) {
    dispatch('sort', event.currentTarget.value as CollectionSort);
  }

  function linkHost(link: WebLinkSummary) {
    return new URL(link.normalizedUrl).hostname;
  }
</script>

<header class="collection-header" class:overview={!inbox}>
  <div class="collection-heading">
    {#if inbox}
      <h2>受信箱</h2>
      <button type="button" class="ghost icon-button" aria-label={`${label}を追加`} title={`${label}を追加`} {disabled}
        on:click={() => dispatch('add')}><Icon name="plus" size={16} /></button>
    {:else}
      <span class="row-icon" class:tone-link={isLinks} class:tone-knowledge={!isLinks}><Icon name={icon} size={17} /></span>
      <h1 tabindex="-1">{label}</h1>
    {/if}
  </div>
  {#if !inbox}
    <div class="collection-options">
      <label class="collection-search">
        <Icon name="search" size={15} />
        <input type="search" value={query} placeholder="一覧内を検索" aria-label={`${label}内を検索`} {disabled}
          on:input={(event) => dispatch('query', event.currentTarget.value)} />
      </label>
      <label class="collection-select">
        <Icon name={collectionLayouts[layout].icon} size={15} />
        <select aria-label="表示形式" value={layout} {disabled} on:change={changeLayout}>
          {#each Object.entries(collectionLayouts) as [value, option]}
            <option {value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="collection-select">
        <Icon name="sort" size={15} />
        <select aria-label="並び替え" value={preferences.sort} {disabled} on:change={changeSort}>
          {#each Object.entries(collectionSorts) as [value, caption]}
            <option {value}>{caption}</option>
          {/each}
        </select>
      </label>
    </div>
  {/if}
</header>

<div class="collection-body" class:overview={!inbox} aria-busy={loading}>
  {#if error}
    <div class="notice-box error" role="alert">
      {error}
      <button type="button" class="small-button" on:click={() => dispatch('retry')}>再試行</button>
    </div>
  {/if}
  {#if items.length === 0 && !error}
    <p class="list-empty" role="status">{loading ? '読み込み中…' : query.trim() ? '一致する項目がありません。' : `${label}はまだありません。`}</p>
  {/if}
  {#if layout === 'gallery'}
    <div class="collection-gallery">
      {#each items as item (item.id)}
        <article class="gallery-card" class:selected={item.id === selectedId}>
          <button type="button" class="gallery-open" title={item.displayTitle} aria-label={item.displayTitle}
            aria-current={item.id === selectedId ? 'true' : undefined} on:click={() => dispatch('select', item.id)}>
            {#if 'normalizedUrl' in item}
              <WebLinkThumbnail saved={item} />
            {:else}
              <span class="knowledge-cover" aria-hidden="true">
                {#if item.excerpt}<span>{item.excerpt}</span>{:else}<Icon name="note" size={27} />{/if}
              </span>
            {/if}
            <span class="gallery-title">{item.displayTitle}</span>
          </button>
          <div class="gallery-foot">
            {#if 'normalizedUrl' in item}
              <div class="gallery-source">
                <span title={item.url}>{linkHost(item)}</span>
                <button type="button" class="ghost icon-button collection-open-external" title="ブラウザで開く"
                  aria-label={`${item.displayTitle}をブラウザで開く`} on:click={() => dispatch('external', item.normalizedUrl)}>
                  <Icon name="external" size={14} />
                </button>
              </div>
            {:else}
              <div class="gallery-source"><Icon name="clock" size={13} /> <span>{formatTimestamp(item.updatedAtMs)}</span></div>
            {/if}
            <CollectionTagLabels tags={item.tags} on:select={(event) => dispatch('tag', event.detail)} />
          </div>
        </article>
      {/each}
    </div>
  {:else if layout === 'table' && items.length}
    <div class="collection-table-scroll">
      <table class="collection-table" aria-label={`${label}のテーブル`}>
        <thead>
          <tr>
            <th scope="col">タイトル</th>
            {#if isLinks}<th scope="col">リンク先</th>{/if}
            <th scope="col">タグ</th>
            <th scope="col">更新</th>
            <th scope="col">作成</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr class:selected={item.id === selectedId}>
              <td>
                <button type="button" class="ghost collection-table-title" title={item.displayTitle}
                  aria-current={item.id === selectedId ? 'true' : undefined} on:click={() => dispatch('select', item.id)}>
                  <Icon name={icon} size={16} /> <span>{item.displayTitle}</span>
                </button>
              </td>
              {#if 'normalizedUrl' in item}
                <td>
                  <button type="button" class="ghost collection-table-url" aria-label={`${item.displayTitle}をブラウザで開く`}
                    title={item.url} on:click={() => dispatch('external', item.normalizedUrl)}>
                    <span>{linkHost(item)}</span> <Icon name="external" size={13} />
                  </button>
                </td>
              {/if}
              <td><CollectionTagLabels tags={item.tags} on:select={(event) => dispatch('tag', event.detail)} /></td>
              <td class="collection-time">{formatTimestamp(item.updatedAtMs)}</td>
              <td class="collection-time">{formatTimestamp(item.createdAtMs)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    {#each items as item (item.id)}
      <div class="collection-row" class:selected={item.id === selectedId}>
        <button type="button" class="list-row"
          aria-current={item.id === selectedId ? 'true' : undefined} on:click={() => dispatch('select', item.id)}>
          <span class="row-icon" class:tone-link={isLinks} class:tone-knowledge={!isLinks}>
            <Icon name={icon} size={16} />
          </span>
          <span class="row-labels">
            <span class="row-title">{item.displayTitle}</span>
            <span class="row-sub" title={'url' in item ? item.url : undefined}>
              {'url' in item ? item.url : formatTimestamp(item.updatedAtMs)}
            </span>
          </span>
        </button>
        {#if !inbox}<CollectionTagLabels tags={item.tags} on:select={(event) => dispatch('tag', event.detail)} />{/if}
        {#if 'normalizedUrl' in item}
          <button type="button" class="ghost icon-button collection-external"
            title="ブラウザで開く" aria-label={`${item.displayTitle}をブラウザで開く`}
            on:click={() => dispatch('external', item.normalizedUrl)}>
            <Icon name="external" size={15} />
          </button>
        {/if}
      </div>
    {/each}
  {/if}
  {#if nextCursor}
    <button type="button" class="list-more" disabled={loading} on:click={() => dispatch('more')}>
      {loading ? '読み込み中…' : 'さらに表示'}
    </button>
  {/if}
</div>
