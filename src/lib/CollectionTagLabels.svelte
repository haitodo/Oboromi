<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Tag } from './types';

  export let tags: Tag[];
  const dispatch = createEventDispatcher<{ select: string }>();
</script>

{#if tags.length}
  <span class="collection-tag-labels">
    {#each tags.slice(0, 3) as tag (tag.id)}
      <button type="button" class="collection-tag" title={tag.name} aria-label={`${tag.name}の関連項目を表示`}
        on:click={() => dispatch('select', tag.id)}>{tag.name}</button>
    {/each}
    {#if tags.length > 3}
      <span class="collection-tag-more" title={tags.slice(3).map((tag) => tag.name).join('、')}>+{tags.length - 3}</span>
    {/if}
  </span>
{/if}
