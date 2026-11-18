# Oboromi — Windows向け個人コレクションアプリ仕様書

| 項目 | 内容 |
|---|---|
| 文書バージョン | 0.7 |
| 更新日 | 2026-11-18 |
| 対象 | Windows 11での個人利用 |
| 状態 | MVP実装用仕様。性能値は計測目標 |
| 前版 | 0.6 |

## 1. 目的

よく使うWebサイト、記事・動画のURL、そのURLに紐づく使い方や注意点、日々の記録をWindows上で管理する。主な流れは **URLを登録する → Markdownでメモを足す → 検索して見返す** と **日付を選ぶ → 日記を書く → 後から読み返す** の2つ。

- Webリンクは単独で保存でき、必要に応じて複数のMarkdownページを持てる。
- 調べたことや独立したメモは、親リンクを持たないナレッジページとして保存できる。
- 付属ページ、ナレッジ、日記はMarkdownベースのWYSIWYGエディタ（`@milkdown/crepe`）でインプレース編集し、必要に応じて「原文表示」からMarkdown原文を直接編集できる。
- データは端末上のSQLiteに保存する。個別の本文は`.md`に書き出せる。リンクやタグを含むライブラリ全体の保存と復元にはSQLiteバックアップを使う。
- CapacitiesとNotionは、サイドバーから対象を選び、見出し・プロパティ・本文を同じ画面で確認でき、本文をそのまま直接編集できる情報配置・操作感の参考とする。Oboromiの画面構成と配色は第7章で定める。

## 2. 技術構成

| 層 | 採用技術 | 責務 |
|---|---|---|
| デスクトップ | Tauri 2 / Rust / WebView2 | アプリ起動、ローカルファイル操作、既定ブラウザ起動 |
| UI | Svelte / TypeScript | 画面、検索操作、入力・保存状態 |
| Markdown編集 | `@milkdown/crepe` / HTML `textarea` | WYSIWYGインプレース編集（`/`コマンド、ツールバー、CodeMirror、表・タスクリスト）および原文表示切替時の原文編集 |
| Markdown解析 | Rustの`pulldown-cmark` | 検索用テキスト抽出と閲覧用サニタイズ済みHTML生成 |
| 永続化 | `rusqlite`のbundled SQLite | 正本、トランザクション、バックアップ |
| 検索 | SQLite B-tree / FTS5 `trigram` | タイトルやタグの前方一致、日本語を含む本文の部分一致 |

UIからは型付きのTauri IPCで操作を呼び出し、Rustが検証と永続化を行う。依存関係の導入とフロントエンドのコマンド実行には`pnpm`を使い、Windows向け配布物はNSIS（`.exe`インストーラ）としてビルドする。Windows配布ビルドでFTS5と`trigram`の利用を検証する。

## 3. MVPの完了条件

| ID | 機能 | 完了条件 |
|---|---|---|
| D-01〜03 | 日記・カレンダー | JSTの1日につき1ページ。月間の記入済み表示と、今日・前日・翌日・日付選択で移動できる |
| W-01〜03 | Webリンク | URLで登録・編集し、一覧から既定ブラウザで開ける。URL変更時を含め、重複候補を保存前に確認できる |
| P-01 | 付属ページ | 1リンクに0〜複数ページを追加し、改名・並べ替えできる |
| P-02 | Markdown編集 | WYSIWYG（リッチ編集）と原文表示を切り替えてそのまま編集できる。自動保存と即時保存があり、失敗や競合時に下書きを保持する |
| P-03 | ナレッジ | 独立したMarkdownページを作成・編集・検索できる |
| T-01〜02 | タグ | リンク、付属ページ、ナレッジ、日記に個別に付与できる。種別と複数タグのANDで絞り込める |
| Q-01〜02 | 統合検索 | タイトル、URL、説明、本文、タグを横断し、日本語の部分一致に対応する |
| Q-03〜04 | パレットとプレビュー | `Ctrl+P`からキーボードで検索し、候補をプレビューして開ける |
| Q-05〜06 | 順位と同期 | タイトルの一致を優先し、保存・改名・削除・復元を検索に反映する |
| B-01〜02 | バックアップと復元 | 利用中のDBを手動で保存できる。復元時はバックアップを検証し、現DBを退避する |
| X-01 | Markdown書き出し | 保存済みのページを1ページずつUTF-8の`.md`で出力できる |
| R-01 | ごみ箱 | Webリンク、付属ページ、ナレッジを論理削除し、設定のごみ箱から復元できる |

固定ナビゲーションは「日記」「Webリンク」「ナレッジ」「タグ」。分類はタグで行う。後から検討する機能は第14章に記載する。

## 4. データモデル

| 対象 | 保存内容と関係 |
|---|---|
| Webリンク | URL、任意のタイトル・説明。付属ページを0〜複数件持つ |
| 付属ページ | 親Webリンク1件、任意のタイトル、Markdown本文、親の中での表示順 |
| ナレッジ | 任意のタイトルとMarkdown本文 |
| 日記 | JSTの暦日ごとに1ページ。日付とMarkdown本文を同じ行に保存し、表示タイトルは日付から生成する |
| タグ | 共通辞書。Webリンクと各種ページに直接付与できる |

