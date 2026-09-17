# CLI・MCPで研究を進める

Bukanで使う研究フォルダを一度指定し、Codexから文献検索、全文読解、根拠の点検、wikiの更新を進めます。成果物はVS CodeのMarkdownプレビュー、原PDFはPDFビューアーで確認します。

研究フォルダには`bukan.toml`と`data/research.sqlite`があります。以前のBukan GUIで使っていたものも、そのまま指定できます。Bukan本体、Codexプラグイン、研究データは別々に管理します。

## 実行ファイルと研究環境を準備する

配布されたアーカイブには`bukan/`フォルダがあり、実行ファイル、研究エンジン、SKILLを含みます。Python 3.11以上を使い、展開先の`install_local.py`で導入できます。導入先の既定値は`~/plugins/bukan`です。

```powershell
python C:/Downloads/bukan/install_local.py --bundle C:/Downloads/bukan
```

既存の導入先は上書きしません。更新時の扱いと開発用のパッケージ生成は[プラグインの説明](../plugins/bukan/README.md)を参照してください。

CLIを`bukan`という名前で呼ぶ場合は`~/plugins/bukan/bin`をPATHへ追加するか、実行ファイルの絶対パスを使います。インストーラーは導入先プラグインのMCP設定を絶対パスにするため、プラグイン起動にはPATH追加は不要です。研究環境の準備には`uv`が必要です。PDF取得にはPopplerの`pdfinfo`、`pdftotext`、`pdftoppm`が必要で、準備方法は[PDF読解](full-paper-reading.md)を参照してください。

新しい研究を作る場合は、最初にワークスペースを初期化します。

```powershell
bukan init C:/Research/my-workspace
bukan setup C:/Research/my-workspace --default
bukan doctor C:/Research/my-workspace
```

`setup`は研究エンジンの実行環境を準備し、研究DBがなければ作成します。初回の依存パッケージ取得にはネットワーク接続が必要です。Pythonの実行環境はBukanのキャッシュに置き、研究フォルダとは分けます。`uv`自体は自動導入しません。

`--default`はBukanの既定ワークスペースを保存します。Windowsでは`%APPDATA%/bukan/settings.toml`、実行環境のキャッシュは`%LOCALAPPDATA%/bukan/research-runtime`です。Unixではそれぞれ`$XDG_CONFIG_HOME/bukan`または`~/.config/bukan`、`$XDG_CACHE_HOME/bukan`または`~/.cache/bukan`を使います。必要なら絶対パスの`BUKAN_CONFIG_DIR`と`BUKAN_CACHE_DIR`で変更できます。

## 既存の研究フォルダを使い続ける

既存の`bukan.toml`がある場合は、`init`を繰り返さず、そのフォルダを`setup`へ渡します。

```powershell
bukan setup C:/Research/existing-workspace --default
bukan research C:/Research/existing-workspace -- wiki-status
```

既存の`data/research.sqlite`、`notes/`、`reports/`、`data/wiki/`、図表、未取得台帳を保持します。新規ワークスペースへのコピーは不要です。ダスト輸送の研究やPaperpileのMarkdown資料が別フォルダにある場合も、元の場所を保存先として維持してください。

旧アプリが管理していた研究では、Windowsの`%APPDATA%/jp.bukan.literature/workspaces/<ライブラリ別ID>/`など、旧アプリのデータ領域に`bukan.toml`がある場合があります。その既存フォルダを指定します。GUIを使わなくなっても、この領域やキャッシュをまとめて削除・移動しないでください。既存の研究データが含まれます。

形式1の研究DBは参照できますが、更新には明示的な移行が必要です。`setup`は既存DBを自動移行しません。

```powershell
bukan research C:/Research/existing-workspace -- migrate
```

移行はDBの隣の`backups/`へSQLiteバックアップを作り、ID・本文・改訂を保持して形式2に更新します。DBと図表を含む研究フォルダ全体をバックアップ対象にします。稼働中のSQLiteは単純コピーせず、書き込みを止めるかSQLiteのバックアップ機能を使います。

既存の`AGENTS.md`やSKILLの独自設定も自動上書きしません。新しい指示へ更新する場合は、[研究ハーネス](research-harness.md)に従って差分を統合します。

## MCPをCodexへ接続する

