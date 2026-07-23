## 現在のアプリケーション方針

Bukanを利用者が日常的に触る統合フロントエンド、Paperpileを文献・PDF・引用情報の
正本として扱う。Paperpile同期フォルダは常に読み取り専用であり、Bukanが生成する
ノート、検索条件、候補、レポート、インポート一式は外部Workspaceへ保存する。

```text
Bukan App
├─ Paperpile Viewer / Collections（read-only）
├─ Bukan分類候補
├─ Raw Codex TUI
└─ Workspace notes / reports / candidates
```

CodexはBukan独自のチャットUIで包まず、App内のPTY上で生のCodex CLIを起動する。
作業ディレクトリはGoogle Drive等に置かれたBukan Workspaceで、起動時に
`workspace-write`サンドボックスと`on-request`承認を指定する。Paperpileフォルダを
追加の書き込み可能ディレクトリにはしない。

Viewerで選択した論文は`.bukan/current-context.md`を介してCodexへ渡す。このファイル
には読み取り専用PDFへの参照と文献情報だけを保存し、Git管理しない。将来のRAG/MCP
実装でも、Paperpileへのアクセス制御とパス検証はRustコアへ集約する。

Codexが検索・比較・収集した文献リストは、Bukan MCPの`present_paper_list`でGUIへ
一時表示できる。リストは初期状態ではローカルブリッジだけに保持し、自動的に
PaperpileやWorkspaceへ永続化しない。利用者がViewerで明示した場合に限り、
`reports/codex-lists/`または`candidates/codex-lists/`へ保存する。これにより専用RAG
索引を必須にせず、生のCodexを検索・選定エンジンとして使いながら、結果確認と
採用判断をViewerで行える。

Bukan MCPは`search_library`、`list_collections`、`get_paper`、
`get_current_paper`を読み取り専用で提供する。ViewerとMCPはRustコアの同じ索引を
使用し、書誌ファイル名とサイズに基づく安定IDで同一文献を参照する。索引作成時に
PDF本文を開かず、Google Driveのオンデマンドファイルを不要に実体化しない。
Paperpileの同期領域へ書き込むMCPツールは提供しない。

Paperpile公式APIまたはMCPが一般提供されるまでは、分類・ラベル変更は適用計画と
インポート候補までに留める。同期フォルダを直接変更する実装は行わない。

---

## 結論

**Paperpileを文献データの正本、Codexを検索・分析・整理の作業エンジンにする構成**がおすすめです。

2026年7月現在、Paperpileは公式のREST APIとMCPサーバーを開発中ですが、まだ一般提供されていません。そのため現時点では、以下の経路が最も安定します。Paperpile公式MCPが公開された段階で、手動インポート部分だけを置き換えられます。([Paperpile][1])

```text
                         ┌─ OpenAlex
                         ├─ Crossref
自然言語 ──> Codex ──────┼─ PubMed / PMC
                         ├─ PaperpileのBibTeX
                         └─ Paperpile同期済みPDF
                                │
                                ▼
                   候補一覧・比較表・要約・BibTeX
                                │
                                ▼
                         Paperpileへ一括登録
```

## 1. 既存論文を読むだけなら「Ask AI」

Paperpileにはすでに「Ask AI」というパブリックベータ機能があり、選択したPDFをChatGPT、Claude、Gemini、Copilot、NotebookLMへ送れます。Paperpile側で論文を選択し、要約、方法論の検討、複数論文の比較などを依頼できます。これは**既に収集した論文を読む用途**には最も手軽です。([Paperpile][2])

ただし、現時点では以下の制約があります。

* Codex CLIへ直接送る機能ではない
* AIの回答をPaperpileのノートへ自動保存できない
* Paperpileライブラリを自然言語で検索・更新する公式MCPはまだ未提供

Paperpile自身も、MCPサーバーとAI回答のノート保存について「まだ提供していない」と明記しています。([Paperpile][2])

したがって、**読む作業はAsk AI、探す・差分管理する作業はCodex**と分けるのが実用的です。

---

## 2. Codex用の「文献ワークスペース」を作る

Codexはローカルリポジトリのファイルを調査し、スクリプトを実行し、反復可能なワークフローを構築できます。また、MCPサーバーを通じて外部サービスや検索APIにも接続できます。([OpenAI Developers][3])

次のようなディレクトリを1つ作ります。

