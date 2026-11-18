<script lang="ts">
  import { createEventDispatcher, onDestroy, tick } from 'svelte';
  import { api, normalizeError } from './api';
  import Icon from './Icon.svelte';
  import MilkdownEditor from './MilkdownEditor.svelte';
  import type { PageView, SaveReceipt } from './types';

  export let page: Pick<PageView, 'id' | 'bodyMarkdown' | 'revision'> | null;
  export let diaryDate = '';
  export let exportAvailable = false;

  const dispatch = createEventDispatcher<{
    committed: { receipt: SaveReceipt; markdown: string };
    reloaded: PageView | null;
    openExternalUrl: string;
    export: void;
  }>();

  let savedBody = page?.bodyMarkdown ?? '';
  let revision = page?.revision ?? null;
  let draft = savedBody;
  let initialMarkdown = savedBody;
  let raw = false;
  let composing = false;
  let reloading = false;
  let changingMode = false;
  let richFailed = false;
  let reloadToken = 0;
  let editor: MilkdownEditor | null;
  let rawInput: HTMLTextAreaElement | null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let inFlight: Promise<boolean> | undefined;
  let status: 'saved' | 'dirty' | 'saving' | 'error' | 'conflict' = 'saved';
  let message = '';

  $: statusLabel = {
    saved: revision === null ? '未記入' : '保存済み',
    dirty: '未保存',
    saving: '保存中…',
    error: '保存エラー',
    conflict: '編集の競合',
  }[status];

  onDestroy(clearTimer);

  function clearTimer() {
    clearTimeout(timer);
    timer = undefined;
  }

  function needsSave() {
    return draft !== savedBody && (revision !== null || draft.trim() !== '');
  }

  function syncEditor() {
    if (raw) return;
    const current = editor?.readMarkdown();
    if (current !== undefined) draft = current;
  }

  function updateDraft(markdown: string) {
    draft = markdown;
    if (status === 'conflict' || status === 'error' || inFlight) return;
    status = needsSave() ? 'dirty' : 'saved';
    scheduleAutosave();
  }

  function markRichChange() {
    if (status === 'conflict' || status === 'error' || inFlight) return;
    status = 'dirty';
    scheduleAutosave();
  }

  function scheduleAutosave() {
    clearTimer();
    if (status === 'dirty' && !composing) timer = setTimeout(() => void flush(), 800);
  }

  function setComposition(active: boolean) {
    composing = active;
    scheduleAutosave();
  }

  export function flush(retry = false): Promise<boolean> {
    clearTimer();
    if (composing || reloading) return Promise.resolve(false);
    if (inFlight) return inFlight;
    if (status === 'conflict' || (status === 'error' && !retry)) return Promise.resolve(false);
    inFlight = saveLatest().finally(() => (inFlight = undefined));
    return inFlight;
  }

  async function saveLatest(): Promise<boolean> {
    try {
      syncEditor();
      while (needsSave()) {
        if (composing) {
          status = 'dirty';
          return false;
        }
        status = 'saving';
        message = '';
        const markdown = draft.replace(/\r\n?/g, '\n');
        const receipt = diaryDate
          ? await api.saveDiary(diaryDate, markdown, revision)
          : await api.savePage(page!.id, markdown, revision!);
        if (draft.replace(/\r\n?/g, '\n') === markdown) draft = markdown;
        savedBody = markdown;
        revision = receipt?.revision ?? null;
        if (receipt) dispatch('committed', { receipt, markdown });
        if (composing) {
          status = 'dirty';
          return false;
        }
        syncEditor();
      }
      status = 'saved';
      return true;
    } catch (err) {
      const error = normalizeError(err);
      status = error.code === 'RevisionConflict' ? 'conflict' : 'error';
      message = error.message;
      return false;
    }
  }

  async function changeMode(nextRaw: boolean) {
    if (raw === nextRaw || composing || changingMode || reloading) return;
    changingMode = true;
    try {
      if (!nextRaw && !(await flush())) return;
      syncEditor();
      initialMarkdown = draft;
      raw = nextRaw;
      if (raw) {
        updateDraft(draft);
        await tick();
        rawInput?.focus();
      } else {
        richFailed = false;
      }
    } finally {
      changingMode = false;
    }
  }

  function replaceDraft() {
    clearTimer();
    draft = savedBody;
    initialMarkdown = savedBody;
    reloadToken += 1;
    status = 'saved';
    message = '';
  }

  async function reloadFromDisk() {
    if (reloading) return;
    reloading = true;
    try {
      const latest = diaryDate ? await api.getDiary(diaryDate) : await api.getPage(page!.id);
      savedBody = latest?.bodyMarkdown ?? '';
      revision = latest?.revision ?? null;
      replaceDraft();
      dispatch('reloaded', latest);
    } catch (error) {
      message = normalizeError(error).message;
    } finally {
      reloading = false;
    }
  }

  async function copyDraft() {
    syncEditor();
    try {
      await navigator.clipboard.writeText(draft);
      message = '下書きをコピーしました。';
    } catch {
      message = '原文表示に切り替えて下書きをコピーしてください。';
    }
  }
