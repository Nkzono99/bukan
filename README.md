# Bukan

Paperpile が Google Drive に同期した PDF を読み取り専用で整理・検索・閲覧する、
CLI と Tauri デスクトップアプリです。このリポジトリは Bukan 本体だけを管理し、
ノートや検索結果などの研究データは `bukan init` で作る外部ワークスペースへ分離します。

## 現在できること

- Windows の全ドライブから `マイドライブ/Paperpile` または
  `My Drive/Paperpile` を自動検出
- `All Papers` 以下をコレクションとして索引化
- 書誌ファイル名とサイズに基づく安定IDで、コレクション間の同一文献を統合
- `Starred Papers` と照合
- タイトル、著者、年、コレクションの横断検索
- コレクション、スター、更新日、タイトル、出版年による絞り込み・並び替え
- PDF のアプリ内表示、既定アプリでの表示、エクスプローラーでの表示
- 検出できない場合の手動フォルダ選択
- 外部ワークスペースの初期化・診断・全件走査
- 開いている外部ワークスペースを VS Code で起動
- App内のPowerShell PTYでCodex TUIを起動（PaperpileごとのApp管理作業領域、`workspace-write`）
- Viewerで選択中の文献をCodexの作業コンテキストへ設定
- CodexがMCPで提示した一時文献リストをViewerへ即時表示
- MCPからPaperpile索引・コレクション・現在の論文を読み取り専用で参照
- 一時文献リストを明示操作でWorkspaceのレポートまたは候補へ保存
- テーマ別の継続レビューをMarkdown・構造化引用・出典付き図・改訂履歴として蓄積
- Appで継続レビューを閲覧・編集し、現在のレビューをCodexへ引き継いで更新
- PaperpileのPDF追加・削除・移動を監視し、索引を自動更新
- private GitHub Releaseを起動時に確認し、GUIから署名検証付きで更新
- `taxonomy.toml` による `Bukan/` フォルダと `bukan:` ラベルの整理候補生成

Paperpile 配下に対する書き込み・移動・削除操作は実装していません。

## 開発

必要環境は Node.js、Rust、Windows では WebView2 です。

```powershell
npm install
npm run desktop:dev
```

CLI を実行する場合:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin bukan-cli -- --help
cargo run --manifest-path src-tauri/Cargo.toml --bin bukan-cli -- init D:\Research\lunar-workspace
cargo run --manifest-path src-tauri/Cargo.toml --bin bukan-cli -- organize suggest D:\Research\lunar-workspace
```

または `npm run cli -- <command>` でも実行できます。

フロントエンドのみをビルドする場合:

```powershell
npm run build
```

Rust のテスト:

```powershell
cd src-tauri
cargo test
```

マウント中の実ライブラリを走査する診断テスト:

```powershell
cargo test reports_mounted_paperpile_library -- --ignored --nocapture
```

Windows インストーラーを生成する場合:

```powershell
npm run desktop:build
```

生成物は `src-tauri/target/release/bundle/` に出力されます。

## リリース

`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` の
バージョンを揃え、同じバージョンの `v` 付きタグを push すると、GitHub Actions が
Windows 用の署名なし NSIS インストーラーをビルドし、Private GitHub Release へ
添付します。同時にTauri Updater用の`latest.json`と署名を生成します。

```powershell
git tag v0.2.1
git push origin v0.2.1
```

タグといずれかの設定ファイルのバージョンが一致しない場合、Release は作成されません。

## アプリの更新

Appは起動後にprivate GitHub Releaseを静かに確認し、新しいバージョンがある場合だけ
更新ダイアログを表示します。Top barのバージョン表示またはコマンドパレットの
「Bukanの更新を確認」から手動確認もできます。

privateリポジトリへアクセスする認証は、次の順で自動検出します。

1. AppがWindows Credential Managerへ保存したfine-grained token
2. `BUKAN_GITHUB_TOKEN`、`GH_TOKEN`、`GITHUB_TOKEN`
3. `gh auth login`済みのGitHub CLI

tokenを使う場合は対象リポジトリを`Nkzono99/bukan`、リポジトリ権限を
`Contents: Read-only`に限定できます。tokenはWorkspaceや設定ファイルへ保存しません。
各認証候補はGitHub Releases APIで検証され、期限切れの保存済みtokenがあっても
次の候補へフォールバックします。GitHub CLIの状態は`gh auth status -h github.com`で
確認でき、無効な場合は`gh auth login -h github.com`で再認証してください。

Updater署名鍵はGitHub Actionsの`TAURI_SIGNING_PRIVATE_KEY` secretへ登録済みです。
ローカルバックアップは`%USERPROFILE%\.tauri\bukan-updater.key`、パスワードの
Windows DPAPIバックアップは同じ場所の`bukan-updater.password.dpapi`です。この鍵を
失うと、既存インストールへ同じ更新経路で配布できなくなるため、2ファイルを一緒に
保持してください。これはアップデート検証用署名であり、Windowsコード署名証明書
ではありません。

## 外部ワークスペース

`bukan init <path>` は指定先へ `bukan.toml`、分類体系、`AGENTS.md`、および
`data/`, `notes/`, `queries/`, `candidates/`, `reports/`, `imports/`, `cache/`
を生成します。ワークスペースは独立した Git リポジトリとして管理できます。

形式と運用方針は [ワークスペース仕様](docs/workspace.md) を参照してください。

整理候補は `reports/bukan-organization-plan.json` に保存されます。初期ルールは
タイトル・ファイル名・既存コレクションを根拠にするため、Paperpileへ反映する前に
`要確認` の項目をレビューしてください。

文献IDはPaperpileのファイル名から得られる書誌情報とファイルサイズから生成する
`p2-...` 形式です。PDF本文を開かないためDriveのオンデマンドファイルを実体化せず、
コレクション間を移動しても同じ文献を追跡できます。旧版のパス由来IDも
`legacyId` として返すため、既存連携は段階的に移行できます。

## 継続レビュー

CodexがBukan MCPの`create_review`でテーマを作ると、
`reports/reviews/<review-id>/` に現在の本文`article.md`、構造化された引用情報
`review.json`、過去の本文`history/`、出典付き画像`figures/`を保存します。
Appはレビューの閲覧とMarkdown本文の直接編集に使い、「Codexで更新」を選ぶと対象が
`.bukan/current-review.md`と`BUKAN_REVIEW_FILE`を通してCodexへ渡ります。

CodexはBukan MCPの`search_library`、`get_paper`でローカル文献を確認し、
`update_review`で本文と引用元の論文ID・ページまたは節を一緒に更新します。
図を使う場合はPDFから抽出した画像をまずWorkspaceの`cache/`等へ保存し、
`attach_review_figure`でレビューへ複製します。Paperpile内のPDFは常に読み取り専用で、
図には元論文IDとページ番号を記録します。