- 付属ページ、ナレッジ、日記は`pages`で管理する。付属ページは親WebリンクIDを、日記は日記日付を持つ。ナレッジでは両方が空になる。IDはUUIDv7とし、改名やURL変更後も維持する。
- 付属ページの有効分を`position, id`順で表示する。作成と復元では末尾に追加し、並べ替えでは有効な全ページのIDを順に受け取って一括更新する。
- 有効な対象は、`deleted_at_ms`が空のWebリンクとナレッジ、日記、親リンクと自身の両方が未削除の付属ページ。親リンクの削除中は配下ページも画面・検索・タグ集計から外れる。親の復元後は、個別に削除したページがごみ箱に残る。
- 日記の記入済み判定は、Markdown原文の前後の空白を除いた結果が空でないこと。初回に空の本文を送っても行は作らない。保存済みの日記を空にした場合はカレンダーの印が消え、その日付と既存のタグは維持される。
- 更新日時は、本文、タイトル、URL、説明、直接付与するタグの変更で更新する。タグ辞書の改名、付属ページの並べ替え、削除・復元では対象の更新日時を維持する。

## 5. SQLiteの正本

日時はUTCのUnix epochミリ秒、日記日付は`YYYY-MM-DD`。Rustで実在する暦日を検証する。通常接続では外部キーを有効化し、WALと`synchronous=FULL`で保存する。詳細や一覧の読み取り、検索、書き込みは別の接続を使う。複数のSQL文を使う読み取りは同じスナップショットを参照し、書き込みは直列に処理する。新規DBは第5章と第6.3節のDDLをスキーマ3として適用する。

```sql
CREATE TABLE web_links (
    id             TEXT PRIMARY KEY NOT NULL,
    url            TEXT NOT NULL,
    normalized_url TEXT NOT NULL,
    title          TEXT NOT NULL DEFAULT '',
    description    TEXT NOT NULL DEFAULT '',
    created_at_ms  INTEGER NOT NULL,
    updated_at_ms  INTEGER NOT NULL,
    deleted_at_ms  INTEGER
);
CREATE INDEX idx_web_links_active_url
    ON web_links(normalized_url) WHERE deleted_at_ms IS NULL;
CREATE INDEX idx_web_links_active_updated
    ON web_links(updated_at_ms DESC, id) WHERE deleted_at_ms IS NULL;

CREATE TABLE pages (
    id            TEXT PRIMARY KEY NOT NULL,
    web_link_id   TEXT REFERENCES web_links(id) ON DELETE CASCADE,
    diary_date    TEXT UNIQUE
        CHECK (diary_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    title         TEXT NOT NULL DEFAULT '',
    body_markdown TEXT NOT NULL DEFAULT '',
    position      INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    revision      INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    deleted_at_ms INTEGER,
    CHECK (
        (web_link_id IS NOT NULL AND diary_date IS NULL)
        OR (web_link_id IS NULL AND diary_date IS NOT NULL
            AND title = '' AND position = 0 AND deleted_at_ms IS NULL)
        OR (web_link_id IS NULL AND diary_date IS NULL AND position = 0)
    )
);
CREATE INDEX idx_pages_web_link_position ON pages(web_link_id, position, id);
CREATE INDEX idx_pages_knowledge_active_updated
    ON pages(updated_at_ms DESC, id)
    WHERE web_link_id IS NULL AND diary_date IS NULL AND deleted_at_ms IS NULL;

CREATE TABLE tags (
    id       TEXT PRIMARY KEY NOT NULL,
    name     TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE CHECK (name_key <> '')
);
CREATE TABLE web_link_tags (
    web_link_id TEXT NOT NULL REFERENCES web_links(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (web_link_id, tag_id)
);
CREATE INDEX idx_web_link_tags_tag ON web_link_tags(tag_id, web_link_id);

CREATE TABLE page_tags (
    page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    tag_id  TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (page_id, tag_id)
);
CREATE INDEX idx_page_tags_tag ON page_tags(tag_id, page_id);
```

### 5.1 URLの保存と重複

- 前後の空白を除いた入力を表示用の`url`に保存する。スキームが明示された絶対`http://`または`https://` URLで、ホストが存在することをRustの`url`クレートで検証する。入力中の制御文字はエラーとする。
- `url::Url`の直列化結果を`normalized_url`に保存する。この結果はスキームとホストの大小文字、既定ポート、空パスの`/`などを正規化する。クエリのパラメータと順序、フラグメントは保持する。`http`と`https`は別のURLとして扱う。
- 新規登録と、比較キーが変わる更新では、有効なリンクから同じ`normalized_url`を検索する。更新対象自身は候補から除く。候補があれば「既存項目を開く」「別件として保存」を表示し、その間はフォームの入力を保持する。保存コマンドは同じ判定をトランザクション内でも行う。
- Webリンクと付属ページのタイトルは前後の空白を除いて保存する。空欄のWebリンクは正規化したURLのホスト名、空欄の付属ページは「無題」、日記は`YYYY-MM-DDの日記`と表示する。検索用タイトルにも同じ表示値を使う。
- 既定ブラウザで開くときはRust側で起動対象を再検証し、正規化したURLを渡す。URL登録時のタイトルや説明は利用者が入力する。

