# Bukan

Bukanは、Paperpileの文献を読み取り専用で検索・読解し、研究ノート、主張と根拠、研究wikiを外部ワークスペースに蓄積するCLIとMCPです。Codexから調査を進め、成果物はVS CodeのMarkdownプレビュー、原資料はPDFビューアーで確認します。

利用手順は[CLI・MCPで研究を進める](docs/usage.md)、保存形式は[ワークスペース仕様](docs/workspace.md)を参照してください。既存の研究フォルダ、DB、ノート、wikiをそのまま使います。

## pipでツールを入れ、Codexへプラグインを追加する

Python 3.11以上の64-bit環境、Codex、Google Driveから見えるPaperpile同期フォルダを用意します。使うOSに対応した配布wheel（`.whl`）を指定して、ツールの導入と初期化を行います。Windowsの例です。

```powershell
python -m pip install /path/to/bukan-0.2.2-py3-none-win_amd64.whl
python -m bukan install
```

続いて、**Codexの個人マーケットプレイスからBukanプラグインをインストール**します。完了時に表示される`codex plugin add ...`コマンドでも追加できます。新しい会話で「Bukanの接続先を確認して、保存済み文献から調査を始めて」と依頼してください。

`pip install`はパッケージの導入、`python -m bukan install`はAppData等への配置、依存ツールの準備、研究フォルダの初期化とプラグインの登録を担当します。研究フォルダが未設定ならWindowsでは`%LOCALAPPDATA%/bukan/workspaces/default`を作成し、設定済みならその場所を引き継ぎます。wheelにはRustの実行ファイルを同梱するため、利用者のコンパイルは不要です。

PyPIにはまだ公開していません。現時点では上のようにwheelのパスを指定してください。Linuxでは対応するwheelとディストリビューションのPopplerが必要です。Pythonを入れずに始めるWindows向けには、配布zipの`bukan/install.cmd`も残しています。詳細は[利用ガイド](docs/usage.md)を参照してください。

## CLIから使う

pipで導入した環境では`bukan`、または`python -m bukan`から実行します。`install`と`update`はPython側のコマンドで、その他は同梱のRust CLIへ渡します。Rust側のコマンドは、インストーラーが表示する実行ファイルの絶対パスでも呼び出せます。

| コマンド | 用途 |
| --- | --- |
| `python -m bukan install` | 本体・依存ツール・研究環境を準備し、Codexの個人マーケットプレイスへ登録する |
| `python -m bukan update` | pipで更新済みの版を本体とプラグインの参照先へ反映する。最新版のダウンロードは行わない |
| `bukan setup [workspace] [--default]` | 研究環境を準備する。接続先が未設定なら、管理領域にワークスペースを作成する |
| `bukan paths --json` | 本体とは別に管理するデータ・設定・キャッシュと、ワークスペースの場所を表示する |
| `bukan research [workspace] -- <engine args>` | 同じ研究DBで検索・改訂・wiki出力などを実行する |
| `bukan mcp [workspace]` | 読み取り専用の文献索引・PDF取得用MCPを起動する |
| `bukan research-mcp [workspace]` | 研究記録を保存・検索するMCPを起動する |
| `bukan mcp-config [workspace] [--format json\|toml]` | 2つのMCPを登録する設定を表示する |
| `bukan init`, `detect`, `doctor`, `scan`, `organize` | ワークスペース作成、Paperpile検出、診断、索引化、分類案の生成 |

省略したワークスペースは`BUKAN_WORKSPACE`、Bukanに保存した既定値の順で決めます。`setup`だけは、どちらも未設定なら管理領域を初期化します。設定済みの接続先に問題がある場合は停止し、別の研究へ切り替えません。複数の研究を扱うときは、コマンドにパスを明示してください。

## できること

- Paperpileの同期済み文献を、題名・著者・年・コレクションから検索する
- 所蔵済みの論文も含め、[公開版・プレプリントの候補](docs/public-access.md)を検索し、リンクの確認結果を履歴付きで保存する
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
python -m pip install .
```

ソースからの`pip install .`はRustのビルド環境が必要です。ユーザーのAppDataや研究データの初期化は、開発用ビルド中にも行いません。

```text
crates/bukan/          CLI・文献索引・PDF・MCP・ワークスペース
research-engine/      研究記録・wiki・stdio MCP
templates/workspace/  研究ワークスペースと作業別SKILLの正本
plugins/bukan/        Codexプラグインの配布定義
docs/                 利用手順・保存形式・研究工程
```

Codexプラグインの配布・導入手順は[プラグインの説明](plugins/bukan/README.md)を参照してください。研究エンジンとSKILLを含む配布物を使い、利用者の研究データをアプリケーションのリポジトリやプラグインへコピーしません。

wheelはリポジトリのルートで次のように作成します。Rust CLIもビルドし、`dist/`へ出力します。AppDataや研究ワークスペースの初期化は行いません。

```powershell
python -m pip install build
python -m build --wheel
```

Windowsの配布zipをローカルで作る例です。先に`cargo build -p bukan --release --locked`を実行します。Linuxでは実行ファイルを`target/release/bukan`、対象を`x86_64-unknown-linux-gnu`に替えます。

```powershell
python scripts/package_release.py --binary target/release/bukan.exe --target x86_64-pc-windows-msvc --output-dir dist
```

対応・配布対象はWindowsとLinuxのx86_64です。GitHub Actionsは両環境用のzipとwheelを生成します。Linuxの配布wheelはmuslを使ってビルドし、ローカルで通常ビルドしたGNU版wheelはビルド環境に依存します。Releaseは下書きとして作り、公開は別途行います。macOSは現時点では動作保証の対象外です。

## 研究工程を確認する

- [全文読解](docs/full-paper-reading.md)：正確なPDF版、ページ本文、図表・数式の確認
- [網羅的な先行研究解析](docs/exhaustive-review.md)：引用・非引用の探索と終了条件
- [研究wiki](docs/research-wiki.md)：現在の解釈、固定根拠、改訂と未統合候補
- [情報の階層](docs/research-knowledge-layers.md)：原資料、読解記録、wiki、対話の役割
- [モデルの使い分け](docs/review-model-routing.md)：担当の指定と独立した科学的点検

Paperpile全体の全文Markdown化は、このCLI移行とは別の作業です。既存の抽出資料と研究成果を保持し、変換の実行や完成をこの移行に含めません。
