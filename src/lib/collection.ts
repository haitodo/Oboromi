import { writable } from 'svelte/store';
import { normalizeError } from './api';
import type { ResultPage } from './types';

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

  function clear() {
    request += 1;
    requested = false;
    publish({ items: [], loading: false, error: '' });
  }

  return { subscribe: store.subscribe, load, ensureLoaded, clear, invalidate };
}