```text
research-workspace/
├── AGENTS.md
├── data/
│   └── paperpile.bib          # Paperpileから自動同期、編集禁止
├── papers/
│   └── paperpile-pdfs/        # Google Drive同期先への参照
├── queries/
│   └── current-search.yaml
├── candidates/
│   ├── candidates.csv
│   └── candidates.bib
├── reports/
│   ├── search-report.md
│   └── evidence-matrix.csv
├── imports/
│   └── paperpile-import.bib
└── tools/
    ├── search_papers.py
    ├── deduplicate.py
    ├── extract_pdf.py
    └── build_import.py
```

ここで重要なのは、次の役割分担です。

| 領域              | 役割                           |
| --------------- | ---------------------------- |
| Paperpile       | 正式な文献情報、フォルダ、ラベル、PDFの管理      |
| `paperpile.bib` | Codexが「すでに持っている論文」を判断するための索引 |
| PDF同期フォルダ       | Codexが全文を比較・要約するための読み取り元     |
| `candidates/`   | 検索したがまだPaperpileに入れていない候補    |
| `imports/`      | Paperpileへ追加することが確定した文献      |

---

## 3. PaperpileからBibTeXを自動同期する

Paperpileは、フォルダ、ラベル、またはライブラリ全体をBibTeXファイルとして自動同期できます。同期先はGoogle Drive、GitHub、更新可能なダウンロードリンクから選べます。([Paperpile][4])

### おすすめ設定

Paperpileで以下を設定します。

```text
プロフィール
  → Workflows and Integrations
  → Add
  → BibTeX Export
```

対象は、最初はライブラリ全体よりも、例えば次のような専用ラベルがおすすめです。

```text
AI-Workspace
```

保存先は次のどちらかです。

* **Google Drive**：セットアップが簡単
* **GitHub**：変更履歴が残り、Codexとの相性がよい

GitHubを使う場合は、例えば以下に同期します。

```text
data/paperpile.bib
```

Paperpileで文献を追加・編集すると、Paperpile BotがBibTeXを更新できます。なお、自動同期されたBibTeXは読み取り専用として扱う必要があり、直接編集した内容は次回同期時に上書きされます。([Paperpile][4])

したがって、Codexには次のルールを与えます。

```text
data/paperpile.bib は絶対に編集しない。
新規候補は imports/paperpile-import.bib に書き出す。
```

---

## 4. PDFをCodexから参照できるようにする

PaperpileはPDFをGoogle Driveへ同期できます。デフォルトでは著者名の頭文字によるフォルダ分けと、「Author year - Title」の形式でファイル名が付けられます。([Paperpile][5])

Google Driveデスクトップ版でPaperpileフォルダをローカル表示し、ワークスペースから参照します。

例：

```bash
ln -s "/path/to/Google Drive/Paperpile" papers/paperpile-pdfs
```

ただし、同期先のPDFは**読み取り専用**として使います。ファイル名変更、移動、削除はGoogle Drive側ではなくPaperpile側で実施する必要があります。Paperpile公式も、変更はPaperpile上で行うよう案内しています。([Paperpile][5])

CodexにはPDFを直接変更させず、抽出テキストや要約を以下へ保存させます。

```text
reports/
notes/
cache/
```

---

## 5. 論文検索用のAPIをCodexに使わせる

初期段階では、MCPを自作しなくても、PythonスクリプトをCodexに実行させれば十分です。

### 基本の検索構成

**OpenAlex**

広い研究分野を横断して候補を探す主検索エンジンにします。論文、著者、研究機関、トピックなどをAPIで検索でき、出版年や論文種別でフィルタリングできます。([OpenAlex Developers][6])

**Crossref**

DOI、タイトル、著者、出版年、ライセンス、出版後の更新情報などの確認に使います。検索結果をそのまま信用せず、Crossrefで書誌情報を正規化する構成が堅実です。([www.crossref.org][7])

**PubMed / PMC**

医学・生命科学の場合は必須です。NCBIのE-utilitiesからPubMedやPMCを検索・取得できます。([NCBI][8])

実装するツールは、最初は次の5つで足ります。

```text
search_papers
check_existing_library
get_metadata
find_open_access_pdf
create_paperpile_import
```

