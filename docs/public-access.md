# 公開版・プレプリントのリンクを調べる

研究MCPから、論文の出版版・プレプリント・著者最終稿の候補を検索し、確認結果を保存できます。PaperpileにPDFを持っている論文にも使えます。公開用の参考文献表や文献マップに載せるリンクを整えるための機能です。

Codexなどのホストには、たとえば「この文献一覧の公開版リンクを調べ、未確認事項を残して保存し、Markdownの比較表を出して」と依頼します。全文を読むことやPDFを取得することは、この調査の前提ではありません。

## 候補を探してから、必要なリンクを確認する

`find_public_versions`へ最大20件の`papers`を渡します。Bukanの文献ID、研究DBのPaper ID、DOI、または題名を使えます。著者と年も指定できます。

```json
{
  "papers": [
    {"paper_id": "<search_libraryで得たID>"},
    {"title": "Dust charging and transport on airless planetary bodies", "authors": "Wang", "year": 2016}
  ]
}
```

CrossrefとOpenAlexを検索し、`reports`を返します。片方のサービスが停止していても、もう片方の候補と失敗理由を返します。`providers: ["crossref"]`のように検索先を選べます。タイトル検索は各サービスの上位3件を返し、同じ論文と断定しません。上限に達した候補一覧や今回の検索で未検出だった結果は、網羅的な探索の完了を意味しません。

Bukanのランチャーから起動した研究MCPは、未登録の文献IDをRustの読み取り専用索引で解決します。研究エンジンを単体で動かす場合は、研究DBへ取り込んだPaper IDか、文献MCPの`get_paper`で得た題名・DOIを渡してください。検索のために原PDFを読み出したり、Paperpileを書き換えたりしません。

必要な候補に`check_public_urls({"urls": [...]})`を使うと、最大20件の応答を確認できます。ログイン情報やCookieは送りません。結果の`check`は同じURLの候補へ追加して保存します。

| 項目 | 意味 |
| --- | --- |
| `match` | DOI一致、書誌による候補、外部候補、人が確認した一致の別。DOI一致だけでは候補PDFの中身まで照合したことにならない |
| `version` | `publishedVersion`、`acceptedVersion`、`submittedVersion`、`unknown` |
| `free_to_read` | メタデータが報告する無料閲覧の可否。`null`は不明 |
| `license` / `license_status` | ライセンス名やURLと、`unknown`・`reported`・`verified`の別。自動検索では検証済みとしない |
| `provider` / `source_url` / `discovered_at` | 情報源と取得日時 |
| `check` | 実際にURLへアクセスした日時、HTTP状態、転送先、MIME型、先頭部分のPDF識別子 |

HTTP 200でも、表示できるのは要旨やログイン画面だけかもしれません。403・429・タイムアウトは有料・非OAという意味ではありません。PDF識別子が見つかっても、全ファイルの取得や版の照合を済ませたことにはなりません。無料で読めるという報告と、再利用ライセンスの確認も分けて扱います。

## 調査結果を保存し、同じ記録を改訂する

検索とURL確認だけでは保存しません。初回は`save_public_access`へ返された`report`と`expected_revision: 0`を渡します。再調査では`search_public_access`で既存記録を探し、`get_public_access`で内容と現改訂を取得してください。以前の有用な候補・確認結果・手修正を統合してから、その改訂を`expected_revision`に指定して保存します。同時更新は競合として拒否し、過去の改訂を残します。

外部検索や別プラグインで見つけた候補も追加できます。最小限の候補は次の形です。日付は省略すると保存データを受け取った時点になるため、過去の調査を転記するときは実際の日時を入れてください。

```json
{
  "url": "https://example.org/paper.pdf",
  "provider": "external-search",
  "source_url": "https://example.org/repository-record",
  "discovered_at": "2026-09-18T00:00:00+00:00"
}
```

版やライセンスが不明でも保存できます。訂正した理由は候補の`note`やレポートの`notes`に残します。外部候補だけを新規登録する場合のレポートは、`format_version: 1`、任意の一意な`id`、`paper`、`candidates`を指定します。検索試行は`searches`へ記録できます。

`get_public_access(record_id, revision)`で任意の改訂を取得できます。`search_public_access(query, limit, offset)`は現在版の部分一致検索で、`next_offset`から続きを取得します。どちらも構造化JSONと`markdown`を返すので、文献マップへ渡したり、研究フォルダに一覧を保存したりできます。

データは選択中の研究DB内の`public_access_reports`テーブルに保存します。既存の研究レコードとは独立しており、本文の根拠や読了状態にはなりません。DBのバックアップに含まれ、JSONの`export`にも全改訂を含めます。未取得文献の購入候補を扱う`candidates/literature-access.bib`等は、そのまま別用途で使えます。

## CLIでも同じ処理を使う

研究CLIの`public-access`は、標準入力からJSONを1件読みます。`operation`は`find`・`check`・`save`・`get`・`search`で、残りは対応するMCPツールと同じ引数です。保存結果を再利用する例です。

```powershell
'{"operation":"search","query":"dust","limit":20}' | bukan research -- public-access
```

外部APIへの接続は検索・確認を呼んだときだけ行います。既存結果の取得と保存にはネット接続は不要です。OpenAlexのキーを使う場合は、MCP起動環境の`BUKAN_OPENALEX_API_KEY`に設定します。キーはレポートへ保存しません。基本的な検索にはキーなしでも利用できますが、利用枠を超えた場合はエラーを記録するので、時間を置くか別の検索手段で補ってください。[OpenAlexの認証仕様](https://help.openalex.org/api/authentication/)

Crossrefの本文リンクにはテキストマイニング向けのものもあり、リンクやライセンス登録だけで無料公開とは扱いません。OpenAlexの無料閲覧・版・ライセンスも取得時点のメタデータとして記録します。[Crossref REST API](https://www.crossref.org/documentation/retrieve-metadata/rest-api/)、[OpenAlexの公開先データ](https://help.openalex.org/data/locations/)
