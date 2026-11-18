import { writable } from 'svelte/store';
import { normalizeError } from './api';
import type { CollectionSort, ResultPage } from './types';

export const collectionLayouts = {
  gallery: { label: 'ギャラリー', icon: 'grid' },
  list: { label: 'リスト', icon: 'list' },
  table: { label: 'テーブル', icon: 'table' },
};

export const collectionSorts: Record<CollectionSort, string> = {
  updated_desc: '更新が新しい順',
  updated_asc: '更新が古い順',
  created_desc: '作成が新しい順',
  created_asc: '作成が古い順',
  title_asc: 'タイトル昇順',
  title_desc: 'タイトル降順',
};

export type CollectionKind = 'web_links' | 'knowledge';
export type CollectionLayout = keyof typeof collectionLayouts;
export type CollectionPreferences = { layout: CollectionLayout; sort: CollectionSort };

export function readCollectionPreferences(kind: CollectionKind): CollectionPreferences {
  const defaults: CollectionPreferences = { layout: 'gallery', sort: 'updated_desc' };
  try {
    const value = JSON.parse(localStorage.getItem(`oboromi-collection-${kind}`) ?? 'null');
    if (value && Object.hasOwn(collectionLayouts, value.layout)) defaults.layout = value.layout;
    if (value && Object.hasOwn(collectionSorts, value.sort)) defaults.sort = value.sort;
  } catch { /* Use the default view. */ }
  return defaults;
}

interface CollectionState<T> extends ResultPage<T> {
  loading: boolean;
  error: string;
}

export function createCollection<T>(fetchPage: (cursor?: string) => Promise<ResultPage<T>>) {
  const store = writable<CollectionState<T>>();
  let current: CollectionState<T> = { items: [], loading: false, error: '' };
  let request = 0;
  let requested = false;

  function publish(next: CollectionState<T>) {
    current = next;
    store.set(next);
  }

  publish(current);

  function invalidate() {
    request += 1;
    publish({ items: [], loading: true, error: '' });
  }

  async function load(reset = true) {
    if (!reset && (current.loading || !current.nextCursor)) return;
    requested = true;
    const seq = ++request;
    const cursor = reset ? undefined : current.nextCursor;
    publish({ ...current, loading: true, error: '' });
    try {
      const page = await fetchPage(cursor);
      if (seq === request) {
        publish({
          ...page,
          items: reset ? page.items : [...current.items, ...page.items],
          loading: false,
          error: '',
        });
      }
    } catch (error) {
      if (seq === request) publish({ ...current, loading: false, error: normalizeError(error).message });
    }
  }

  function ensureLoaded() {
    if (!requested) void load();
  }

  function refresh() {
    if (requested) void load();
  }

  function clear() {
    request += 1;
    requested = false;
    publish({ items: [], loading: false, error: '' });
  }

  return { subscribe: store.subscribe, load, ensureLoaded, refresh, clear, invalidate };
}
