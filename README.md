# Bukan

Paperpile が Google Drive に同期した PDF を、読み取り専用で整理・検索・閲覧する
Tauri デスクトップアプリです。研究ワークスペースの運用方針は [SPEC.md](SPEC.md) を
参照してください。

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

## ワークスペース

- `data/`: Paperpile から自動同期する BibTeX（直接編集禁止）
- `queries/`: 再現可能な検索条件
- `candidates/`: 未登録候補
- `reports/`: 検索レポートとエビデンス表
- `imports/`: Paperpile へ戻す BibTeX と PDF
- `notes/`, `cache/`: 抽出テキスト、要約、作業キャッシュ

詳細な運用ルールは [AGENTS.md](AGENTS.md) に固定しています。
