<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { api, normalizeError } from './api';
  import { currentJstDate, formatTimestamp } from './jst';
  import { searchKinds } from './search';
  import Icon from './Icon.svelte';
  import type { AppStatus, BackupInfo, RestoreInfo, TrashEntry } from './types';

  export let status: AppStatus | null = null;
  export let busy = false;
  export let theme: 'light' | 'dark' = 'light';
  export let flushBeforeMutation: () => Promise<boolean>;

  const dispatch = createEventDispatcher<{
    restored: RestoreInfo;
    trashRestored: void;
    theme: 'light' | 'dark';
    close: void;
  }>();

  let trash: TrashEntry[] = [];
  let backups: BackupInfo[] = [];
  let backupPath = '';
  let restorePath = '';
  let info = '';
  let error = '';
  let loading = true;
  const sqliteFilter = [{ name: 'SQLite バックアップ', extensions: ['sqlite3'] }];

  onMount(() => { void refresh(); });

  async function refresh() {
    loading = true;
    try {
      [trash, backups] = await Promise.all([status?.startupError ? Promise.resolve([]) : api.listTrash(), api.listBackups()]);
    } catch (issue) {
      error = normalizeError(issue).message;
    } finally {
      loading = false;
    }
  }

  async function perform(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    info = '';
    try {
      if (await flushBeforeMutation()) await action();
    } catch (issue) {
      error = normalizeError(issue).message;
    } finally {
      busy = false;
    }
  }

  function chooseBackupPath() {
    return perform(async () => {
      const name = `Oboromi-${currentJstDate()}.sqlite3`;
      const path = await api.pickSavePath({ title: 'バックアップの保存先', defaultPath: backupPath || name, filters: sqliteFilter });
      if (path) backupPath = path;
    });
  }

  function chooseRestorePath() {
    return perform(async () => {
      const path = await api.pickFile({ title: '復元するバックアップを選択', defaultPath: restorePath || status?.backupsDir, filters: sqliteFilter });
      if (path) restorePath = path;
    });
  }

  function createBackup() {
    return perform(async () => {
      const result = await api.createBackup(backupPath.trim() || undefined);
      backupPath = '';
      info = `バックアップを作成しました。${result.path}`;
      await refresh();
    });
  }

  function restoreBackup(source: string) {
    if (!source.trim() || !window.confirm('このバックアップから復元しますか？ 現在のデータベースは backups フォルダに退避されます。')) return;
    return perform(async () => {
      const result = await api.restoreBackup(source);
      info = `${result.source} から復元しました。復元前のデータベースの退避先は ${result.preRestoreBackupPath} です。`;
      dispatch('restored', result);
      await refresh();
    });
  }

  function restoreRecord(entry: TrashEntry) {
    return perform(async () => {
      if (entry.kind === 'web_link') await api.restoreWebLink(entry.id);
      else await api.restorePage(entry.id);
      info = `「${entry.title}」を復元しました。`;
      dispatch('trashRestored');
      await refresh();
    });
  }

  function rebuildIndex() {
    return perform(async () => {
      const count = await api.rebuildSearchIndex();
      info = `検索インデックスを再構築しました（${count}件）。`;
    });
  }
</script>

<article class="detail-container settings-content" aria-busy={busy || loading}>
  <header class="document-header">
    <div class="document-icon tone-diary"><Icon name="settings" size={27} /></div>
    <div class="detail-title-row"><h1 class="detail-title" tabindex="-1">設定</h1>
      <button type="button" class="ghost compact-button" on:click={() => dispatch('close')} disabled={busy || Boolean(status?.startupError)}>閉じる</button>
    </div>
  </header>

  {#if info}<div class="notice-box success" role="status">{info}</div>{/if}
  {#if error}<div class="notice-box error" role="alert">{error}</div>{/if}

  <section class="settings-group">
    <h2>外観</h2>
    <div class="filter-tabs appearance-options" role="group" aria-label="表示テーマ">
      <button type="button" aria-pressed={theme === 'light'} on:click={() => dispatch('theme', 'light')}>ライト</button>
      <button type="button" aria-pressed={theme === 'dark'} on:click={() => dispatch('theme', 'dark')}>ダーク</button>
    </div>
  </section>

  <section class="settings-group">
    <h2>保存場所</h2>
    {#if status}
      <dl class="settings-paths">
        <div><dt>データフォルダ</dt><dd>{status.dataDir}</dd></div>
        <div><dt>データベース</dt><dd>{status.dbPath}</dd></div>
        <div><dt>バックアップフォルダ</dt><dd>{status.backupsDir}</dd></div>
      </dl>
    {/if}
  </section>

  <section class="settings-group">
    <h2>バックアップと復元</h2>
    <form class="form-field" on:submit|preventDefault={() => void createBackup()}>
      <label for="backup-path">バックアップの保存先</label>
      <div class="inline-form">
        <input id="backup-path" type="text" placeholder="空欄で既定のフォルダに保存" bind:value={backupPath} disabled={busy} />
        <button type="button" disabled={busy} on:click={() => void chooseBackupPath()}>参照…</button>
      </div>
      <div class="form-actions"><button type="submit" class="primary" disabled={busy}>バックアップを作成</button></div>
    </form>

    <form class="form-field" on:submit|preventDefault={() => void restoreBackup(restorePath)}>
      <label for="restore-path">復元元のファイル</label>
      <div class="inline-form">
        <input id="restore-path" type="text" placeholder=".sqlite3 ファイルのパス" bind:value={restorePath} disabled={busy} />
        <button type="button" disabled={busy} on:click={() => void chooseRestorePath()}>参照…</button>
      </div>
      <div class="form-actions"><button type="submit" disabled={busy || !restorePath.trim()}>指定ファイルから復元</button></div>
    </form>
    {#if backups.length}
      <div class="attached-list">
        {#each backups as backup (backup.path)}
          <div class="attached-row">
            <span class="row-labels"><span class="row-title">{backup.fileName}</span>
              <span class="row-sub">{formatTimestamp(backup.createdAtMs)} · {(backup.sizeBytes / 1024).toFixed(1)} KB</span></span>
            <button type="button" disabled={busy} on:click={() => void restoreBackup(backup.path)}>復元</button>
          </div>
        {/each}
      </div>
    {:else if !loading}
      <p class="settings-text">既定のフォルダにバックアップはありません。</p>
    {/if}
  </section>

  <section class="settings-group">
    <h2>ごみ箱</h2>
    {#if trash.length}
      <div class="attached-list">
        {#each trash as entry (`${entry.kind}:${entry.id}`)}
          <div class="attached-row">
            <span class="row-labels"><span class="row-title">{entry.title}</span>
              <span class="row-sub">{searchKinds[entry.kind].label} · {entry.parentTitle ?? entry.url ?? formatTimestamp(entry.deletedAtMs)}</span></span>
            <button type="button" disabled={busy} on:click={() => void restoreRecord(entry)}>復元</button>
          </div>
        {/each}
      </div>
    {:else if !loading}
      <p class="settings-text">ごみ箱は空です。</p>
    {/if}
  </section>

  <section class="settings-group">
    <h2>検索メンテナンス</h2>
    <div><button type="button" disabled={busy} on:click={() => void rebuildIndex()}>検索インデックスを再構築</button></div>
  </section>
</article>
