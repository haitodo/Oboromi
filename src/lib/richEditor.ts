import { CrepeBuilder } from '@milkdown/crepe/builder';
import { blockEdit } from '@milkdown/crepe/feature/block-edit';
import { codeMirror } from '@milkdown/crepe/feature/code-mirror';
import { cursor } from '@milkdown/crepe/feature/cursor';
import { linkTooltip } from '@milkdown/crepe/feature/link-tooltip';
import { listItem } from '@milkdown/crepe/feature/list-item';
import { placeholder } from '@milkdown/crepe/feature/placeholder';
import { table } from '@milkdown/crepe/feature/table';
import { toolbar } from '@milkdown/crepe/feature/toolbar';
import { Plugin } from '@milkdown/kit/prose/state';
import { $prose } from '@milkdown/kit/utils';
import '@milkdown/crepe/theme/common/prosemirror.css';
import '@milkdown/crepe/theme/common/reset.css';
import '@milkdown/crepe/theme/common/block-edit.css';
import '@milkdown/crepe/theme/common/code-mirror.css';
import '@milkdown/crepe/theme/common/cursor.css';
import '@milkdown/crepe/theme/common/link-tooltip.css';
import '@milkdown/crepe/theme/common/list-item.css';
import '@milkdown/crepe/theme/common/placeholder.css';
import '@milkdown/crepe/theme/common/toolbar.css';
import '@milkdown/crepe/theme/common/table.css';
import '@milkdown/crepe/theme/frame.css';

export function createRichEditor(root: HTMLElement, defaultValue: string, prompt: string, onChange: () => void) {
  const editor = new CrepeBuilder({ root, defaultValue })
    .addFeature(codeMirror)
    .addFeature(listItem)
    .addFeature(linkTooltip)
    .addFeature(cursor)
    .addFeature(blockEdit, {
      textGroup: {
        label: 'テキスト', text: { label: '段落' },
        h1: { label: '見出し 1' }, h2: { label: '見出し 2' }, h3: { label: '見出し 3' },
        h4: { label: '見出し 4' }, h5: { label: '見出し 5' }, h6: { label: '見出し 6' },
        quote: { label: '引用' }, divider: { label: '区切り線' },
      },
      listGroup: {
        label: 'リスト', bulletList: { label: '箇条書き' },
        orderedList: { label: '番号付きリスト' }, taskList: { label: 'タスクリスト' },
      },
      advancedGroup: { label: 'その他', codeBlock: { label: 'コードブロック' }, table: { label: '表' } },
    })
    .addFeature(toolbar)
    .addFeature(placeholder, { text: prompt, mode: 'doc' })
    .addFeature(table);

  editor.editor.use($prose(() => new Plugin({
    view: () => ({
      update(view, previousState) {
        if (view.state.doc !== previousState.doc) onChange();
      },
    }),
  })));
  return editor;
}
