<script lang="ts">
  export let name: string;
  export let size = 18;

  const paths: Record<string, string> = {
    calendar: 'M7 3v4m10-4v4M4 10h16M6 5h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Zm3 10h2m3 0h1',
    book: 'M6 3h12a2 2 0 0 1 2 2v15H6a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3Zm14 13H6a2 2 0 0 0 0 4m2-12h7m-7 4h5',
    link: 'M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71m2.25 5.82a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71',
    note: 'M13 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-9Zm0 0v7h7M8 14h8m-8 3h5',
    tag: 'M3 4v7l9.5 9.5a1.5 1.5 0 0 0 2.1 0l6-6a1.5 1.5 0 0 0 0-2.1L11 3H4a1 1 0 0 0-1 1Zm4 3h.01',
    settings: 'M4 7h16M4 17h16M8 4v6m8 4v6',
    search: 'M15.8 15.8 21 21M17.5 10.5a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z',
    plus: 'M12 5v14M5 12h14',
    close: 'm6 6 12 12M18 6 6 18',
    left: 'm14 6-6 6 6 6',
    right: 'm10 6 6 6-6 6',
    down: 'm6 9 6 6 6-6',
    up: 'm6 15 6-6 6 6',
    panel: 'M9 4v16M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z',
    external: 'M13 4h7v7m0-7-9 9m6-1v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h7',
    code: 'm8 6-6 6 6 6m8-12 6 6-6 6m-2-15-4 18',
    pen: 'm14 6 4 4M4 20l1-5L16 4a2.8 2.8 0 0 1 4 4L9 19l-5 1Z',
    check: 'm5 12 4 4L19 6',
    download: 'M12 3v12m-4-4 4 4 4-4M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5m4-5v5',
    clock: 'M12 7v5l3 2m6-2a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z',
    storage: 'M4 8c0-2 3.6-4 8-4s8 2 8 4-3.6 4-8 4-8-2-8-4Zm0 0v8c0 2 3.6 4 8 4s8-2 8-4V8m-16 4c0 2 3.6 4 8 4s8-2 8-4',
  };
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
  stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === 'dots'}
    <circle cx="5" cy="12" r="1" fill="currentColor" />
    <circle cx="12" cy="12" r="1" fill="currentColor" />
    <circle cx="19" cy="12" r="1" fill="currentColor" />
  {:else}
    <path d={paths[name] ?? paths.note} />
  {/if}
</svg>