### 5.2 タグ

- 表示名`name`の前後の空白を除き、第6.2節の正規化結果を`name_key`にする。空の名前は入力エラー。作成時に同じキーがあれば既存タグを選べるようにし、改名時の衝突は入力エラーを返す。
- タグ絞り込みでは、指定したすべてのタグIDが同一の対象に直接付与されていることを条件にする。親リンクと付属ページの付与関係は独立する。
- タグの改名は辞書を更新する。削除はタグ行と付与関係を除去し、リンクとページを保持する。タグごとの件数は削除中のリンク・ページを除いて集計する。

## 6. 検索

### 6.1 入力・結果

- 「検索」と`Ctrl+P`は同じパレットを開く。パレットとタグ画面は同じ検索サービスを呼び出す。対象種別は「すべて」「Webリンク」「付属ページ」「ナレッジ」「日記」。タグ条件は本文検索と同時に適用できる。
- 空の入力では、指定のフィルターに合う最近更新した項目を表示する。空白を含む入力は正規化後の1つの文字列として照合する。
- 最初に30件を表示し、「さらに表示」で続きを取得する。本文は選択された項目の詳細取得時に読み込む。付属ページには親リンク名を併記する。
- 既定は一致の優先度順。「更新順」では更新日時が新しい順に表示する。

### 6.2 正規化と検索経路

検索キーには **NFKC → Unicodeの既定の完全ケースフォールド → NFKC → 前後の空白除去と連続する空白の1スペース化** を適用する。保存する本文や表示名は元の表記を使う。URLの完全一致と登録時の重複判定には第5.1節のURL比較キーを使う。

| 照合 | 経路 |
|---|---|
| タイトルの完全・前方一致 | `search_documents.title_key`のB-tree |
| URLの完全一致 | URLとして解釈できた入力を`web_links.normalized_url`と比較 |
| ホスト名の前方一致 | `search_documents.host_key`のB-tree |
| タグ名の完全・前方一致とタグIDのAND | `tags.name_key`とタグ中間テーブル |
| タイトル、親リンク名、URL、説明、本文の部分一致 | 正規化後に3 Unicodeスカラー値以上の入力をFTS5 `trigram`で照合 |

- 前方一致はBINARY照合順序でキー範囲を指定する。下限を接頭辞とし、上限は末尾から最初に繰り上げられるUnicodeスカラー値を進め、後続を切り落とした排他的な値とする。上限を作れない場合は下限だけを使う。
- 正規化後の入力が1〜2文字の場合は、完全一致と前方一致の経路を使う。UIに「1〜2文字ではタイトル・タグ・ホスト名の先頭から検索」と表示する。
- FTS5の`MATCH`には入力中の`"`を`""`に置き換えたうえで全体を`"`で囲み、単一のフレーズとしてバインドする。`AND`や`*`なども文字列の一部として検索する。
- Markdown本文は保存時に解析する。表示に出る文字列を連結し、ブロックの区切りを空白として扱う。リンクはラベル、画像は代替テキストを検索対象にする。

### 6.3 検索用の派生テーブル

有効なWebリンクまたはページにつき1行を保存する。`target_id`はその対象のID、`kind`は`web_link`、`attached_page`、`knowledge_page`、`diary`のいずれか。`host_key`はWebリンクのホスト名、`url_text_key`は表示用URL、`parent_title_key`は付属ページの親リンクの表示タイトルから作る。各対象が持たない検索項目のキーは空文字とする。

```sql
CREATE TABLE search_documents (
    id               INTEGER PRIMARY KEY,
    target_id        TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('web_link', 'attached_page', 'knowledge_page', 'diary')),
    title_key        TEXT NOT NULL,
    host_key         TEXT NOT NULL DEFAULT '',
    url_text_key     TEXT NOT NULL DEFAULT '',
    description_key  TEXT NOT NULL DEFAULT '',
    body_key         TEXT NOT NULL DEFAULT '',
    parent_title_key TEXT NOT NULL DEFAULT '',
    updated_at_ms    INTEGER NOT NULL,
    UNIQUE (kind, target_id)
);
CREATE INDEX idx_search_title ON search_documents(title_key);
CREATE INDEX idx_search_host
    ON search_documents(kind, host_key) WHERE kind = 'web_link';
CREATE INDEX idx_search_updated ON search_documents(updated_at_ms DESC, kind, target_id);
CREATE INDEX idx_search_kind_updated
    ON search_documents(kind, updated_at_ms DESC, target_id);
CREATE INDEX idx_search_match ON search_documents(
    id, kind, target_id, updated_at_ms, title_key, parent_title_key, url_text_key, description_key
);

CREATE VIRTUAL TABLE search_fts USING fts5(
    title_key, url_text_key, description_key, body_key, parent_title_key,
    content='search_documents',
    content_rowid='id',
    tokenize='trigram case_sensitive 1'
);

CREATE TRIGGER search_documents_ai AFTER INSERT ON search_documents BEGIN
    INSERT INTO search_fts(rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES (new.id, new.title_key, new.url_text_key, new.description_key, new.body_key, new.parent_title_key);
END;

CREATE TRIGGER search_documents_ad AFTER DELETE ON search_documents BEGIN
    INSERT INTO search_fts(search_fts, rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES ('delete', old.id, old.title_key, old.url_text_key, old.description_key, old.body_key, old.parent_title_key);
END;

CREATE TRIGGER search_documents_au AFTER UPDATE ON search_documents
WHEN old.title_key <> new.title_key
    OR old.url_text_key <> new.url_text_key
    OR old.description_key <> new.description_key
    OR old.body_key <> new.body_key
    OR old.parent_title_key <> new.parent_title_key
BEGIN
    INSERT INTO search_fts(search_fts, rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES ('delete', old.id, old.title_key, old.url_text_key, old.description_key, old.body_key, old.parent_title_key);
    INSERT INTO search_fts(rowid, title_key, url_text_key, description_key, body_key, parent_title_key)
    VALUES (new.id, new.title_key, new.url_text_key, new.description_key, new.body_key, new.parent_title_key);
END;
```