</script>

<section class="editor-area" aria-label="本文">
  <div class="editor-toolbar">
    <div class="editor-modes" role="group" aria-label="編集モード">
      <button type="button" aria-pressed={!raw} title="本文で / を入力してブロックを挿入"
        disabled={changingMode || reloading} on:click={() => void changeMode(false)}>
        <Icon name="pen" size={15} /> リッチ編集
      </button>
      <button type="button" aria-pressed={raw} disabled={changingMode || reloading} on:click={() => void changeMode(true)}>
        <Icon name="code" size={16} /> 原文表示
      </button>
    </div>
    <div class="editor-actions">
      <span class="save-status" class:save-alert={status === 'error' || status === 'conflict'} role="status">
        {#if status === 'saved' && revision !== null}<Icon name="check" size={14} />{:else}<span class="status-dot" class:pending={status !== 'saved'} aria-hidden="true" />{/if}
        {statusLabel}
      </span>
      <button type="button" class="ghost compact-button" title="今すぐ保存 (Ctrl+S)" aria-label="今すぐ保存 (Ctrl+S)"
        disabled={status === 'saved' || status === 'saving' || status === 'conflict' || reloading}
        on:click={() => void flush(true)}>
        保存
      </button>
      {#if exportAvailable}
        <button type="button" class="ghost icon-button" aria-label="Markdownを書き出す" title="Markdownを書き出す"
          on:click={() => dispatch('export')}>
          <Icon name="download" size={16} />
        </button>
      {/if}
    </div>
  </div>

  {#if status === 'conflict' || status === 'error'}
    <div class="notice-box" class:warn={status === 'conflict'} class:error={status === 'error'} role="alert">
      <div>{message}</div>
      <div class="notice-actions">
        {#if status === 'conflict'}
          <button type="button" on:click={() => void copyDraft()} disabled={reloading}>下書きをコピー</button>
          <button type="button" on:click={() => void reloadFromDisk()} disabled={reloading}>保存済みの内容を再読み込み</button>
        {:else}
          <button type="button" class="primary" on:click={() => void flush(true)}>再試行</button>
          <button type="button" on:click={replaceDraft}>下書きを破棄</button>
        {/if}
      </div>
    </div>
  {:else if richFailed && !raw}
    <div class="notice-box error" role="alert">
      リッチエディタを開けませんでした。
      <button type="button" on:click={() => void changeMode(true)}>原文表示で編集</button>
    </div>
  {/if}

  <div class="editor-surface" inert={reloading}>
    {#if raw}
      <textarea
        class="markdown-textarea"
        aria-label={diaryDate ? '日記のMarkdown原文' : 'ページのMarkdown原文'}
        placeholder="Markdown原文を入力…"
        rows="4"
        spellcheck="false"
        bind:this={rawInput}
        value={draft}
        on:input={(event) => updateDraft(event.currentTarget.value)}
        on:compositionstart={() => setComposition(true)}
        on:compositionend={(event) => {
          composing = false;
          updateDraft(event.currentTarget.value);
        }}
      />
    {:else}
      {#key reloadToken}
        <MilkdownEditor
          bind:this={editor}
          initialValue={initialMarkdown}
          label={diaryDate ? '日記の本文' : 'ページの本文'}
          placeholder={diaryDate ? '日記を書き始める…' : '本文を書き始める…'}
          on:change={markRichChange}
          on:compositionchange={(event) => setComposition(event.detail)}
          on:openExternalUrl
          on:failed={() => (richFailed = true)}
        />
      {/key}
    {/if}
  </div>
</section>
