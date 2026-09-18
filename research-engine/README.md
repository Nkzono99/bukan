# Bukan Research prototype

文献の主張・条件・根拠・発展関係を蓄積し、研究上の問いを改訂するための試作です。
Rust内部型に依存しないPythonパッケージで、CLI、stdio MCP、汎用JSON窓口から同じSQLiteストアを使います。
文献の読解と関係の判断はAIクライアントが担当します。このパッケージはLLM呼び出しや自動サーベイを実行しません。

## 起動

通常利用ではPython 3.11以上の64-bit環境にBukanの配布wheelを`pip install`し、`python -m bukan install`で初期化してから、CodexへBukanプラグインをインストールします。PyPIには未公開のため、wheelのパスを指定します。Windowsでは依存ツールも準備し、新規の研究は`%LOCALAPPDATA%/bukan/workspaces/default`、実行環境は`%LOCALAPPDATA%/bukan/research-runtime`に置きます。既存の研究フォルダが設定済みならその場所を保ちます。LinuxではPopplerを先に用意します。Python未導入のWindowsでは配布zipの`install.cmd`も使えます。[利用ガイド](../docs/usage.md)を参照してください。

CLIからは`bukan research [workspace] -- <engine args>`または`bukan research-mcp [workspace]`を使います。`python -m bukan`からも同じコマンドを呼べます。`bukan paths --json`で保存場所を確認できます。引数なしの`bukan setup`は、WindowsとLinuxで未設定時の管理ワークスペースを作れます。

研究エンジンを単体で開発する場合はPython 3.11以上とuvを使います。以下はリポジトリのルートから実行する例です。
ストアは所有ホストのローカルディスク上に置きます。

```powershell
uv sync --project research-engine --locked
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite init
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite serve
```

`serve`はstdio MCPサーバーです。標準入力でMCPリクエストを受け取るため、通常はMCPクライアントから起動します。
既存のBukan MCPと研究用MCPを別々に登録できます。クライアントの設定例では、パスを実際の保存先へ置き換えてください。

```json
{
  "mcpServers": {
    "bukan-research": {
      "command": "uv",
      "args": [
        "run", "--directory", "C:/path/to/bukan/research-engine", "--locked",
        "bukan-research", "--store", "C:/Research/shared/research.sqlite", "serve"
      ],
      "env": {"PYTHONUTF8": "1"}
    }
  }
}
```

上記JSONは研究エンジンを直接起動する開発用の例です。配布物から使う場合は、`bukan mcp-config <workspace>`が文献管理MCPと研究MCPの設定を表示します。クライアントのグローバル設定は変更しません。
既存Bukanとのstdio連携を試すため、公式MCP Python SDKの1.x保守系列を指定し、実際の依存版を`uv.lock`で固定しています。

## CLI・MCPで使うJSON窓口

`bukan-research --store PATH request`は標準入力からJSONを1件読み、結果を標準出力へ返して終了します。
`bukan research <workspace> -- request`からも呼べます。実装は`workspace_api.py`にあり、MCPと同じStoreを使用します。旧`desktop`コマンドとモジュールは互換用に残しています。研究DBの初期化は`init`で別に行い、
`summary`を呼んだだけではDBを作成・更新しません。通信形式は`version: 1`です。

