<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import Icon from './Icon.svelte';
  import type { Tag } from './types';

  export let tags: Tag[] = [];
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    open: void;
    remove: string;
  }>();
</script>

<div class="tag-bar tag-editor">
  {#each tags as tag (tag.id)}
    <span class="tag-chip active">
      <span class="tag-chip-name">{tag.name}</span>
      <button
        type="button"
        class="tag-remove"
        aria-label={`タグ ${tag.name} を外す`}
        title={`タグ ${tag.name} を外す`}
        on:click={() => dispatch('remove', tag.id)}
        {disabled}
      >
        <Icon name="close" size={12} />
      </button>
    </span>
  {/each}
  <button type="button" class="ghost compact-button tag-add-button" aria-haspopup="dialog" {disabled}
    on:click={() => dispatch('open')}>
    <Icon name="plus" size={15} /> タグを追加
  </button>
</div>