MCP化する場合も、同じ5つをMCPツールとして公開すればよいです。Codex CLIはローカルプロセスとして動くSTDIO型MCPと、HTTP型MCPの双方を利用できます。([OpenAI Developers][3])

---

## 6. Codexに読ませる `AGENTS.md`

Codexは作業開始時にリポジトリ内の`AGENTS.md`を自動的に読みます。ここに文献管理ルールを書いておくと、毎回同じ説明をする必要がありません。([OpenAI Developers][9])

以下をそのまま初期版として使えます。

```markdown
# Literature Research Workspace

## Source of truth

- Paperpile is the source of truth for the accepted literature library.
- `data/paperpile.bib` is automatically exported from Paperpile.
- Never edit `data/paperpile.bib`.
- Never rename, move, delete, or modify files under `papers/paperpile-pdfs/`.
- Write new candidate references to `candidates/`.
- Write references approved for import to `imports/paperpile-import.bib`.

## Search workflow

When asked to find papers:

1. Convert the user's request into:
   - research question
   - inclusion criteria
   - exclusion criteria
   - date range
   - preferred study types
   - synonyms and related terminology

2. Search:
   - OpenAlex for broad discovery
   - Crossref for DOI and bibliographic verification
   - PubMed/PMC for biomedical topics
   - other domain-specific databases when appropriate

3. Deduplicate results against `data/paperpile.bib` using:
   - DOI
   - PMID or other persistent identifier
   - normalized title
   - title and first-author similarity

4. Do not treat citation count as proof of research quality.

5. Separate:
   - discovered
   - metadata verified
   - full text available
   - full text reviewed
   - recommended for import

6. Never invent:
   - DOI
   - PMID
   - article title
   - quotations
   - page numbers
   - statistical results

7. For every search, record:
   - search date
   - databases searched
   - actual query
   - inclusion and exclusion criteria
   - reasons for ranking each paper

## Output files

Create:

- `candidates/<topic>.csv`
- `candidates/<topic>.bib`
- `reports/<topic>-search-report.md`
- `imports/<topic>-paperpile-import.bib`

Candidate CSV columns:

- rank
- title
- authors
- year
- journal
- DOI
- PMID
- study_type
- abstract
- open_access_url
- already_in_paperpile
- relevance_reason
- limitations
- verification_status

## PDF policy

- Only download open-access PDFs or files the user is authorized to access.
- Save downloaded candidate PDFs outside the Paperpile sync directory.
- Cite the PDF page or section when extracting important claims.
```

---

## 7. 実際にCodexへ投げる指示

### 文献探索

```text
2022年以降に発表された、生成AIを用いた臨床文書作成支援の論文を探して。
システマティックレビュー、RCT、前向き比較研究を優先すること。

data/paperpile.bibと照合し、すでにPaperpileにある論文は除外して。
OpenAlexとPubMedで探索し、CrossrefでDOIを確認すること。

上位20件をcandidates/clinical-documentation-ai.csvに保存し、
各論文について採用理由、研究デザイン、主な限界、オープンアクセスの有無を示して。
```

### 手持ち文献との差分調査

```text
data/paperpile.bibに含まれる「LLM evaluation」関連文献を分析して。
既存文献の出版年、研究対象、評価指標を整理したうえで、
現在のライブラリで不足している研究領域を5つ挙げて。

その不足領域を補う論文を探索し、未所蔵のものだけ提示して。
```

### 複数PDFの比較

```text
papers/paperpile-pdfs以下から、○○に関する論文を特定して。
対象集団、データセット、モデル、比較対象、主要評価指標、結果、限界を抽出し、
reports/evidence-matrix.csvを作成して。

論文に書かれていないことは推測せず、不明と記載すること。
```

### 反証論文の探索

```text
「○○は△△を改善する」という主張について、
支持する論文だけでなく、否定的結果、再現失敗、効果が限定的だった研究を優先して探して。
既存のPaperpileライブラリとの重複は除外すること。
```

### 定期ウォッチ用の検索定義

```yaml
name: llm-clinical-documentation
question: >
  Do large language models improve the quality or efficiency
  of clinical documentation?
year_from: 2024
languages:
  - English
  - Japanese
include:
  - prospective study
  - randomized controlled trial
  - systematic review
exclude:
  - editorial
  - opinion
  - non-clinical benchmark only
databases:
  - openalex
  - pubmed
verification:
  - crossref
max_results: 30
```

Codexに、