新規作成時に`PRAGMA application_id = 0x4F42524D`と`PRAGMA user_version = 3`を設定する。派生行はRustが正本と同一トランザクションで更新し、FTS5への反映は上記トリガーで行う。派生行の更新では内部の`id`を維持する。

- 親リンクの表示タイトルが変わった場合は、配下の有効ページの`parent_title_key`も更新する。リンクの削除・復元時は、配下のページも第4章の表示条件に従って同期する。
- タグ名とタグ付与の照合は正本を参照する。タグ付与で対象の更新日時が変わった場合は、派生行の更新日時も同期する。
- 保守時はトランザクション内で検索用テーブルとトリガーを作り直し、正本から派生行を投入する。最後にFTS5の`integrity-check`（`rank = 1`）で派生行との整合性を確認する。復元時も第10.3節に従い同じ処理を使う。

### 6.4 順位とページング

| 優先順 | 一致 |
|---:|---|
| 1 | タイトルまたはURLの完全一致 |
| 2 | タイトルまたはホスト名の前方一致 |
| 3 | 直接付与されたタグ名の完全・前方一致 |
| 4 | タイトルまたは親リンク名の部分一致 |
| 5 | URLの部分一致 |
| 6 | 説明の部分一致 |
| 7 | 本文の部分一致 |

候補をSQLite内で統合し、各対象で最上位の一致を採用する。同一対象で同じ優先順の一致があれば、表の左側の項目を`matchedField`に使う。FTS5候補は`idx_search_match`に含まれる項目を順位順に判定し、通常の本文一致では索引から得た候補IDを使う。結果間の同順位は`updated_at_ms DESC, kind, target_id`で確定する。更新順と空入力では一致優先度を省いて同じ順序を使う。種別・タグ条件とカーソルを適用し、30件と続きの判定用1件に絞って表示用のデータを取得する。

### 6.5 応答性

- 検索欄は50msのデバウンスを初期値とし、RustのDB処理はUIと別のワーカーで行う。入力やフィルターが変わった時点で、以前の条件の検索・追加取得結果を破棄する。パレットでは検索とプレビューそれぞれの実行中に新しい要求が来たら最新の要求を待機させる。起動時は表示する日記とカレンダーを読み込み、ほかの一覧やタグ集計は利用時に取得する。
- 保存・改名・タグ変更・削除・復元が成功した時点で、該当する一覧と表示中の検索条件を再取得し、ページングを先頭から行う。有効な選択はIDで維持する。検索や詳細取得に失敗した場合は再試行できるようにする。
- 性能はWindowsの配布ビルドで計測する。1万件を日常の測定用、10万件を負荷確認用とし、リンク・日記・付属ページの比率、本文長、タグ数、PCの構成を記録する。ウォーム時のP95の目標は最終入力から結果表示まで150ms、候補選択からプレビュー表示まで100ms、2回目以降の起動から一覧表示まで1秒。デバウンス・DB・IPC・描画の時間を分けて記録し、負荷に応じて調整する。

Windowsのリリースモードで、AMD Ryzen 7 9700Xを使用して検索サービスを計測した。各条件を30回実行し、本文の共通語に一致する検索のP95を比較した。本文は主に約1,000文字の日本語、タグは2種類。1万件はリンク3,000・付属ページ5,000・日記2,000、10万件はリンク30,000・付属ページ68,000・日記2,000で構成した。この値は検索サービス内の処理時間で、デバウンス・IPC・画面描画は計測範囲外。

| 件数 | 初回ページの変更前 | 初回ページの変更後 | 次ページの変更前 | 次ページの変更後 |
|---:|---:|---:|---:|---:|
| 10,000 | 22.7ms | 4.3ms | 23.1ms | 4.3ms |
| 100,000 | 311.1ms | 68.4ms | 323.0ms | 70.9ms |

## 7. UIと操作

