# Bukan

Bukanは、Paperpileの文献を読み取り専用で検索・読解し、研究ノート、主張と根拠、研究wikiを外部ワークスペースに蓄積するCLIとMCPです。Codexから調査を進め、成果物はVS CodeのMarkdownプレビュー、原資料はPDFビューアーで確認します。

利用手順は[CLI・MCPで研究を進める](docs/usage.md)、保存形式は[ワークスペース仕様](docs/workspace.md)を参照してください。既存の研究フォルダ、DB、ノート、wikiをそのまま使います。

## 研究を始める

配布物の`bukan`を利用できる状態にして、研究ワークスペースを指定します。

```powershell
bukan init C:/Research/my-workspace
bukan setup C:/Research/my-workspace --default
bukan doctor C:/Research/my-workspace
bukan mcp-config C:/Research/my-workspace --format toml
```

`init`は新しいワークスペースを作り、`setup`は研究実行環境と未作成の研究DBを準備します。既存ワークスペースでは`init`を省略し、同じ研究DBを使います。形式1のDBは自動移行しません。`mcp-config`は文献管理・研究の2つのMCP設定を表示し、Codexのグローバル設定は変更しません。

| コマンド | 用途 |
| --- | --- |
| `bukan setup <workspace> [--default]` | 初期化済みワークスペースの研究環境を準備し、必要ならBukanの既定値を保存する |
| `bukan research [workspace] -- <engine args>` | 同じ研究DBで検索・改訂・wiki出力などを実行する |
| `bukan mcp [workspace]` | 読み取り専用の文献索引・PDF取得用MCPを起動する |
| `bukan research-mcp [workspace]` | 研究記録を保存・検索するMCPを起動する |
| `bukan mcp-config [workspace] [--format json\|toml]` | 2つのMCPを登録する設定を表示する |
| `bukan init`, `detect`, `doctor`, `scan`, `organize` | ワークスペース作成、Paperpile検出、診断、索引化、分類案の生成 |

省略したワークスペースは`BUKAN_WORKSPACE`、Bukanに保存した既定値の順で決めます。カレントディレクトリからは推測しません。複数の研究を扱うときは、コマンドにパスを明示してください。

## できること

- Paperpileの同期済み文献を、題名・著者・年・コレクションから検索する
- PDFの版とSHA-256を固定し、ページ本文とページ画像を取得する
- 全文読解ノート、条件付きのClaim、原文と一致するEvidence、論文間のRelationを保存する
- wikiの同じページを改訂し、根拠の更新と未統合の記録を追跡する
- 文献別の作業と現在の改訂を記録し、中断や同時編集から再開する
- 数式と埋め込み図を含むMarkdownを、画像付きの改訂スナップショットとして出力する
- 既存の継続レビュー、候補、整理計画を研究ワークスペースに保持する

全ページの読解、科学的な独立点検、表示確認は別々に記録します。全文の自動取得や読了フラグだけでは、科学的な妥当性を保証しません。[研究ハーネス](docs/research-harness.md)が指示と検査の役割を、[並列レビュー](docs/parallel-review.md)が担当の配布と保存を定めます。

Paperpileの同期領域には書き込みません。ノート、図表、検索条件、候補、レポートは研究ワークスペースに保存します。文献整理は適用案とインポート候補の生成までで、Paperpileへの登録・変更は別の操作です。

## 開発する

RustのCargo workspaceとPythonの[研究エンジン](research-engine/README.md)で構成します。Node.js、Tauri、WebView2は不要です。

```powershell
cargo run -p bukan -- --help
cargo build -p bukan --release --locked
cargo test --workspace --locked
uv run --project research-engine --locked pytest research-engine/tests -q
```

```text
crates/bukan/          CLI・文献索引・PDF・MCP・ワークスペース
research-engine/      研究記録・wiki・stdio MCP
templates/workspace/  研究ワークスペースと作業別SKILLの正本
plugins/bukan/        Codexプラグインの配布定義
docs/                 利用手順・保存形式・研究工程
```

Codexプラグインの配布・導入手順は[プラグインの説明](plugins/bukan/README.md)を参照してください。研究エンジンとSKILLを含む配布物を使い、利用者の研究データをアプリケーションのリポジトリやプラグインへコピーしません。

Windowsの配布物をローカルで作る例です。Linuxでは実行ファイルを`target/release/bukan`、対象を`x86_64-unknown-linux-gnu`に替えます。

```powershell
python scripts/package_release.py --binary target/release/bukan.exe --target x86_64-pc-windows-msvc --output-dir dist
```

対応・配布対象はWindowsとLinuxです。GitHub Actionsは両環境用のzipを生成します。Releaseは下書きとして作り、公開は別途行います。macOSは現時点では動作保証の対象外です。

## 研究工程を確認する

- [全文読解](docs/full-paper-reading.md)：正確なPDF版、ページ本文、図表・数式の確認
- [網羅的な先行研究解析](docs/exhaustive-review.md)：引用・非引用の探索と終了条件
- [研究wiki](docs/research-wiki.md)：現在の解釈、固定根拠、改訂と未統合候補
- [情報の階層](docs/research-knowledge-layers.md)：原資料、読解記録、wiki、対話の役割
- [モデルの使い分け](docs/review-model-routing.md)：担当の指定と独立した科学的点検

Paperpile全体の全文Markdown化は、このCLI移行とは別の作業です。既存の抽出資料と研究成果を保持し、変換の実行や完成をこの移行に含めません。
