<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { buildCalendarGrid, formatDiaryDate, formatDiaryWeekday } from './jst';
  import Icon from './Icon.svelte';

  export let year: number;
  export let month: number;
  export let today: string;
  export let selected: string;
  export let entries: string[];
  export let loading = false;
  export let error = '';

  const dispatch = createEventDispatcher<{
    select: string;
    month: number;
    today: void;
    retry: void;
  }>();

  $: cells = buildCalendarGrid(year, month);
  $: written = new Set(entries);
  $: recentEntries = [...entries].reverse();
</script>

<div class="pane-header">
  <h2>カレンダー</h2>
  <button type="button" class="small-button" on:click={() => dispatch('today')}>今日</button>
</div>

<div class="calendar-box">
  <div class="calendar-nav">
    <strong>{year}年{month}月</strong>
    <div class="calendar-controls">
      <button type="button" class="ghost icon-button" aria-label="前月" title="前月" on:click={() => dispatch('month', -1)}>
        <Icon name="left" size={16} />
      </button>
      <button type="button" class="ghost icon-button" aria-label="翌月" title="翌月" on:click={() => dispatch('month', 1)}>
        <Icon name="right" size={16} />
      </button>
    </div>
  </div>

  <div class="calendar-grid" role="group" aria-label="月間カレンダー" aria-busy={loading}>
    {#each ['日', '月', '火', '水', '木', '金', '土'] as day}
      <span class="cal-dow" aria-hidden="true">{day}</span>
    {/each}
    {#each cells as cell (cell.date)}
      <button
        type="button"
        class="cal-day"
        class:muted={!cell.inCurrentMonth}
        class:today={cell.date === today}
        class:selected={cell.date === selected}
        aria-current={cell.date === today ? 'date' : undefined}
        aria-pressed={cell.date === selected}
        aria-label={`${formatDiaryDate(cell.date)} ${formatDiaryWeekday(cell.date)}${written.has(cell.date) ? '（記入済み）' : ''}`}
        on:click={() => dispatch('select', cell.date)}
      >
        {cell.day}
        {#if written.has(cell.date)}<span class="cal-dot" aria-hidden="true" />{/if}
      </button>
    {/each}
  </div>
</div>

<section class="month-entries" aria-label="この月の記録" aria-busy={loading}>
  <h3><Icon name="clock" size={15} /> この月の記録 {#if entries.length}<span>{entries.length}</span>{/if}</h3>
  {#if error}
    <div class="notice-box error" role="alert">
      {error}
      <button type="button" class="small-button" on:click={() => dispatch('retry')}>再試行</button>
    </div>
  {:else if loading && !entries.length}
    <p class="list-empty" role="status">読み込み中…</p>
  {:else if !entries.length}
    <p class="list-empty">この月の記録はまだありません。</p>
  {:else}
    {#each recentEntries as date (date)}
      <button type="button" class="list-row" class:selected={selected === date} aria-current={selected === date ? 'true' : undefined}
        on:click={() => dispatch('select', date)}>
        <span class="row-icon tone-diary"><Icon name="note" size={15} /></span>
        <span class="row-title">{formatDiaryDate(date, false)} <span class="row-weekday">{formatDiaryWeekday(date)}</span></span>
      </button>
    {/each}
  {/if}
</section>