### 7.1 レイアウトと外観

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Oboromi      ［検索 Ctrl+P］                         ［新規］      │
├─────────────┬───────────────────────┬───────────────────────────────┤
│ 日記        │ 月間カレンダー        │ 日付・タイトル               │
│ Webリンク   │ Webリンク一覧        │ URL・説明・タグ              │
│ ナレッジ    │ ナレッジ一覧          │ 付属ページ一覧／Markdown本文 │
│ タグ        │ タグ／検索結果        │ ［原文表示 / リッチ編集］    │
└─────────────┴───────────────────────┴───────────────────────────────┘
```

- 左は固定ナビ、中は一覧・カレンダー・検索候補、右は選択した内容。中と右は独立してスクロールする。付属ページの見出しには親リンクに戻る導線を置く。
- 初期幅の目安は左200px、中320px、右は残り。1000px未満では左ナビをメニューに収納し、760px未満では一覧と詳細を1画面ずつ表示する。詳細から戻ると一覧の選択とスクロール位置を復元する。
- 配色の初期値は本文背景`#FFFFFF`、左ペイン`#F6F6F4`、本文`#252725`、補助文字`#5B605C`、選択背景`#EAF2EE`、強調色`#365F55`。境界線と余白で区切り、Webリンクのタイトルと日記の日付を見出しにする。本文はSegoe UIと游ゴシックUIを優先し、コードブロックと「原文表示」モードのMarkdown原文は等幅フォントを使う。
- 選択中の行は背景と印で示し、キーボードフォーカスを枠線で表示する。一覧・フィルター・詳細の操作はTabで移動できるようにし、アイコンだけの操作には名前を付ける。

### 7.2 日記

- 月間カレンダーに記入済みの日を示す。月の移動はカレンダーの範囲を変え、日付の選択で右ペインを切り替える。前日・翌日操作で月をまたぐ場合は表示月も移動する。
- 「今日」の日付は選択時点のJSTで計算する。編集中に日付が変わっても、その編集で選択していた日付に保存する。
- 未記入の日を開いた時点でエディタが待機状態になり、そのまま書き始められる。空白のみの間はDB行を作成せず、初回保存後にタグ付与と書き出しを利用できる。

### 7.3 Webリンク・付属ページ・ナレッジ・タグ

- リンク一覧は更新順。詳細にURL、説明、直接付与したタグ、付属ページを表示する。主要操作は「ブラウザで開く」「編集」「ページを追加」。リンクのURL・タイトル・説明はフォームの「保存」で確定する。
- 付属ページは各ページの詳細で編集・改名・タグ付与を行い、親リンクの一覧で順序を変更する。
- ナレッジは独立した一覧に更新順で表示し、詳細で本文編集・改名・タグ付与を行う。
- タグ画面はタグごとの付与件数を示し、タグチップを複数選んで対象を絞り込める。ページ側に付けたタグはページの詳細に表示する。
- リンク・付属ページは確認後にごみ箱へ移す。親リンクが削除中の場合は、ごみ箱に親リンクを表示し、親の復元後に個別で削除済みの子ページを復元できるようにする。

### 7.4 パレットとプレビュー

- パレットを開くと検索入力にフォーカスする。空入力時は最近更新した項目と「今日の日記を開く」「Webリンクを追加」を表示する。パレット内で種別・タグ・並び順を変更できる。
- `↑`・`↓`で候補を移動する。幅760px以上ではパレット内のプレビューが切り替わり、候補一覧と本文をそれぞれスクロールできる。`Enter`または候補のクリックでパレットを閉じ、対象の詳細を開く。`Esc`で取り消した場合は起動前の選択とフォーカスに戻る。
- 日本語IMEによる変換中のキー入力はIMEへ渡し、候補の移動・確定やパレットの終了を発火させない。`Ctrl+P`はアプリ内で処理する。
- 右ペインはリンクのメタデータまたは対象ページのMarkdownエディタを表示し、「原文表示」と「リッチ編集」を切り替えられる。項目移動やパレット起動時は第8.2節の保存完了を待つ。

## 8. Markdownと保存

### 8.1 本文・表示・書き出し

- `pages.body_markdown`にMarkdownを保存する。改行はLFに統一し、書き出す`.md`も同じ内容をUTF-8で保存する。「原文表示」モードでは空白・記法を入力どおり保持する。リッチ編集モードでは`@milkdown/crepe`のMarkdownシリアライザを通じて保存される。
- エディタはCommonMarkを基本とし、GFMの表・タスクリスト・取り消し線、スラッシュコマンド（`/`）、選択ツールバー、CodeMirrorによるコードブロックに対応する。表・コードブロックは必要に応じて横スクロールする。
- ローカル運用・セキュリティ方針として、Milkdownの`ImageBlock`および`Latex`機能は無効化し、CSP（`img-src 'self' data:; font-src 'self' data:`）により外部URLからの画像取得を禁止する。エディタ内の`http://`・`https://`リンクのクリックはWebView内遷移を止め、Rustの`open_external_url`に渡して既定ブラウザで開く。またバックエンドの`pulldown-cmark`によるプレビューHTML生成でも、生HTMLのエスケープ、画像の代替テキスト表示、`http(s)`以外のリンク無効化を維持する。
- 一覧・検索時は編集欄を生成しない。

