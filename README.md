# Bukan

Paperpile が Google Drive に同期した PDF を読み取り専用で整理・検索・閲覧する、
CLI と Tauri デスクトップアプリです。このリポジトリは Bukan 本体だけを管理し、
ノートや検索結果などの研究データは `bukan init` で作る外部ワークスペースへ分離します。

## 現在できること

- Windows の全ドライブから `マイドライブ/Paperpile` または
  `My Drive/Paperpile` を自動検出
- `All Papers` 以下をコレクションとして索引化
- 同一ファイル名・サイズの文献を統合
- `Starred Papers` と照合
- タイトル、著者、年、コレクションの横断検索
- コレクション、スター、更新日、タイトル、出版年による絞り込み・並び替え
- PDF のアプリ内表示、既定アプリでの表示、エクスプローラーでの表示
- 検出できない場合の手動フォルダ選択

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
```

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

## 外部ワークスペース

`bukan init <path>` は指定先へ `bukan.toml`、分類体系、`AGENTS.md`、および
`data/`, `notes/`, `queries/`, `candidates/`, `reports/`, `imports/`, `cache/`
を生成します。ワークスペースは独立した Git リポジトリとして管理できます。

形式と運用方針は [ワークスペース仕様](docs/workspace.md) を参照してください。
