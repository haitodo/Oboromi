<script lang="ts">
  import type { Tag } from './types';

  export let tags: Tag[];
</script>

{#if tags.length}
  <span class="collection-tag-labels">
    {#each tags.slice(0, 3) as tag (tag.id)}
      <span class="collection-tag" title={tag.name}>{tag.name}</span>
    {/each}
    {#if tags.length > 3}
      <span class="collection-tag-more" title={tags.slice(3).map((tag) => tag.name).join('、')}>+{tags.length - 3}</span>
    {/if}
  </span>
{/if}