### 8.2 自動保存と競合

1. 最終入力から800msを初期値として自動保存する。`Ctrl+S`は即時保存。IME変換中はタイマーによる保存を待機し、確定した内容を対象とする。自動保存の完了時にエディタを再マウントせず、カーソル位置を維持する。
2. 同じページに対する保存は直列化する。保存中に追加入力があれば、その応答のrevisionを使って最新の下書きを続けて保存する。送信後に追加入力がある間は「保存済み」へ切り替えない。
3. UIは直近の読み込みまたは保存で得たrevisionを`expectedRevision`として送る。Rustは現在のrevisionと照合し、一致して本文が変わる場合に本文・更新日時・revision・検索用の派生行を同一トランザクションで更新する。同じ本文の再送には現在の保存結果を返す。
4. 未作成の日記では`expectedRevision = null`で保存する。空白のみの場合は送信・行作成を行わず、API呼び出し時も新規行なしの成功を返す。初回の作成は日付の一意制約で保護し、同じ日付に別の本文が保存済みなら競合を返す。
5. 「原文表示」から「リッチ編集」への切替、項目移動、パレット起動、書き出し、バックアップ・復元、ウィンドウを閉じる操作では、保留中の最新入力まで保存を待つ。失敗時は下書きと編集中の画面を保持し、再試行または破棄を選べる。競合時は下書きをコピーできる状態にして、利用者の操作で保存済みの内容を再読み込みする。

## 9. Tauri IPC契約

以下は論理API。コマンド名はsnake_case、引数と戻り値のフィールドはcamelCase。各コマンドは入力を検証し、失敗時に型付きのエラーを返す。

```text
get_diary(date) -> PageView | null
save_diary(date, markdown, expectedRevision) -> SaveReceipt | null
list_diary_dates(year, month) -> string[]

create_web_link(input, allowDuplicate = false) -> WebLinkDetail
update_web_link(id, patch, allowDuplicate = false) -> WebLinkDetail
find_web_link_duplicates(url, excludeId?) -> WebLinkSummary[]
list_web_links(cursor?) -> ResultPage<WebLinkSummary>
get_web_link(id) -> WebLinkDetail
get_web_link_preview(id) -> { url, description, pageCount }
open_external_url(url) -> void

create_page(webLinkId, title) -> PageView
create_knowledge_page(title, bodyMarkdown?) -> PageView
list_knowledge_pages(cursor?) -> ResultPage<KnowledgePageSummary>
get_page(id) -> PageView
get_page_preview(id) -> string | null
save_page(id, markdown, expectedRevision) -> SaveReceipt
rename_page(id, title) -> PageView
reorder_pages(webLinkId, pageIds) -> void

list_tags(query?) -> TagSummary[]
create_tag(name) -> Tag
rename_tag(id, name) -> Tag
delete_tag(id) -> void
set_web_link_tags(id, tagIds) -> void
set_page_tags(id, tagIds) -> void

search(query, kind?, tagIds?, sort?, cursor?) -> ResultPage<SearchHit>

soft_delete_web_link(id) -> void
restore_web_link(id) -> void
soft_delete_page(id) -> void
restore_page(id) -> void
list_trash() -> TrashEntry[]

create_backup(destination?) -> BackupInfo
list_backups() -> BackupInfo[]
restore_backup(source) -> RestoreInfo
export_page_markdown(id, destination) -> void
```

```ts
type SearchKind = 'web_link' | 'attached_page' | 'knowledge_page' | 'diary'
type SearchSort = 'relevance' | 'updated'

type SearchHit = {
  id: string
  kind: SearchKind
  title: string
  updatedAtMs: number
  url?: string
  parent?: { id: string; title: string }
  diaryDate?: string
  matchedField?: 'title' | 'url' | 'tag' | 'parentTitle' | 'description' | 'body'
}

type ResultPage<T> = { items: T[]; nextCursor?: string }
type SaveReceipt = { pageId: string; revision: number; updatedAtMs: number }
```

- `PageView`は原文・revision・直接タグと、付属ページの親情報または日記日付を持つ。`save_page`は付属ページとナレッジに使い、`get_page`は日記にも使える。`get_page_preview`は表示用HTMLを生成し、空の本文には`null`を返す。`WebLinkDetail`は編集用の入力値と表示タイトルを個別に持ち、直接タグと有効な付属ページ一覧を含む。
- `list_diary_dates`は指定月の記入済みの日付を`YYYY-MM-DD`で返す。新規の日記への空保存時は、`save_diary`が`null`を返す。
- `search`の`kind`の省略は全種別、`sort`の省略は`relevance`を表す。`ResultPage`は最大30件と、続きがある場合のカーソルを返す。
- 代表的なエラーは`Validation`、`NotFound`、`DuplicateUrl`、`RevisionConflict`、`StorageError`、`IncompatibleBackup`。`DuplicateUrl`には候補を含め、`RevisionConflict`は利用者に保存済みデータの再取得を促す。
- SQLの値はバインドする。検索条件は第6章に従って構築し、UIが利用するのは上記の用途別コマンドに限る。