次のコマンドが、この研究に固定した2つのMCP設定を表示します。

```powershell
bukan mcp-config C:/Research/my-workspace --format toml
```

JSON形式を使うクライアントでは`--format json`を指定します。文献管理MCPは`bukan mcp`、研究MCPは`bukan research-mcp`で起動します。いずれも標準入力で待機するサーバーなので、日常利用ではMCPクライアントから起動します。

`mcp-config`は設定の表示だけを行い、Codexのグローバル設定を編集しません。プラグインから利用する場合も、設定したBukanの実行ファイルとワークスペースを使います。プラグインの登録方法は[配布定義](../plugins/bukan/README.md)を参照してください。

コマンドでワークスペースを省略した場合は、`BUKAN_WORKSPACE`、Bukanの既定設定の順で解決します。`BUKAN_WORKSPACE`には絶対パスを指定します。現在いるディレクトリからの推測は行いません。複数の研究へ接続するときは、パスを明示したMCP設定を使います。

## 問いを伝え、保存済みの研究から続ける

Codexで研究ワークスペースを開き、対象の問いとwiki項目を伝えます。開始時に`workspace_context`で接続先の研究とPaperpileを確認します。BukanのSKILLは、作業に必要な読解・統合・取得・表示確認の手順を選びます。

> ダスト輸送について、粒子が表面から離脱する条件を比較して。既存wiki、文献ノート、作業記録を確認し、引用先・被引用文献・引用関係のない研究を追って。未読論文は文献ごとに全文を読み、重要な本文記述と登録する全Claimを別担当で検証して。条件の違う結果を分け、同じwikiの解釈と未取得台帳を更新して。

対象が特定のページや節なら、題名とID・改訂、条件を伝えます。本文を毎回コピーしたり、旧GUIの一時コンテキストファイルを作ったりする必要はありません。MCPで現行記録、固定根拠、未処理作業を取得して進めます。以前の`queries/requests/`に保存した依頼は再開資料として保持できます。

Codexが担当の実行とモデル選択を行い、Bukanは記録・参照・競合を検査します。エンジン自体はLLMや常駐スケジューラーを起動しません。中断後は、実際の担当状況と保存済みの作業を確認して再開します。

調査範囲と終了条件は[網羅的な先行研究解析](exhaustive-review.md)、担当の配布と保存は[並列レビュー](parallel-review.md)にまとめています。文献ノートの完成に続いて、影響するwikiを改訂するか、比較した根拠と変更不要の理由を記録します。

## 結果を出力して読み、訂正を同じDBへ戻す

CLIでも保存済みの記録を参照できます。次は指定したワークスペースで実行する例です。

```powershell
bukan research C:/Research/my-workspace -- wiki-home
bukan research C:/Research/my-workspace -- search 輸送 --kind paper_note
bukan research C:/Research/my-workspace -- export-wiki wiki-example
bukan research C:/Research/my-workspace -- export-note note-example
```

`wiki-example`と`note-example`は実際の記録IDへ置き換えます。出力が返す`path`のMarkdownをVS Codeで開き、標準プレビューを表示します。ノートやwikiの改訂出力には文書配下の`assets/`があり、重要な図表を画像として埋め込みます。数式は独立した`$$`ブロックで記述します。

原PDFをPDFビューアーで開き、式、図表、条件を照合します。出力したすべての図と新規・変更した数式を対象プレビューで確認し、持ち出す場合は画像ごと別の場所へコピーして再確認します。ファイルが存在するだけでは表示確認を完了にしません。対象アプリケーションを開けない場合は、その確認を未検証として残します。

出力Markdownは改訂スナップショットです。直接編集した内容はDB検索へ自動反映されません。訂正はMCPの`put_records`、またはJSON窓口の`save-note`・`save-wiki`で現行改訂を指定して保存します。CLIのJSON窓口も同じエンジンを使います。

```powershell
'{"version":1,"operation":"wiki-home","limit":20}' | bukan research C:/Research/my-workspace -- request
```

編集には返された`editText`を使い、表示用に書き換えた画像パスをそのままDBへ戻さないでください。本文の訂正が科学的な意味を変える場合は、対応するClaim・根拠・比較・wikiにも点検を戻します。操作の引数は[研究エンジン](../research-engine/README.md)で確認できます。
