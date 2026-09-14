# Bukan Research prototype

文献の主張・条件・根拠・発展関係を蓄積し、研究上の問いを改訂するための試作です。
BukanのGUIやRust内部型に依存しないPythonパッケージで、CLIとstdio MCPから同じSQLiteストアを使います。
文献の読解と関係の判断はAIクライアントが担当します。このパッケージはLLM呼び出しや自動サーベイを実行しません。

## 起動

Python 3.11以上とuvを使います。以下はリポジトリのルートから実行する例です。
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

アプリ内Codexの設定への自動追加は行いません。上記JSONは一般的なMCPクライアント向けの例です。
既存Bukanとのstdio連携を試すため、公式MCP Python SDKの1.x保守系列を指定し、実際の依存版を`uv.lock`で固定しています。

## 保存するデータ

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

各参照は`{"id": "...", "revision": 1}`の形で特定の改訂を固定します。
新しい判断を保存しても過去の判断と根拠は残ります。SourceとEvidenceは変更できず、別の版や抜粋には新IDを使います。
研究の要約・関係の判断には、原則として全ページを読みます。要旨・数ページだけの読解は選別段階の暫定メモとして扱います。
本文に加え、方法、結果、限界、参考文献、付録まで確認し、図表・数式も目視します。未取得の補足資料は未確認と明記します。
`source_type`で要旨と本文を区別し、PDFを取得・抽出したことと実際に読んだことを区別してください。
ストアは読解の実施や科学的な妥当性を自動認定しません。

`extends`には「何を維持したか」「何を変えたか」が必要です。
関係は既定で`analyst_inference`です。著者が関係を明記している根拠を確認した場合だけ`author_explicit`を使います。
研究上の仮説はQuestionへ保存し、著者の結果を表すClaimに混ぜません。

## MCP操作

| ツール | 操作 |
|---|---|
| `put_records` | 複数レコードを参照検証付きでまとめて登録。途中の不整合は全体を取り消す |
| `get_record` | 現在または指定改訂のレコードを読む |
| `search_records` | 型と部分文字列で検索。ページ送りに対応 |
| `trace_record` | テーマ・問いから主張、抜粋、原資料へ遡る |
| `get_history` | 判断の改訂履歴を読む |
| `store_info` | 保存件数と形式の版を確認する |
| `get_paper_note` | 現在または指定改訂のMarkdownノートと、ページごとの読解状況を取得する |
| `export_paper_note` | DBの隣の`paper-notes/`へ改訂ごとのMarkdownと画像のコピーを出力する。既存ファイルの編集は上書きしない |
| `plan_paper_reviews` | 読了済みの同じPDF版を再利用し、残る文献の担当・進捗を保存する。サブエージェントの起動はクライアントが行う |

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
出力したMarkdownを直接編集してもDBの検索には自動反映されません。追記は`paper_note.markdown`へ取り込み、`expected_revision`を指定して保存してください。
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

文献検出・索引は既存Rustコアが担当します。`bukan-cli scan --json`やBukan MCPから取得した書誌を、
`adapters.paper_from_bukan`で変換します。アダプターはPDFを開かず、Bukanへの書き戻しもしません。
研究データの保存先として、Paperpile配下とBukanソース配下を拒否します。

現時点の同定は外部IDとの対応までです。DOI照合による異版統合や、外部IDが変わった場合の自動追跡は未実装です。
タイトル・DOIを確認して重複文献を選別し、別の版を同じ根拠へ自動的に置き換えないでください。
検索は部分一致で、意味検索や関係の自動判定ではありません。
登録した根拠テキストと原資料ハッシュの対応は取り込み側が確認し、ストアはテキスト上の抜粋一致と参照整合性を検証します。
汎用的なPDF取得、図表解析、バックグラウンド実行、予算管理、GUIへの新しい表示は今回の試作に含めていません。