## 10. ローカル運用と復元

### 10.1 保存場所と通信

```text
%LOCALAPPDATA%\com.oboromi.app\
  library.sqlite3
  library.sqlite3-wal     # WAL稼働中に存在する場合
  library.sqlite3-shm     # WAL稼働中に存在する場合
  backups\
```

- 起動、登録、編集、検索、バックアップ、復元、書き出しはオフラインで完結する。アプリ内で読み込むアセットは同梱する。リンク先へのアクセスは利用者が既定ブラウザで開く操作に委ねる。
- Windows上のデータディレクトリはTauriのローカルデータパスから解決し、設定画面に表示する。DB・検索インデックス・WAL・バックアップは平文で保存する。ログには操作名とエラー種別を記録し、本文・検索語・URLは含めない。
- データディレクトリを使用するアプリのインスタンスは`instance.lock`の排他ロックにより1つに制限する。
- ウィンドウを閉じる際の保存待機（`onCloseRequested`）には`core:window:allow-close`および`core:window:allow-destroy`権限を付与する。またWebView2上でMilkdownのブロック並び替え（HTML5ドラッグ＆ドロップ）を有効にするため、`tauri.conf.json`で`dragDropEnabled: false`を設定する。

### 10.2 初期化と形式の検証

- 初回起動時はTauriのアプリ別データディレクトリにスキーマ3のライブラリを作成する。既存DBを開くときは`application_id`と`user_version`で現行形式を確認する。
- 開けないDBや対応外のDBでは、ファイルを保持し、エラーと保存場所を表示する。復元と検索テーブルの再構築の間は通常のデータ操作を止める。

### 10.3 手動バックアップと復元

1. **バックアップ**は設定から実行する。既定の`backups`フォルダまたは利用者が選んだ保存先に、SQLite Online Backup APIでスナップショットを作成する。出力先で一時ファイルに作成し、ジャーナルを`DELETE`モードにして閉じた後、単一の`.sqlite3`ファイルとして確定する。現DBと同じパスや既存ファイルへの上書きはエラーにする。
2. **復元候補**は設定に表示されるバックアップ一覧、またはファイル選択から指定する。候補を読み取り専用で開いて作業用DBに取り込み、現行の`application_id`と`user_version`、`PRAGMA integrity_check`、`PRAGMA foreign_key_check`を確認する。作業用DBで検索テーブルを再構築し、検証に成功してから現DBを変更する。
3. **反映**の直前に現DBを別のバックアップに退避する。通常接続を閉じ、専用の書き込み接続に検証済みの作業用DBからSQLite Online Backup APIで反映する。同アプリで作成した互換ページサイズのDBを扱う。完了前に失敗した場合はSQLiteのロールバックで現DBを保持する。現DBを退避できない場合は元ファイルを保持してエラーを表示する。
4. **再読み込み**では、DB接続と表示中の一覧・検索状態を初期化する。成功後に復元元と退避先を表示する。反映後の再読み込みでエラーが出た場合は通常操作を停止し、退避先を表示する。

現DBを開けない場合の復旧は、アプリ終了中に`library.sqlite3`と存在する`-wal`・`-shm`をまとめて別フォルダに退避し、再起動で作成されたライブラリにバックアップを復元する手順とする。

## 11. 実装の順序

| 工程 | 成果物と確認 |
|---|---|
| 1. 日記の縦切り | Tauri起動、SQLite初期化、日付選択、原文編集、保存、閲覧、再起動後の確認 |
| 2. コレクション | WebリンクとURL重複、付属ページ、個別タグ、削除・復元 |
| 3. 読み返し | 統合検索、カレンダーの印と移動、パレット、右ペイン、狭い幅での切替 |
| 4. ローカル運用 | バックアップ・復元・Markdown書き出し、配布ビルドでのオフライン動作と性能計測 |

画面と編集状態は`src/`、Tauriコマンド、ドメイン処理、DB・マイグレーションは`src-tauri/`に置く。検索とインデックス更新の責務はRust側に集める。

## 12. 受け入れ条件