| `operation` | 入力 | 結果 |
|---|---|---|
| `summary` | 任意の`query`、`kind`、`offset`、`limit`（既定50、最大200） | 件数、現在のレコードの一覧、`hasMore`と`nextOffset`。検索は保存内容全体の部分一致 |
| `get` | `id`、任意の`revision` | 省略しない本文のMarkdown、固定した改訂への参照、文献ノートのプレビュー出力先と編集用本文 |
| `save-note` | `id`、`expectedRevision`、`markdown` | 新しい文献ノート改訂の`get`と同じ結果。競合時は保存しない |
| `wiki-status` / `migrate` | 追加引数なし | 保存形式を確認／SQLiteバックアップを作成して形式1から2へ移行 |
| `wiki-home` | 任意の`query`、`pageType`、`category`、`offset`、`limit` | wikiの現在版、作業と未統合候補、ページ送り |
| `wiki-refresh` | 追加引数なし | 未統合の研究記録と根拠が更新された節を照合し、作業を保存 |
| `create-wiki` | `title`、任意の`pageType`、`parentId` | 固定IDを持つページを作成 |
| `save-wiki` | `id`、`expectedRevision`、`title`、`summary`、`sections`、`changeReason` | 節の本文を改訂。各節は`id`、`title`、`markdown`。既存の根拠を保持 |
| `create-wiki-task` | `pageId`、`purpose`、任意の`pageRevision`、`sectionId`、`conditions`、`searchThrough` | 対象を固定した調査作業を保存。閲覧した版を対象にするときは`pageRevision`を指定 |
| `update-wiki-task` | `id`、`expectedRevision`、`state`、担当・途中経過・理由・結果、`resultPageRevision` | 同じ調査作業を更新。完了時には比較したページ改訂と、改訂または変更不要の理由を指定 |
| `decide-wiki-candidate` | `id`、`expectedRevision`、`state`、`pageId`、`reason`、`resultPageRevision` | 関連付け・採否を保存。統合時は確認した結果ページの改訂を指定 |

文献ノートの`markdown`は表示用で、画像を同梱した出力先の`path`を基準にします。
編集には`editText`を使います。こちらは元の`paper_note.markdown`で、画像参照も著者入力時の
`../../../note-assets/...`を保ちます。編集中のプレビューは`editPath`を基準に画像やリンクを解決します。
`editPath`はその改訂の従来形式`rN.md`のパスで、実ファイルを作成するものではありません。
保存済み表示用の`path`とはディレクトリの深さも異なります。表示用の`assets/...`を編集本文へ戻してはいけません。
保存では本文だけを変更し、Source、読解範囲、根拠参照は保持します。変更した本文はDB検索にも反映され、旧改訂は残ります。

画像欠落や既存出力の手修正を検出した場合、新しいDB改訂は保存しません。
既存ノートの表示で出力できない場合は`previewWarning`を返し、`path`を返しません。
その場合もDBの本文を取得・修正できますが、手修正した出力を自動取り込み・上書きすることはありません。
出力先への書き込み途中でOSエラーが起きた場合は、未公開改訂の一部ファイルが残る場合があります。
再試行では既存内容を照合し、異なる内容を黙って置き換えません。

## 保存するデータ

新規ストアの形式は2です。形式1の既存ストアは読み取りできますが、書き込み前に`migrate`を実行します。移行はDBの隣の`backups/`にSQLiteバックアップを作り、既存のレコード本文・ID・改訂を保持します。通常の参照や`init`だけでは移行しません。JSON通信の`version: 1`はDB形式とは別です。

| 型 | 内容 |
|---|---|
| Paper | 書誌と外部文献管理システムへの参照 |
| Source | 読み取った本文・要旨の固定テキスト、原資料のURI・SHA-256・版・ページ等 |
| Evidence | Source内の文字範囲と、完全一致する短い抜粋 |
| Claim | 著者の結果・手法・仮定・限界と、成立条件・根拠 |
| Relation | 主張間の拡張・手法利用・支持・問題提起・条件差。比較前提と根拠を持つ |
| Question | 研究上の問い、理由、別の説明、検証方法、判断状態 |
| Topic | 共通データへの参照をまとめたテーマ |
| PaperNote | 文献ごとのMarkdownノート、PDF版への参照、全ページの読解状況、知見の根拠 |
| ReviewTask | PDF版ごとの読解依頼、担当、進捗、更新元のノート改訂、完了ノートへの参照 |
| WikiPage | 横断的な解説。種類、別名、分類、親項目、節ごとの本文・固定根拠・点検状態 |
| WikiTask | ページ・節の調査依頼、担当、途中経過、改訂または変更不要の理由 |
| WikiCandidate | まだ統合されていない研究記録、関連しそうな項目、採否・統合先の固定改訂 |

