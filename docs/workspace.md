# Bukan workspace format

Bukan本体と研究データは別々に管理します。引数なしの`bukan setup`は、接続先が未設定なら管理領域に新しい研究ワークスペースを作り、研究実行環境とDBを準備します。Windowsでの既定の場所は`%LOCALAPPDATA%/bukan/workspaces/default`、Linuxでは`$XDG_DATA_HOME/bukan/workspaces/default`または`~/.local/share/bukan/workspaces/default`です。

既存ワークスペースは同じ場所で使い続けます。保存場所を選ぶ場合は`bukan init <path>`、`bukan setup <path> --default`の順で実行します。`bukan paths --json`でデータ・設定・キャッシュと既定ワークスペースの実際の場所を確認できます。保存先の上書き方法は[利用ガイド](usage.md)を参照してください。

```text
my-research/
├── bukan.toml             # ワークスペース設定
├── taxonomy.toml          # Paperpileフォルダ・ラベルの分類体系
├── AGENTS.md              # 文献調査時のルール
├── .agents/skills/bukan-paper-review/
│   ├── SKILL.md           # 作業別の入口
│   └── references/        # 読解・統合・取得・表示・ツールの詳細
├── data/
│   ├── paperpile.bib      # Paperpile BibTeXなどの読み取り用索引
│   ├── collections.json   # Workspace独自のコレクションと文献割り当て
│   ├── research.sqlite    # 研究エンジンの構造化記録
│   ├── wiki/              # 研究wikiの改訂出力と固定した参照資料
│   ├── wiki-assets/       # wikiへ取り込む画像
│   ├── paper-notes/       # 文献ノートの改訂スナップショットと図表
│   └── note-assets/       # 永続保存した原図・表のプレビュー
├── notes/                 # 独立した文献ノート
├── queries/               # 再現可能な検索条件・保存済み依頼
├── candidates/            # Paperpileへ未登録の候補・未取得台帳
├── reports/               # レポート、比較資料、既存の継続レビュー
├── imports/               # 承認済みのインポート用ファイル
├── .bukan/                # 旧コンテキスト等（Git対象外）
└── cache/                 # 再生成可能な索引・抽出テキスト（Git対象外）
```

研究用SKILLは新規ワークスペースへ同梱します。既存のカスタムファイルを上書きせず、初期化済みワークスペースを使うだけでは指示文を更新しません。配布元と更新時の比較・保全は[研究ハーネス](research-harness.md)を参照してください。

## Paperpileは読み取り専用にする

`bukan.toml`の`paperpile.path = "auto"`はマウント済みドライブを探索します。絶対パスを指定すれば、ワークスペースごとに同期先を選べます。PDFの移動・改名・削除は行わず、分類案、ノート、分析結果をワークスペースへ保存します。自動出力された`data/paperpile.bib`も編集しません。

`data/collections.json`がない状態で最初に`collection create`を実行すると、Paperpileのコレクション階層と割り当てをWorkspaceへ複製してから独自分類を追加します。初回はPaperpileへの接続が必要です。`collection list`は読み取り専用で、初期化前は空の一覧を返します。以後の再索引では上書きせず、保存済みの独自分類はPaperpileが未接続でも編集できます。独自分類の作成・割り当て変更はこのファイルだけを更新します。

```powershell
bukan collection list C:/Research/my-workspace
bukan collection create 比較対象 C:/Research/my-workspace
bukan collection add 比較対象 p2-example C:/Research/my-workspace
```

最後の例の`p2-example`は実際の文献IDへ置き換えます。独自分類はPaperpileへ書き戻しません。

## CLIと2つのMCPが同じ研究を参照する

使い方は[利用ガイド](usage.md)を参照してください。`bukan research <path> -- <engine args>`と`bukan research-mcp <path>`は、指定したワークスペースの`data/research.sqlite`を使います。文献管理MCPは`bukan mcp <path>`で起動します。

ワークスペースの解決順はコマンドの明示パス、`BUKAN_WORKSPACE`、Bukanの既定設定です。カレントディレクトリは使いません。`setup`はどれも未設定の場合だけ管理領域を初期化し、設定済みの接続先が壊れていればエラーにします。既定設定を保存してもCodexのグローバル設定は変更しません。`bukan mcp-config <path>`で接続設定だけを表示できます。

研究エンジンの実行環境はBukanのキャッシュへ保存し、研究フォルダには置きません。配布物が研究エンジンを含むため、インストール後に開発リポジトリは不要です。Paperpileが未接続でも保存済みの研究を参照できますが、原PDFの取得には同期先が必要です。

研究対象はMCPで題名やIDから探し、必要な現行記録・固定根拠・作業を取得します。旧`queries/requests/`や`.bukan/current-*.md`を再開資料として保持できますが、CLIは旧GUIの選択状態を更新しません。一時コンテキストのコピーは研究の前提にしません。

## DBの改訂と画像を一緒に保つ

新しい研究DBは形式2です。形式1も参照できますが、書き込み前に`bukan research <path> -- migrate`を明示して移行します。SQLiteバックアップを作り、元のID・本文・改訂を保持します。`setup`は既存DBを自動移行しません。

ノートとwikiはDBの現在版を改訂し、閲覧時に固定版のMarkdownを書き出します。重要な図表は画像として埋め込み、出力文書の配下へ必要な画像を同梱します。出力が返す`path`をVS Codeで開いてプレビューを確認します。原資料はPDFビューアーで照合します。

書き出したMarkdownへの手修正はDBに自動反映されません。元の改訂と差分を確認し、研究MCPまたはCLIの`request`を通して新しい改訂へ取り込みます。既存の出力、図表、人間の追記を黙って上書きしません。

## 既存の継続レビューを保持する

従来のテーマ別レビューも引き続き次の場所に保存できます。

```text
reports/reviews/<review-id>/
├── article.md             # 現在の本文
├── review.json            # テーマ、改訂番号、引用、図の出典
├── figures/               # Workspaceで抽出後、出典付きで複製した画像
└── history/               # 更新前のMarkdownスナップショット
```

`list_reviews`、`get_review`、`update_review`でIDを指定して扱います。重要な主張には文献IDとPDFページ・節を記録します。`attach_review_figure`はWorkspaceへ抽出した画像だけを複製し、元論文IDとページ番号を保持します。Paperpileへの書き戻しは行いません。

旧`reports/codex-lists/`と`candidates/codex-lists/`も成果物として保持します。これらのファイルや通常のMarkdownが、研究DBへ自動取り込みされるわけではありません。

## 分類案を確認して保存する

分野・トピック・対象・ミッション・プロジェクトは階層を持つ`Bukan/`フォルダ、方法・文献種別・状態・キーワードは平坦な`bukan:`ラベルとして提案します。同じ文献を複数の分類へ割り当てられます。

```text
Bukan/分野/
Bukan/トピック/
Bukan/対象/
Bukan/ミッション/
Bukan/プロジェクト/

bukan:method:PIC
bukan:type:review
bukan:status:要確認
bukan:keyword:lunar-dust
```

`taxonomy.toml`の`folder_rules`と`label_rules`は、タイトル・ファイル名・既存コレクション名に照合する用語を定義します。

```powershell
bukan organize suggest C:/Research/my-workspace
```

結果は既定で`reports/bukan-organization-plan.json`へ保存します。候補がない文献は`bukan:status:未整理`、候補がある文献も適用前は`bukan:status:要確認`になります。ルールの一致は本文の意味を保証しないため、案を確認してから別途承認した方法で適用します。
