# Bukan

Paperpileと連携する文献管理・研究解析のCLI／MCPツールです。CodexなどのMCPクライアントから調査を進め、根拠をたどれる研究ノートとwikiを育てます。

- 所蔵文献の検索、PDF本文・ページ画像の取得
- Paperpileへの文献登録、公開版・プレプリント候補の検索
- 全文読解ノート、主張と根拠、論文間の関係の保存
- 研究wikiの改訂と、文献ごとのレビュー作業の管理

## はじめる

対応環境はWindows／Linuxのx86_64、Python 3.11以上です。文献検索にはGoogle Drive等から見えるPaperpile同期フォルダ、LinuxではPopplerも用意してください。

1. [Releases](https://github.com/Nkzono99/bukan/releases/latest)からOSに合うwheelを取得し、導入・初期化します。Windowsの例です。

   ```powershell
   python -m pip install ./bukan-0.3.0-py3-none-win_amd64.whl
   python -m bukan install
   ```

2. 完了時に表示される手順で、Codexの個人マーケットプレイスから**Bukanプラグイン**を追加します。

新しい会話で「Bukanの接続先を確認して、保存済み文献から調査を始めて」と依頼してください。PyPIには未公開です。Python未導入のWindowsでは、配布zip内の`bukan/install.cmd`も使えます。

Paperpileへ文献を登録する場合は、Google Chromeを用意して初回に`python -m bukan paperpile login`を実行します。更新は、新しいwheelをpipで導入した後に`python -m bukan update`を実行します。

研究データはアプリ本体とは別に保存します。Windowsの新規環境では`%LOCALAPPDATA%/bukan/workspaces/default`を使い、既存の研究フォルダが設定済みなら引き継ぎます。Paperpile同期ファイルへの直接アクセスは読み取り専用です。

## ドキュメント

- [導入・更新・CLIの使い方](docs/usage.md)
- [Paperpileへの登録とPDF取得](docs/paperpile-registration.md)
- [研究wikiの設計](docs/research-wiki.md)
- [全文読解](docs/full-paper-reading.md)・[網羅的サーベイ](docs/exhaustive-review.md)
- [研究エンジン](research-engine/README.md)・[プラグイン](plugins/bukan/README.md)

## 開発

RustのCLI・文献管理層は`crates/bukan/`、Pythonの研究エンジンは`research-engine/`にあります。

```sh
cargo test --workspace --locked
uv run --project research-engine --locked --extra paperpile pytest research-engine/tests -q
python -m unittest discover -s scripts -p "test_*.py"
```

配布wheelはRustのビルド環境とPythonの`build`パッケージを用意し、`python -m build --wheel`で作成できます。
