# CLI・MCPで研究を進める

Bukanを導入し、Codexから文献検索、全文読解、根拠の点検、wikiの更新を進めます。新規の研究フォルダはユーザーのデータ領域へ自動で作成します。成果物はVS CodeのMarkdownプレビュー、原PDFはPDFビューアーで確認します。

研究フォルダには`bukan.toml`と`data/research.sqlite`があります。以前のBukan GUIで使っていたものも、そのまま指定できます。Bukan本体、Codexプラグイン、研究データは別々に管理します。

## pipでツールを入れて初期化し、プラグインを追加する

Python 3.11以上の64-bit環境とCodexを用意し、Google DriveでPaperpileの同期フォルダを表示できるようにします。BukanはWindowsとLinuxのx86_64に対応しています。LinuxではディストリビューションのPopplerを先に導入し、`pdfinfo`、`pdftotext`、`pdftoppm`をPATHから使えるようにしてください。

まず、OSに対応する配布wheelをpipで導入し、初期化コマンドを実行します。Windowsの例です。Linuxではwheelのファイル名と、必要に応じて`python`を`python3`に置き換えます。システムのPythonにパッケージを追加できない場合は、仮想環境を作ってそのPythonを使ってください。

```powershell
python -m pip install /path/to/bukan-0.3.0-py3-none-win_amd64.whl
python -m bukan install
```

PyPIにはまだ公開していないため、wheelのパスを指定します。wheelにはコンパイル済みのRust CLI、研究エンジン、SKILLを含みます。Rustの導入は不要です。`pip install`だけでは研究データやCodex設定を作成せず、続く`python -m bukan install`で本体をAppData等へ配置し、`bukan setup`による研究環境の準備と、個人マーケットプレイスへの登録を行います。Windowsでは固定版のuv・Popplerも準備します。Linuxではpipで入るuvと、事前導入したPopplerを使います。

次に、**CodexにBukanプラグインをインストール**します。個人マーケットプレイスのBukanを選ぶか、表示された`codex plugin add bukan@<マーケットプレイス名>`を実行してください。新しい会話を開き、Bukanの接続先と文献検索を確認します。ホストが追加したプラグインを読み込まない場合は再起動してください。

初期化ではプラグインを有効化せず、他のプラグイン設定やCodexのモデル設定も変更しません。MCPはAppData等へ配置した実行ファイルの絶対パスで起動します。pipの仮想環境やScriptsディレクトリをCodexのPATHへ追加する必要はありません。初回の依存取得にはネットワーク接続が必要です。途中で失敗した場合は原因を解消し、`python -m bukan install`を再実行します。

Paperpileはマウント済みのドライブから自動検出します。見つからない場合も研究の保存先は残ります。Driveの接続を確認し、必要ならワークスペースの`bukan.toml`で`paperpile.path`へ同期先の絶対パスを指定します。Paperpileのファイルは読み取り専用です。

## データは本体と分けてAppDataへ保存する

Windowsの既定の配置は次のとおりです。

| 内容 | 保存先 |
| --- | --- |
| Bukan本体・プラグイン | `%LOCALAPPDATA%/bukan/installations/<version>-<bundlehash>/bukan` |
| uv・Poppler | `%LOCALAPPDATA%/bukan/dependencies/` |
| 新規の研究ワークスペース | `%LOCALAPPDATA%/bukan/workspaces/default/` |
| Bukan設定 | `%APPDATA%/bukan/settings.toml` |
| 研究用Python環境 | `%LOCALAPPDATA%/bukan/research-runtime/` |
| 原論文 | 既存のGoogle Drive／Paperpile同期先 |

`workspaces/default/`は永続データです。SQLite、wiki、文献ノート、図表、作業記録を含むため、キャッシュ削除の対象にしません。AppData配下でも自動でバックアップされるわけではなく、研究フォルダ全体をバックアップ対象にしてください。既に接続先を設定していれば、新しいフォルダへ移さず元の研究を使います。

実際の保存先は`python -m bukan paths --json`で確認できます。`dataDir`、`configDir`、`cacheDir`、`managedWorkspace`、`defaultWorkspace`を表示します。このガイドの`bukan`はpipが登録するコマンドです。PATHに見つからないときは`python -m bukan`に置き換えてください。初期化時に表示される実行ファイルの絶対パスも利用できますが、`install`と`update`はPython側のコマンドです。