各参照は`{"id": "...", "revision": 1}`の形で特定の改訂を固定します。
新しい判断を保存しても過去の判断と根拠は残ります。SourceとEvidenceは変更できず、別の版や抜粋には新IDを使います。
研究の要約・関係の判断には、原則として全ページを読みます。要旨・数ページだけの読解は選別段階の暫定メモとして扱います。
本文に加え、方法、結果、限界、参考文献、付録まで確認し、図表・数式も目視します。未取得の補足資料は未確認と明記します。
`source_type`で要旨と本文を区別し、PDFを取得・抽出したことと実際に読んだことを区別してください。
ストアは読解の実施や科学的な妥当性を自動認定しません。

`extends`には「何を維持したか」「何を変えたか」が必要です。
関係は既定で`analyst_inference`です。著者が関係を明記している根拠を確認した場合だけ`author_explicit`を使います。
研究上の仮説はQuestionへ保存し、著者の結果を表すClaimに混ぜません。

### wikiの根拠、更新、画像

`WikiPage.sections`は固定した節ID、Markdown、`basis`を持ちます。根拠は記録のID・改訂に加え、wiki内の対象節と`supports`・`challenges`・`context`を指定できます。項目間の案内は現在版のIDへ接続し、根拠への依存と分けます。本文や根拠を変更した節では以前の点検結果を引き継ぎません。

根拠の新版があれば依存先をたどって再検討作業を保存します。まだ根拠に使われていない論文・Source・Evidence・主張・関係・文献ノートは候補に残します。関連ページの提案は文字列照合であり、科学的な関連性や採否はクライアントが判断します。過去版を意図的に使う研究史では、その理由を作業記録へ残してください。

wiki本文の画像は、仮想の著者入力位置`data/wiki/<page hash>.md`から`../wiki-assets/`を参照します。画像をこの永続保存先へ置いてからページを登録してください。保存時の画像バイトもSQLiteへ固定し、`export-wiki`は`data/wiki/<page hash>/rN/index.md`と配下の`assets/`を書き出します。原画像の後の変更で、旧改訂の図が置き換わることはありません。出力フォルダごとコピーすれば図を持ち運べます。書き出し済みファイルへの手修正は自動で上書きしません。

```powershell
uv run --project research-engine bukan-research --store C:/Research/topic/data/research.sqlite wiki-status
uv run --project research-engine bukan-research --store C:/Research/topic/data/research.sqlite migrate
uv run --project research-engine bukan-research --store C:/Research/topic/data/research.sqlite wiki-home
uv run --project research-engine bukan-research --store C:/Research/topic/data/research.sqlite export-wiki wiki-detachment
```

ページの完全な構造を変更する場合は`put_records`を使います。`save-wiki`は人間向けの本文編集用で、既存節の根拠を保持します。数値・単位による条件検索や比較表の自動生成は未実装です。設計と運用は[研究wiki](../docs/research-wiki.md)を参照してください。

## MCP操作

| ツール | 操作 |
|---|---|
| `put_records` | 複数レコードを参照検証付きでまとめて登録。途中の不整合は全体を取り消す |
| `get_record` | 現在または指定改訂のレコードを読む |
| `search_records` | 型と部分文字列で検索。ページ送りに対応 |
| `trace_record` | テーマ・問いから主張、抜粋、原資料へ遡る |
| `get_history` | 判断の改訂履歴を読む |
| `store_info` | 保存件数と形式の版を確認する |
| `research_wiki_request` | JSON窓口と同じ`operation`でwikiの検索・表示・改訂・作業を扱う。上の操作表を参照 |
| `export_wiki_page` | wikiの固定改訂を、保存時の画像とともに書き出す |
| `get_paper_note` | 現在または指定改訂のMarkdownノートと、ページごとの読解状況を取得する |
| `export_paper_note` | DBの隣の`paper-notes/`へ改訂ごとのMarkdownと画像のコピーを出力する。既存ファイルの編集は上書きしない |
| `plan_paper_reviews` | 読了済みの同じPDF版を再利用し、残る文献の担当・進捗を保存する。サブエージェントの起動はクライアントが行う |
| `find_public_versions` | 文献ID・DOI・書誌から公開版候補を一括検索。ローカルPDFの所蔵とは独立 |
| `check_public_urls` | URLの応答・転送先・確認日時を記録。無料の全文やライセンスの確認とは区別 |
| `save_public_access` | 外部検索由来の候補も、現改訂を指定して履歴付き保存 |
| `get_public_access`, `search_public_access` | 保存した公開版候補を、過去の改訂も含めJSON・Markdownで取得 |

