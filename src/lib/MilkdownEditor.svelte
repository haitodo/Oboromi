<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from 'svelte';
  import type { CrepeBuilder } from '@milkdown/crepe/builder';

  export let initialValue = '';
  export let placeholder: string;
  export let label: string;

  const dispatch = createEventDispatcher<{
    change: void;
    compositionchange: boolean;
    openExternalUrl: string;
    failed: void;
  }>();

  let rootEl: HTMLDivElement | null = null;
  let crepe: CrepeBuilder | null = null;
  let ready = false;
  let cancelled = false;
  let baselineMarkdown = '';
  let changed = false;
  let failed = false;

  // Serialize the live document only when the caller needs its Markdown.
  export function readMarkdown(): string | undefined {
    if (!crepe || !ready || !changed) return;
    const markdown = crepe.getMarkdown();
    changed = false;
    return markdown === baselineMarkdown ? initialValue : markdown;
  }

  onMount(async () => {
    if (!rootEl) return;
    try {
      const { createRichEditor } = await import('./richEditor');
      if (cancelled) return;
      const instance = createRichEditor(rootEl, initialValue, placeholder, () => {
        if (!ready) return;
        changed = true;
        dispatch('change');
      });
      crepe = instance;
      await instance.create();
      if (cancelled) {
        await instance.destroy();
        return;
      }
      const editable = rootEl.querySelector('.ProseMirror');
      editable?.setAttribute('role', 'textbox');
      editable?.setAttribute('aria-label', label);
      editable?.setAttribute('aria-multiline', 'true');
      baselineMarkdown = instance.getMarkdown();
      ready = true;
    } catch {
      if (crepe) void crepe.destroy().catch(() => {});
      crepe = null;
      if (!cancelled) {
        failed = true;
        dispatch('failed');
      }
    }
  });

  onDestroy(() => {
    cancelled = true;
    if (crepe && ready) void crepe.destroy().catch(() => {});
    ready = false;
  });

  function handleClick(e: MouseEvent) {
    const target = e.target as HTMLElement | null;
    const anchor = target?.closest<HTMLAnchorElement>('a[href]');
    if (!anchor) return;
    const href = anchor.getAttribute('href')?.trim() ?? '';
    e.preventDefault();
    e.stopPropagation();
    if (/^https?:\/\//i.test(href)) {
      dispatch('openExternalUrl', href);
    }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div
  class="oboromi-milkdown-wrap"
  aria-busy={!ready && !failed}
  bind:this={rootEl}
  on:click|capture={handleClick}
  on:auxclick|capture={handleClick}
  on:compositionstart|capture={() => dispatch('compositionchange', true)}
  on:compositionend|capture={() => dispatch('compositionchange', false)}
>
  {#if !ready && !failed}<p class="editor-loading" role="status">エディタを準備中…</p>{/if}
</div>