Linuxのデータ領域は`$XDG_DATA_HOME/bukan`または`~/.local/share/bukan`、設定は`$XDG_CONFIG_HOME/bukan`または`~/.config/bukan`、キャッシュは`$XDG_CACHE_HOME/bukan`または`~/.cache/bukan`です。絶対パスの`BUKAN_DATA_DIR`、`BUKAN_CONFIG_DIR`、`BUKAN_CACHE_DIR`で各領域を変更できます。`BUKAN_DATA_DIR`を変えても、既存の研究フォルダは移動しません。

個人マーケットプレイスから参照できるよう、本体のデータ領域はホームディレクトリ内に置きます。`BUKAN_DATA_DIR`でホーム外を指定する構成は、下記の手動導入を使ってください。研究ワークスペース自体は別ドライブにも置けます。実際に使うデータ・設定・キャッシュの保存先をMCP設定にも記録するため、Codexの起動環境が異なっても同じ場所を使います。`BUKAN_WORKSPACE`を明示した場合は、その接続先も引き継ぎます。

## 更新しても研究データは元の場所に残る

新しいwheelをpipで導入してから、配置済み本体とプラグインの参照先を更新します。次の`<new-wheel-path>`を取得したwheelのパスに置き換えてください。

```powershell
python -m pip install --upgrade "<new-wheel-path>"
python -m bukan update
```

`update`は現在のPython環境に入ったBukanを反映するコマンドです。最新版の検索やPyPIからの取得は行いません。`install`の再実行でも同じ更新を行えます。完了後は表示された手順でCodexのプラグインを更新し、新しい会話を開いてください。本体は版とハッシュで分けて保存するため、既存の研究フォルダやDBを上書きしません。

`pip uninstall bukan`やpip用の仮想環境の削除では、AppData等へ配置済みの本体・依存ツール・研究データや、Codexのプラグイン登録は削除されません。準備済みの研究実行環境が残っていれば、MCPも引き続き使えます。Linuxで実行環境のキャッシュを作り直すときは、wheelをpipで再導入して`python -m bukan install`を実行し、必要なuvを利用できる状態にします。

## Python未導入のWindowsや手動構成で準備する

Python未導入のWindowsでは、配布zipを展開して`bukan/install.cmd`を実行します。ダブルクリックでも起動でき、Python、uv、Popplerの事前導入、管理者権限、システムPATHの編集は不要です。完了後、表示された手順でCodexへプラグインをインストールします。更新時も新しい配布zipの`install.cmd`を実行できます。

依存ツールとプラグイン登録を自分で管理する場合は、従来の手動インストーラーも使えます。Python 3.11以上、uv、Popplerを用意して実行します。

```sh
python3 /path/to/bukan/install_local.py --bundle /path/to/bukan
```

導入先の既定値は`~/plugins/bukan`で、既存の導入先は上書きしません。出力された実行ファイルで`bukan setup`を実行し、ホストの手順でプラグインを登録・インストールします。この手動経路のインストーラーは依存ツールの導入や個人マーケットプレイスの登録を行いません。

`setup`は研究実行環境を準備し、研究DBがなければ作成します。接続先が未設定なら管理領域のワークスペースを作成します。保存先を選んで新しい研究を作る場合は次のようにします。

```powershell
bukan init C:/Research/my-workspace
bukan setup C:/Research/my-workspace --default
bukan doctor C:/Research/my-workspace
```

更新と開発用パッケージの詳細は[プラグインの説明](../plugins/bukan/README.md)を参照してください。

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

コマンドでワークスペースを省略した場合は、`BUKAN_WORKSPACE`、Bukanの既定設定の順で解決します。`BUKAN_WORKSPACE`には絶対パスを指定します。`setup`だけは両方が未設定の場合に管理ワークスペースを初期化します。指定済みのフォルダが不正・不在なら停止し、別の研究を自動作成して切り替えることはありません。複数の研究へ接続するときは、パスを明示したMCP設定を使います。

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