公開先の調査は全文読解を前提にしません。[公開版リンクの利用手順](../docs/public-access.md)を参照してください。
結果は研究DB内の追加テーブル`public_access_reports`へ保存し、既存の形式2の科学的レコードとは分けます。
従来のレコード形式は変えず、既存クライアントも引き続き読み書きできます。通常のJSON `export`は、保存済みなら
`public_access`キーにこの調査履歴も含めます。形式1のDBへの書き込みには、従来どおり明示的な移行が必要です。

新規登録は`expected_revision: 0`、更新は取得した現改訂を指定します。同じ内容の再登録では改訂を増やしません。
古い改訂からの更新は競合として拒否します。これは手修正を含む同時更新の保護であり、利用者認証の仕組みではありません。
MCP経由の著者表記は`ai-client`に固定します。全レコードに返す`semantic_review: not_verified_by_store`は、科学的妥当性を自動認定しないことを示します。

登録用JSONは`Write`の配列です。完全な型は[src/bukan_research/models.py](src/bukan_research/models.py)にあります。

```json
[
  {
    "entity": {"kind": "paper", "id": "example-paper", "title": "Example paper"},
    "expected_revision": 0
  }
]
```

```powershell
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite put C:/Research/input.json
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite search 輸送 --kind question
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite trace my-topic
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite export
uv run --project research-engine pytest research-engine/tests -q
```

`export`は全改訂と現在の参照をJSONとして標準出力へ出します。PDF実体は含みません。
`trace_record`は参照方向へ遡り、別レコードからの被参照を探索するものではありません。
打ち切りは`truncated`で示し、古い参照には`latest_revision`を併記します。

## 文献ノートを検索と再読の入口にする

`paper_note`の`markdown`へ、研究目的・手法・結果・前提・限界と、そこから考えた知見・未解決の問いを記します。
著者の結果と読み手の推論を区別し、本文にはPDFページ・節・図表番号を付け、`basis`にはClaimやEvidence等の固定参照を持たせます。
`source`は対象PDF版のSourceを指します。ノートをテーマから参照すれば、同じ読解を別の研究テーマでも利用できます。

総ページ数を`page_count`に、読解状況を`coverage`に保存します。各項目は`page`、`text`（`reviewed` / `unread` / `failed`）、
`visuals`（`reviewed` / `not_present` / `unread` / `failed`）、補足の`note`です。`not_present`は図表・数式がないと確認したページだけに使います。
対象版の本文Source（`source_type: body`）を参照し、全ページの本文と視覚要素を確認した場合だけ、
`get_paper_note`が`full_text_reviewed`を返します。要旨・書誌Sourceを参照するノートは、全ページ読了と申告しても`partial`です。
未登録のページは未読です。この状態は読み手の申告を検査したもので、実際の読解を証明するものではありません。
PDFが変わった場合は別のSourceへ参照を変え、以前の読解状況をそのまま引き継がないでください。

更新は`put_records`で行い、以前の判断と人による追記を確認してから新しい改訂を保存します。
検索は`search_records(query="初期電荷", kind="paper_note")`、再読は`get_paper_note(record_id=...)`を使います。
ノートで候補を絞った後は、重要な主張を引用元のPDFで確認します。

Markdownの出力は改訂ごとのスナップショットです。DBを更新すると新しい改訂を出力でき、以前のファイルは残ります。
出力したMarkdownを直接編集してもDBの検索には自動反映されません。JSON窓口の`save-note`、または
`paper_note.markdown`への取り込みと`expected_revision`を指定した保存を使ってください。
ファイル監視や双方向同期は行いません。

重要な図表は、出典PDFから抽出・描画した画像を`![説明](画像パス)`でノートに埋め込みます。
図表番号、PDFページ、原本のSHA-256、何を読み取れるかを添え、軸・単位・凡例・必要なキャプションを残します。
細部は切り出した画像を目視して確認します。取得できなかった図や判読できない箇所を、読解済みとして扱わないでください。

