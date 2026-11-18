import type { SearchKind } from './types';

export const searchKinds: Record<SearchKind, { label: string; icon: string; tone: string }> = {
  web_link: { label: 'Webリンク', icon: 'link', tone: 'link' },
  attached_page: { label: '付属ページ', icon: 'note', tone: 'link' },
  knowledge_page: { label: 'ナレッジ', icon: 'note', tone: 'knowledge' },
  diary: { label: '日記', icon: 'calendar', tone: 'diary' },
};

export const searchKindOptions = Object.entries(searchKinds) as [SearchKind, typeof searchKinds[SearchKind]][];

export function isShortQuery(query: string): boolean {
  const length = [...query.normalize('NFKC').trim().replace(/\s+/g, ' ')].length;
  return length === 1 || length === 2;
}
