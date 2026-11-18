<script lang="ts">
  import Icon from './Icon.svelte';
  import type { TagSummary } from './types';

  export let tag: TagSummary;
  export let selected = false;

  const tones = ['tag', 'knowledge', 'link', 'diary'];
  $: tone = tones[tag.id.charCodeAt(tag.id.length - 1) % tones.length] ?? 'tag';
</script>

<button type="button" class="tag-browse-button" class:selected title={`${tag.name} (${tag.count})`}
  style:--tag-color={`var(--${tone}-ink)`} aria-pressed={selected} on:click>
  <span class={`tag-browse-icon tone-${tone}`}><Icon name="tag" size={14} /></span>
  <span class="tag-browse-name">{tag.name}</span>
  <span class="tag-count">({tag.count})</span>
</button>