最初の抽出画像は文献別の`cache/`へ置き、登録前に調整役が研究DBの隣の`note-assets/`へコピーします。
DB内の画像参照は従来の出力位置を基準とする`../../../note-assets/<文献>/<画像>.png`です。
画像名に内容のハッシュを含めるなどして、過去のノートが参照する画像を上書きしないでください。

書き出し形式2では、`paper-notes/<ストア名のハッシュ>/<ノートIDのハッシュ>/rN/index.md`と、
その配下の`assets/`へ画像をコピーします。出力Markdownは`assets/…`を参照し、MCPは
`export_format_version: 2`と実際の`path`を返します。DBの形式・本文は変えず、旧`rN.md`も保持します。
新しいノートは返された`path`から開いてください。同じ改訂の旧ファイルに手修正がある場合は、出力を止めて通知します。
追記は新しいDB改訂へ取り込んでください。Windowsのパス長を抑えるため、出力のストア・ノート・画像ハッシュには先頭16・32・32桁を使います。

この配置なら、研究ワークスペースの外から文書だけを開いたVS Codeの標準プレビューでも画像を読み込めます。
ローカル画像は`note-assets/`配下に限定し、欠落や範囲外参照を検出したら書き出しを失敗させます。
Markdownのインライン画像構文を表の外で使ってください。参照形式・表内のローカル画像と、HTMLによる画像埋め込みは出力時に拒否します。
外部HTTP(S)のMarkdown画像は取得・同梱しないため、オフライン表示にはローカルのプレビュー画像を使います。
文書を持ち運ぶ際は`rN/`を画像ごとコピーします。図のコピーと元の`note-assets/`はDBのJSONに含まれないため、一緒にバックアップしてください。

独立した数式は開始・終了の`$$`をそれぞれ単独行にし、前後に空行を入れます。`\(...\)`、`\[...\]`、
バッククォートを数式の区切りに使いません。原文の意味と疑義を保ち、固定SourceやEvidenceの引用には手を加えません。
最後に保存済みファイルを実際の対象プレビューで開き、画像と変更した数式を確認します。パスの存在確認だけでは完了にしません。

```powershell
uv run --project research-engine bukan-research --store C:/Research/shared/research.sqlite export-note note-example
```

文献管理MCPでの全文取得は[PDF読解の使い方](../docs/full-paper-reading.md)を参照してください。
多数の文献を処理する手順は[サブエージェントによる並列レビュー](../docs/parallel-review.md)を参照してください。
担当モデルは[レビュー工程ごとの使い分け](../docs/review-model-routing.md)を参照してください。全文読解と結果の検証は分けて記録します。

## Bukanとの境界と試作の範囲

課題ごとの研究史と未取得文献の台帳は[研究課題の変遷](../docs/research-history.md)の手順で作成します。
探索を反復し、引用・非引用の関係を主張単位で解析する標準工程は[網羅的な先行研究解析](../docs/exhaustive-review.md)を参照してください。
年表と取得状況は外部ワークスペースのMarkdown・JSON・BibTeXに保存し、研究MCPには
根拠付きのノートと研究史の入口となるTopicを登録します。これらのファイルの自動取り込みはありません。

文献検出・索引はRustコアが担当します。`bukan scan --json`やBukan MCPから取得した書誌を、
`adapters.paper_from_bukan`で変換します。アダプターはPDFを開かず、Bukanへの書き戻しもしません。
研究データの保存先として、Paperpile配下とBukanソース配下を拒否します。

現時点の同定は外部IDとの対応までです。DOI照合による異版統合や、外部IDが変わった場合の自動追跡は未実装です。
タイトル・DOIを確認して重複文献を選別し、別の版を同じ根拠へ自動的に置き換えないでください。
検索は部分一致で、意味検索や関係の自動判定ではありません。
登録した根拠テキストと原資料ハッシュの対応は取り込み側が確認し、ストアはテキスト上の抜粋一致と参照整合性を検証します。
汎用的なPDF取得、図表解析、バックグラウンド実行、予算管理は研究エンジン自身では行いません。
