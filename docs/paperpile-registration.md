# Paperpileへの文献登録

文献管理MCPの`paperpile_import_references`が、登録・所蔵確認に続いて、PaperpileのAuto updateとPDF検索を実行します。Bukan専用のChromeプロファイルを使うため、Codex以外のMCPクライアントからも呼び出せます。ホスト側のブラウザ操作機能は不要です。Paperpileサーバーへの同期完了はまだ自動確認できません。

## 初回のログイン

Google Chromeをインストールした環境で、Bukanの導入後に一度実行します。

```powershell
python -m bukan paperpile login
```

専用ChromeウィンドウでPaperpileにログインし、ライブラリが表示されたらそのウィンドウを閉じます。コマンドは、保存されたログイン状態で接続できるかを確認して終了します。通常のChromeとは別のプロファイルで、既存のCookieをコピーすることはありません。初回コマンドはブラウザ操作用のPython依存を追加しますが、研究DBは開きません。Bukan更新後に追加の依存準備が必要な場合は、次のブラウザ操作で自動的に行います。

Auto updateとPDF検索には、この専用ChromeにもPaperpile拡張機能を追加してください。Paperpileの「Install extension」から追加できます。普段のChromeへの導入とは別です。拡張機能の追加は初回だけ手動で行い、以後はMCPが同じプロファイルで利用します。拡張機能がなくても文献登録は可能ですが、後処理は`extension_required`になります。

ログイン状態はWindowsでは`%LOCALAPPDATA%/bukan/paperpile-browser/profile`に保存します。`BUKAN_DATA_DIR`を設定している場合はその配下です。複数の研究ワークスペースで同じPaperpileアカウントを利用し、同時操作はロックで防ぎます。このディレクトリにはログイン状態が含まれるため、配布物やGitには含めません。

ログインが切れた場合は同じコマンドを再実行します。接続確認だけなら`python -m bukan paperpile status`、MCPからは`paperpile_browser_status`を使います。専用ウィンドウを開いたままの場合は、閉じてからMCPを呼び出してください。Linuxでは初回ログイン用のデスクトップ環境も必要です。

ブラウザ用の依存は通常の研究機能から分離しています。Chrome／Playwrightが対応しない環境（musl Linuxなど）でも従来の文献読解・研究機能を利用できますが、Paperpile登録は対応環境で実行してください。

## 使い方

> このDOIの論文をPaperpileのMy Libraryに登録して。重複はスキップして。

プラグインの`bukan-paperpile`スキルがMCPを呼び出します。調査や候補一覧の作成だけでは登録しません。登録を依頼済みなら、同じ許可を繰り返し求める必要はありません。

`paperpile_import_references`の例です。

```json
{
  "format": "identifiers",
  "text": "10.1002/2016GL069491",
  "destination": "My Library"
}
```

DOI・URLは1行に1件、最大100件、入力全体は64 KiBまでです。DOI表記を統一し、入力内の重複を除きます。`bibtex`、`ris`の場合は、文献数の`expectedCount`も指定します。Paperpileが一部だけ解析した場合に、そのまま登録しないための照合値です。数件ずつの小さなバッチを推奨します。

**現在の自動登録先はMy Libraryのみです。** フォルダ・共有ライブラリの指定、手元PDFのアップロード、文献の統合は含みません。非対応の登録先を指定した場合は、別の場所へ追加せずエラーにします。

`prepare_paperpile_import`は入力だけ確認したい場合の補助ツールです。ネットワークアクセスや登録は行いません。通常は`paperpile_import_references`の1回の呼び出しで入力整理から登録まで進みます。

Paperpile上での解析・重複判定だけを試す場合は、`paperpile_import_references`に`previewOnly: true`を付けます。プレビューを取得してキャンセルし、登録・書誌更新・PDF検索は行いません。登録だけに限定する場合は`postprocess: false`を指定します。省略時は`true`で、所蔵確認後に後処理を行います。すべて既存文献だった場合も後処理の対象です。

## 結果の確認

登録処理は **Add > Paste** を開き、貼り付けイベントで文献を解析します。件数、My Library、重複除外を確認した後に1回だけImportを押します。その後にページを再読み込みし、同じ入力のプレビューがすべて既存文献になることを確認します。確認用のプレビューではImportを押しません。

| `status` | 意味 |
| --- | --- |
| `present_in_browser` | 登録を実行し、再読み込み後にブラウザ内で全件の所蔵を確認した。サーバー同期は未確認 |
| `already_present` | ブラウザ内で全件が既存文献だった。登録操作は行っていない。サーバー同期は未確認 |
| `preview_mismatch` | 解析件数が入力と合わず、登録前に停止した |
| `preview_only` | 指定されたプレビューだけを行い、キャンセルした。登録していない |
| `ui_unavailable` | 登録前に画面の状態を確認できず停止した |
| `unknown` | 登録を試みたが、結果を確認できなかった |
| `login_required` / `busy` / `browser_unavailable` | ログイン・実行中の操作・専用Chromeを確認する必要がある |

