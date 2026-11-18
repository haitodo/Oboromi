<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import Icon from './Icon.svelte';
  import type { WebLinkSummary } from './types';

  export let saved: Pick<WebLinkSummary, 'id' | 'hasThumbnail' | 'updatedAtMs'> | undefined = undefined;
  export let source: string | null = null;

  let element: HTMLSpanElement;
  let nearViewport = false;
  let cached: string | null = null;
  let failed: string | null = null;
  let revision = '';
  let request = 0;

  $: if (nearViewport) void load(saved);
  $: image = source ?? cached;

  onMount(() => {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        nearViewport = true;
        observer.disconnect();
      }
    }, { rootMargin: '180px' });
    observer.observe(element);
    return () => {
      observer.disconnect();
      request += 1;
    };
  });

  async function load(link: typeof saved) {
    const key = link?.hasThumbnail ? `${link.id}:${link.updatedAtMs}` : '';
    if (key === revision) return;
    revision = key;
    const sequence = ++request;
    cached = null;
    failed = null;
    if (!link?.hasThumbnail) return;
    const thumbnail = await api.getWebLinkThumbnail(link.id).catch(() => null);
    if (sequence === request) cached = thumbnail;
  }
</script>

<span class="web-thumbnail" bind:this={element} aria-hidden="true">
  {#if image && image !== failed}
    <img src={image} alt="" decoding="async" on:error={() => (failed = image)} />
  {:else}
    <span class="web-thumbnail-symbol"><Icon name="link" size={25} /></span>
  {/if}
</span>
