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
└── cache/                 # 再生成可能な索引・抽出テキスト（Git対象外）
```

## Paperpileとの境界

- `bukan.toml` の `paperpile.path = "auto"` はマウント済みドライブを探索します。
- 絶対パスを指定すれば、ワークスペース単位で異なる同期先を使用できます。
- Paperpile の同期フォルダは常に読み取り専用です。
- PDFを移動・改名せず、タグ提案・ノート・分析結果だけをワークスペースへ保存します。

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