`unknown`や呼び出しの切断は、未登録とは限りません。再実行時もPaperpileの重複チェックを通し、重複除外を解除して登録を強制しないでください。画面が閉じただけでは成功と判定しません。

Paperpileは[ローカルDBからサーバーへ非同期で保存する仕組み](https://paperpile.com/h/performance-tips/)です。再読み込み後の重複チェックでも、確認できるのは同じブラウザ内の状態までです。そのため`serverSyncVerified`は常に`false`で、別端末からの利用やサーバーへの永続化までは保証しません。同期完了の確認が必要な場合は、`paperpile login`で専用ウィンドウを開き、Paperpileの同期表示を確認してください。

## PDFとの境界

登録後、MCPが対象文献を1件ずつ照合して`More > Auto update`を実行し、PDFのない文献には`More > Find PDFs online`を実行します。これは`Add PDF > Find PDF online`と同じPaperpileの検索機能です。既存PDFは保持し、ライブラリ全体を一括選択しません。

返却値の`status`は登録結果、`postprocessing`は後処理結果です。`postprocessing.references`に文献ごとの`metadata`と`pdf`が入り、書誌情報の更新済み／変更なし／照合不可／要確認と、PDFの既存／取得済み／アクセス制限／未発見／CAPTCHA等を区別します。`postprocessing.status`が`completed`でも、PDFが見つからなかった結果を含むため、個々の`pdf.status`を確認してください。`pending`、`unknown`、`not_run`を取得済みとは扱いません。

新しい文献を処理し始める時間の目安は1バッチ3分で、上限後の文献は未処理として返します。少数件のバッチを推奨します。結果不明時の自動再送は行わず、必要なら同じ入力を再実行します。登録は重複確認を通し、後処理でも取得済みPDFを再検索しません。書誌更新が失敗しても、対象を再照合できればPDF検索を試します。

DOI・URLは編集画面の値まで照合し、BibTeX／RISはPaperpileの解析プレビューにある題名・著者・年で照合します。同じ候補が複数あれば推測で選びません。Auto updateの保存は、対象のDOIと提案の文献同一性を確認できる場合に限ります。DOIのない文献や別DOIへの変更候補、読み取れない提案は`needs_review`として保存せず、確認済みの元文献についてPDF検索を続けます。未取得の文献とBibTeXは後日の取得検討に使います。

新しいURLは、同じDOIの解決URLであるか、リンク先ページの`citation_doi`等の論文メタデータが一致するか確認します。既存の確認済みURLは保持します。ページを読めない場合や著者プロフィール等で同一性を確認できない場合は、提案を保存せず`needs_review`を返します。プロフィール内の業績一覧を講演の書誌確認に使う場合も、そのページ名を講演題名に置き換えたり、全文の取得先として扱ったりしません。

登録、PDF取得、Drive同期、Bukanでの索引化は別の結果です。返却値の`pdfSyncVerified`は`false`で、文献登録だけでは全文を読める状態になったとは扱いません。PDFが同期された後に既存の文献検索・PDF取得MCPで確認します。

Paperpileの同期フォルダへ直接ファイルを追加する方式は採用しません。同期済みファイルは引き続き読み取り専用です。登録処理のPython実装は依存配布のため研究エンジンのパッケージに同梱しますが、研究DBを利用・更新せず、Rustの文献管理MCPから呼び出します。

## 対応方式と検証

Paperpileの[公式ヘルプ](https://paperpile.com/h/paste-reference-data/)が案内する貼り付けインポートを使います。非公開APIには依存しません。公式REST APIは[2026-09-15の開発元回答](https://forum.paperpile.com/t/public-developer-api/918?page=4)では開発中です。公開後はブラウザ操作部分を置き換えられる構成にしています。

2026-09-18にログイン済みChromeの画面で、新規文献の解析プレビューと所蔵済みDOIの重複プレビューを確認し、どちらも登録せずキャンセルしました。自動テストでは独立したChromeとローカルの画面を使い、貼り付け、重複除外、件数不一致時の停止、登録後の再確認、結果不明時の停止を検証します。実際の新規文献の登録は、この確認には含めていません。

後処理は独立したブラウザのテストで、操作順、プレビュー時の無変更、既存PDF保持、別文献の更新拒否、拡張機能不足、アクセス制限の返却を検証します。実機でも専用プロファイルの拡張機能からAuto updateとPDF検索を確認しました。出版社側のCAPTCHAや購読制限を回避する実装ではありません。
