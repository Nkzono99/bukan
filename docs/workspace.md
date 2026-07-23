# Bukan workspace format

Bukan のアプリケーション本体と研究データは別々に管理します。ワークスペースは
`bukan init <path>` またはデスクトップアプリから初期化します。

```text
my-research/
├── bukan.toml             # ワークスペース設定
├── taxonomy.toml          # Paperpileフォルダ・ラベルの分類体系
├── AGENTS.md              # 文献調査時のルール
├── data/                  # Paperpile BibTeXなどの読み取り用索引
├── notes/                 # 文献ノート
├── queries/               # 再現可能な検索条件
├── candidates/            # Paperpileへ未登録の候補
├── reports/               # 検索レポートとエビデンス表
├── imports/               # Paperpileへ戻すファイル
├── .bukan/                # ViewerとCodexの一時コンテキスト（Git対象外）
└── cache/                 # 再生成可能な索引・抽出テキスト（Git対象外）
```

## Paperpileとの境界

- `bukan.toml` の `paperpile.path = "auto"` はマウント済みドライブを探索します。
- 絶対パスを指定すれば、ワークスペース単位で異なる同期先を使用できます。
- Paperpile の同期フォルダは常に読み取り専用です。
- PDFを移動・改名せず、タグ提案・ノート・分析結果だけをワークスペースへ保存します。

## Codexターミナル

Bukanデスクトップアプリは、ワークスペースをカレントディレクトリにしてCodex CLIを
PTY上で起動します。表示されるのはCodexの生のTUIであり、Bukan独自のチャットUIへ
変換しません。

起動時は `workspace-write` サンドボックスと `on-request` 承認を明示します。
Paperpile同期フォルダを追加の書き込み可能ディレクトリには設定しません。
読み取り元は`BUKAN_PAPERPILE_ROOT`、読み取り専用境界は
`BUKAN_PAPERPILE_READ_ONLY=true`としてCodexプロセスへ渡します。

Viewerから文献をCodexコンテキストへ設定すると、
`.bukan/current-context.md` に現在の文献ID、タイトル、コレクション、読み取り専用PDF
への参照が保存されます。このファイルはセッション用で、Gitには含めません。

Codex起動時にはBukan自身がstdio MCPサーバーとしてセッション限定で登録されます。
`search_library`、`list_collections`、`get_paper`、`get_current_paper`は、
Viewerと同じ索引を読み取り専用で参照します。書誌ファイル名とサイズに基づく
安定IDを返すため、Paperpile側でコレクションが変わっても同じPDFを追跡できます。
索引作成時にPDF本文は開かず、Google Driveのオンデマンドファイルを実体化しません。

Codexは`present_paper_list`を呼び、検索・比較・選定した文献を構造化リストとして
Viewerへ表示できます。リストは`%LOCALAPPDATA%\Bukan\bridge\`の一時状態であり、
PaperpileおよびGoogle Drive Workspaceには自動で書き込みません。`clear_paper_list`
またはGUIの「消去」で削除できます。利用者が保存ボタンを押した場合だけ、
Markdownレポートを`reports/codex-lists/`、JSON候補を
`candidates/codex-lists/`へ新規ファイルとして保存します。

## 整理モデル

Paperpile のフォルダは階層化でき、ラベルはフラットです。そのため Bukan は、
階層が必要な分野・トピック・対象・ミッション・プロジェクトを `Bukan/` 以下の
フォルダとして、横断的な方法・文献種別・状態・キーワードを接頭辞付きラベルとして
扱います。

```text
Bukan/
├── 分野/
├── トピック/
├── 対象/
├── ミッション/
└── プロジェクト/

bukan:method:PIC
bukan:type:review
bukan:status:要確認
bukan:keyword:lunar-dust
```

同じ文献は複数フォルダおよび複数ラベルへ所属できます。生成された整理案は、
Paperpile の公式APIが利用可能になるまで適用計画として保存します。

## 整理計画の生成

`taxonomy.toml` の `folder_rules` と `label_rules` は、タイトル、ファイル名、既存の
Paperpileコレクション名に照合する用語を定義します。

```powershell
bukan organize suggest D:\Research\my-workspace
```

結果は既定で `reports/bukan-organization-plan.json` に保存されます。候補がない文献は
`bukan:status:未整理`、候補がある文献も適用前は `bukan:status:要確認` になります。
ルールベースの分類は本文の意味を保証しないため、自動確定には使用しません。