| ID | 操作 | 合格基準 |
|---|---|---|
| AT-01 | 未記入の日に移動し、空白だけで閉じる | DBに日記が増えず、記入済みの印も付かない |
| AT-02 | JSTの午前0時をまたいで編集中の日記を保存する | 選択していた日付に保存され、「今日」は新しい日付を開く |
| AT-03 | 同じ日に日記を保存して再起動し、後から本文を空にする | 同日1件を維持し、空にした後はカレンダーの印が消える |
| AT-04 | リンクに3ページ追加して並べ替え、再起動する | 3ページと指定した順番が残る |
| AT-05 | 親にタグA、子にタグBを付けてAとBでAND検索する | 親・子は一致せず、両方を直接付与した対象だけが一致する |
| AT-06 | 全角英数字のタグを作り、半角・大小違いで再作成する | 同じタグが提示され、改名・削除は各付与先に反映される |
| AT-07 | `https://EXAMPLE.com:443`と`https://example.com/`を登録する | 候補が表示され、明示操作で別件として保存できる |
| AT-08 | URLの編集で別の有効リンクと同じURLにする | 自分自身を除いた候補を表示し、クエリやフラグメントが異なるURLは区別する |
| AT-09 | 日本語3文字以上、全角英数字、引用符やアスタリスクを含む語で検索する | 正規化後に一致した項目が出る。記号を含む入力が検索構文のエラーにならない |
| AT-10 | 1〜2文字でタイトル・タグ・ホスト名を検索する | 前方一致が動き、短い検索語で使われる項目を画面に表示する |
| AT-11 | タイトル完全一致と本文だけの一致があり、31件以上で絞り込む | タイトル完全一致が先頭側に出る。絞り込み後のページングで重複や欠落がない |
| AT-12 | 本文、親のタイトル、タグ名を変えて同じ検索を繰り返す | 保存後に新しい語で検索でき、古い語の一致が消える |
| AT-13 | 子1件を削除してから親を削除・復元する | 親の削除中は全ページが検索・集計から外れ、親の復元後も先に削除した子はごみ箱に残る |
| AT-14 | `Ctrl+P`からIMEで入力して候補を移動し、確定または取消する | 変換確定で候補を開かず、`Enter`で開ける。`Esc`で元の選択とフォーカスに戻る |
| AT-15 | 検索入力と候補変更を連続して行う | 最後の入力・選択に対応する検索結果とプレビューが表示される |
| AT-16 | 表、タスクリスト、引用、末尾の空白を含むMarkdownを保存・再読込・書き出しする | 対応構文が整形され、原文保存（API／原文表示モード）および書き出しの内容がLFへの統一を除いて一致する |
| AT-17 | 保存完了前に追加入力し、直後に別の項目へ移る | 最後の入力まで保存され、保存失敗時は下書きと再試行手段が残る |
| AT-18 | 古いrevisionで異なる本文を保存する | 保存済みの本文は維持され、下書きを取り出して再読み込みできる |
| AT-19 | 利用中にバックアップを作成し、変更後に復元する | 退避先を残して元の本文・リンク・タグ・検索結果に戻る |
| AT-20 | 不適合なバックアップを選ぶか、復元の反映完了前に失敗させる | 現DBと現在のデータが保たれ、エラーを表示する |
| AT-21 | 正本から検索テーブルを再構築する | 再構築の前後で有効な結果が一致し、削除済みの対象は結果に現れない |
| AT-22 | 生HTML、外部画像、HTTP(S)以外のURLを含むMarkdownを表示する | 生HTMLはテキストとして表示され、画像の自動取得や危険なURLの起動が起きない |
| AT-23 | ウィンドウを縮め、オフラインで一覧と詳細を往復する | 一覧の選択を維持し、登録から再検索まで操作できる |

性能計測には本文・タイトル・短い日本語・タグ2件のAND・大量一致のケースを含める。`EXPLAIN QUERY PLAN`でタイトル、ホスト、タグ、URLの索引を確認する。配布ビルドでFTS5 `trigram`による日本語部分一致とウィンドウ操作を確認する。

## 13. 変更履歴

### 0.7

- 検索の候補統合・順位付け・ページングをSQLiteで行い、索引で本文へのアクセスを減らした。
- ナレッジを仕様に反映し、検索パレット内にスクロール可能なプレビューを設けた。リッチ編集では保存時やモード切替時にMarkdownへ変換する。
- Tauriのアプリ別データディレクトリに新規ライブラリを作成する。復元は現行スキーマのバックアップに対応する。

### 0.6

- 編集方式を「HTML `textarea` 原文編集＋読み取り用プレビュー切替」から、`@milkdown/crepe` によるWYSIWYGインプレース編集（＋「原文表示」切替）へ変更。
- 外部画像の自動取得を行わない運用に合わせ、`ImageBlock` と `Latex` を無効化し、CSPで外部画像取得を禁止した上でリンククリックを `open_external_url` へ委譲する仕様を明記。
- 配布ビルド対象をNSIS（`.exe`インストーラ）のみとし、終了時保存待機のウィンドウ権限（`core:window:allow-close` / `allow-destroy`）および `instance.lock` による単一インスタンス制御を実装に合わせて明記。

## 14. MVP後に再評価する機能

内部リンク・バックリンク、ブラウザ拡張による取り込み、手動コレクション・保存済み検索、テンプレート、Capacitiesからのインポートと一括エクスポート。用途が増えたときに、添付ファイル、短い本文検索、DB暗号化、同期の必要性を検討する。

## 15. 参考資料

- [CommonMark仕様](https://spec.commonmark.org/)
- [GitHub Flavored Markdown仕様](https://github.github.com/gfm/)
- [pulldown-cmark](https://docs.rs/pulldown-cmark/)
- [SQLite FTS5](https://www.sqlite.org/fts5.html)
- [SQLite WAL](https://www.sqlite.org/wal.html)
- [SQLite Online Backup API](https://www.sqlite.org/backup.html)
- [Tauri v2](https://v2.tauri.app/)