```text
queries/current-search.yamlに従って検索を更新して。
前回のcandidates.csvとの差分のみ報告して。
```

と依頼すれば、新規論文の監視にも使えます。

---

## 8. Paperpileへ戻す方法

Codexが採用候補を確定したら、以下を生成させます。

```text
imports/topic-name/
├── paperpile-import.bib
└── files/
    ├── paper1.pdf
    └── paper2.pdf
```

BibTeXにはPDFの相対パスを入れます。

```bibtex
@article{smith2026example,
  author = {Smith, Jane and Doe, John},
  title = {Example Study},
  journal = {Example Journal},
  year = {2026},
  doi = {10.xxxx/example},
  file = {files/paper1.pdf}
}
```

Paperpileへ`.bib`とPDFをまとめてドラッグ＆ドロップすると、メタデータとの照合に成功したPDFは文献へ自動添付されます。BibTeXの`file`フィールドで明示的に紐付けることもできます。また、重複スキップはデフォルトで有効です。([Paperpile][10])

これにより、現時点でも書き戻し操作は実質的に、

1. Codexがインポート一式を作成
2. Paperpileへフォルダをドラッグ＆ドロップ

の2段階に抑えられます。

---

## 推奨する導入順序

### 段階1：すぐ開始

* PaperpileのAsk AIを有効化
* BibTeX自動同期を設定
* PDFのGoogle Drive同期を設定
* Codex用リポジトリと`AGENTS.md`を作る

### 段階2：検索を自動化

* OpenAlex検索
* Crossrefメタデータ検証
* DOI・タイトルによるPaperpile重複判定
* CSV、Markdown、BibTeXの自動生成

### 段階3：MCP化

検索スクリプトをMCPサーバーとして公開し、Codexから次のように直接操作できる状態にします。

```text
「2024年以降の○○論文を探し、Paperpile未登録だけを表示して」
「上位5本をインポート用BibTeXにして」
「手持ちPDFとのエビデンス表を作って」
```

### 段階4：Paperpile公式MCPへ移行

Paperpile公式MCPが提供されたら、手動インポートを以下のような直接操作へ置き換えます。

```text
paperpile.search_library
paperpile.add_reference
paperpile.attach_pdf
paperpile.add_to_folder
paperpile.apply_label
paperpile.save_note
```

## 最もおすすめの構成

現時点では、次の組み合わせがバランスに優れています。

```text
Paperpile
  ├─ 文献とPDFの正本
  ├─ Google Drive PDF同期
  └─ GitHubまたはDriveへのBibTeX自動同期

Codex
  ├─ AGENTS.mdで検索ルールを固定
  ├─ OpenAlexで広く探索
  ├─ PubMed等で分野別検索
  ├─ CrossrefでDOI・書誌情報を検証
  ├─ Paperpile所蔵文献との差分判定
  └─ インポート用BibTeXとPDF一式を生成
```

なお、Ask AIではPDFが現在ログインしているAIサービスのアカウントへアップロードされます。未公開原稿、患者情報を含む資料、機密性のある共同研究資料については、大学・組織が承認したアカウントと利用条件に限定すべきです。([Paperpile][2])

[1]: https://paperpile.com/roadmap/ "Roadmap - Paperpile"
[2]: https://paperpile.com/h/ask-ai/ "Send PDFs to AI assistants with Ask AI | Paperpile Help Center"
[3]: https://developers.openai.com/codex/mcp "
  Model Context Protocol | ChatGPT Learn
"
[4]: https://paperpile.com/h/sync-bibtex-files/ "Automatically sync BibTeX files | Paperpile Help Center"
[5]: https://paperpile.com/h/sync-google-drive/ "Sync with Google Drive | Paperpile Help Center"
[6]: https://developers.openalex.org/api-reference/introduction "API Overview - OpenAlex Developers"
[7]: https://www.crossref.org/documentation/retrieve-metadata/rest-api/ "REST API - Crossref"
[8]: https://www.ncbi.nlm.nih.gov/home/develop/api/ "APIs - Develop - NCBI"
[9]: https://developers.openai.com/codex/agent-configuration/agents-md "
  Custom instructions with AGENTS.md | ChatGPT Learn
"
[10]: https://paperpile.com/h/import-ris-bibtex/ "Import data from any program via RIS or BibTeX files | Paperpile Help Center"
