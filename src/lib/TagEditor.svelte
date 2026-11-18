<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Tag, TagSummary } from './types';

  export let tags: Tag[] = [];
  export let allTags: TagSummary[] = [];
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    change: { ids: string[]; name?: string };
  }>();

  let input = '';
  let isComposing = false;

  $: attachedIds = new Set(tags.map((t) => t.id));
  $: normalizedInput = input.normalize('NFKC').trim().toLowerCase();
  $: suggestions = allTags
    .filter((t) => !attachedIds.has(t.id))
    .filter(
      (t) =>
        !normalizedInput ||
        t.name.toLowerCase().includes(normalizedInput) ||
        t.nameKey.includes(normalizedInput)
    )
    .slice(0, 8);

  function addTagByName() {
    const trimmed = input.trim();
    if (!trimmed || disabled) return;
    input = '';
    dispatch('change', { ids: [...attachedIds], name: trimmed });
  }

  function attachExisting(tagId: string) {
    if (disabled || attachedIds.has(tagId)) return;
    input = '';
    dispatch('change', { ids: [...attachedIds, tagId] });
  }

  function removeTag(tagId: string) {
    if (disabled) return;
    dispatch('change', { ids: [...attachedIds].filter((id) => id !== tagId) });
  }
</script>

<div class="tag-editor">
  <div class="tag-bar">
    {#each tags as tag (tag.id)}
      <span class="tag-chip active">
        <span>{tag.name}</span>
        <button
          type="button"
          class="tag-remove"
          aria-label={`タグ ${tag.name} を外す`}
          title={`タグ ${tag.name} を外す`}
          on:click={() => removeTag(tag.id)}
          {disabled}
        >
          ×
        </button>
      </span>
    {/each}
    <input
      type="text"
      placeholder="＋ タグを追加"
      aria-label="タグを追加"
      bind:value={input}
      {disabled}
      on:focus
      on:compositionstart={() => (isComposing = true)}
      on:compositionend={() => (isComposing = false)}
      on:keydown={(e) => {
        if (e.isComposing || isComposing || e.keyCode === 229) return;
        if (e.key === 'Enter') {
          e.preventDefault();
          addTagByName();
        }
      }}
    />
    {#if input.trim()}
      <button type="button" class="compact-button" on:click={addTagByName} {disabled}>
        付与
      </button>
    {/if}
  </div>
  {#if input.trim() && suggestions.length > 0}
    <div class="tag-bar tag-suggestions">
      {#each suggestions as s (s.id)}
        <button
          type="button"
          class="tag-chip"
          on:click={() => attachExisting(s.id)}
          {disabled}
        >
          + {s.name} ({s.count})
        </button>
      {/each}
    </div>
  {/if}
</div>
