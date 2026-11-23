# このフォークについて（cool0707/umya-spreadsheet）

[umya-spreadsheet](https://github.com/MathNya/umya-spreadsheet) 3.1.1 を元にした、xljs（`cool0707/xljs`）と RAS（`cool0707/ras`）のためのフォーク。
両方が使う xlsx / xlsm の読み書きをここにまとめる。xljs や RAS がブックのファイル（zip の中の XML）を自分で読み書きする代わりに、ここに足して両方から使う。

## 1. ブランチと使い方

| ブランチ | 中身 |
|---|---|
| `excel-compat` | 本流。3.1.1 ＋このフォークの変更。xljs と RAS はどちらもこのブランチを使う |
| `master` | 本家を追うもの（本家の 4.0 系を取り込んだもの）。xljs・RAS は使わない |
| 作業ブランチ | xljs・RAS の作業ブランチと同じ名前で作る。できたら `excel-compat` に取り込む |

- xljs・RAS は crates.io の `umya-spreadsheet = "3.1"` を `Cargo.toml` の `[patch.crates-io]` でこのフォークの `excel-compat` に差し替える。`Cargo.toml` は書き換えない。
- 両方の CI は、自分のブランチと同じ名前のブランチがこのフォークにあればそれで組み立てる。xljs は `tools/umya-branch.sh`、RAS は `.github/scripts/umya-branch.sh`。RAS のビルドは `--locked` なので、その場で `cargo update -p umya-spreadsheet` もする。
- 手元で試すとき: `cargo test --config 'patch.crates-io.umya-spreadsheet.path="../umya-spreadsheet"'`。`Cargo.lock` が書き換わるのでコミットしない。
  - RAS のテストには中で `cargo build` を走らせるもの（`crates/ras/tests/offline.rs`）がある。`--config` は引き継がれないので、`.cargo/config.toml` に同じ `[patch.crates-io]` を一時的に書く。
- 作業ブランチを `excel-compat` に取り込んだら、xljs と RAS の両方で `cargo update -p umya-spreadsheet` を実行し、`Cargo.lock` だけを進めてコミットする。遅れていると両方の CI が警告する。

## 2. xljs と RAS が共有するもの

### 2.1 モジュール

| モジュール | 中身 | 使うところ |
|---|---|---|
| `xml_tree` | XML の木（`Node`。深さ 256 より下は捨てる）。読み方は 3 つ: `parse`（整形式だけ）、`parse_partial`（読めたところまで）、`parse_lenient`（終了タグを照合しない。VML 用） | xljs `src/xml.rs`（再エクスポート）、RAS `view/xml.rs` |
| `drawingml::geometry` | 図形の形。ECMA-376 の既定の図形とワードアートの文字の変形の定義（`src/drawingml/presets.zip`）、ガイドの式、フリーフォームのパスを評価して SVG のパスにする | xljs の印刷、RAS の `ras book view` |
| `drawingml::color` | テーマの色、色の変換（`lumMod` など）、塗り、スタイルの参照 | 同上 |
| `drawingml::anchor` | アンカーとセルの位置、`xfrm` | 同上 |
| `drawingml::chart` | グラフの自動の色（`colorsN.xml`）、系列の参照、キャッシュの値 | 同上 |
| `drawingml::package` | 部品のパスの解決、リレーション、`Package` トレイト（`sheet_part`・テーマの色） | 同上 |
| `vba` | `vbaProject.bin` の読み書き: CFB、MS-OVBA の圧縮と展開、`dir` ストリーム、モジュールのソースとコードページ、p-code なしの書き出し、プロジェクトの保護、VBA のストンピングの検出、ユーザーフォームのデザイナ ストレージ。`.xls` の `_VBA_PROJECT_CUR` も読める（`VbaProject::read_storage`） | xljs `src/vba`、RAS `crates/ras-ovba` |

- 図形の定義の作り直し方は `tools/preset-shapes/make.py`。
- 図形を HTML や SVG に描くところは、xljs と RAS でそれぞれに持つ。描く先が違う（xljs は印刷、RAS は画面）。
- RAS の `crates/ras-ovba` には、バイトを保つ OPC パッケージの操作が残る。`pack` / `unpack`、シートの操作、`.xlsb` の読み取りなど。これらは変更のないパートを元のバイトのまま残すために、わざとこのクレートのモデルを通さない。

### 2.2 モデルから読めるもの

以前は xljs や RAS がファイルを開き直して読んでいた。今はこのクレートのモデルが読んで持ち、保存でも書き戻す。

| 読めるもの | API |
|---|---|
| 1904 年方式 | `Workbook::date1904` |
| 標準のスタイルのフォント（読む・設定する。保存で最初のフォントとして書く）、セルに書式がないときの書式 | `Workbook::default_font` / `set_default_font` / `normal_style` |
| ブックが定義するテーブルスタイルと、その差分書式 | `Workbook::table_styles` / `default_table_style` / `differential_format`。差分書式の未指定と「オフ」を区別するのは `Font::bold_value` など |
| テーブルの見出し行の数・集計行の数式・ほかの属性。名前付きスタイルのオプション（縞模様など）を正しく読む | `Table::header_row_count` / `TableColumn::totals_row_formula` / `other_attributes` |
| 印刷の設定（`pageSetup` の属性、印刷する枠線・行列番号） | `PageSetup::has_fit_to_width` / `black_and_white` / `page_order` / `first_page_number`、`PrintOptions::grid_lines` / `headings` |
| シートの表示（枠線、行列番号、ゼロ値、右から左、数式、枠線の色、アウトライン記号） | `SheetView::show_grid_lines` などと `remove_pane`。`showGridLines` がないときは「表示する」と読む |
| ブックのウィンドウ（シート見出し・スクロールバー） | `WorkbookView::show_sheet_tabs` |
| ほかのファイルの場所へのハイパーリンク（アドレスと場所の両方） | `Hyperlink::target_location` |
| フィルターの条件（値・比較・トップテン）と、条件で言えない絞り込み | `FilterColumn::criteria` / `is_filtered`。日付のグループ・色・アイコンなどは読んだまま書き戻す |
| 図形を印刷するか・ロックするか、代替テキスト、画像やコネクタのマクロ | `ClientData`、`NonVisualDrawingProperties::description` / `title`、`Picture::set_macro` |
| 「'」を付けて入力した文字列、ピボットのボタン | `Style::quote_prefix` / `pivot_button` |
| VBA プロジェクトの署名 | `Workbook::has_vba_signature`。プロジェクトを置き換えると消える |

## 3. 保存で落ちないようにしたもの

仕組みは 4 つ:

1. **モデルに足す**: 読み書きする値として持つ。
2. **読んだまま持つ**: 中身を見ずに読んだ XML や部品のまま持ち、保存で書き戻す。
   - 部品: `kept_parts.rs` の `KeptRelationships`（ブック・シート・テーブル・パッケージのリレーションごと）。
   - 描画のアンカー: `KeptAnchor`。
   - 要素や属性: `other_attributes` と、要素を読んだまま持つもの。
3. **読んだまま持ち、モデルも読む**: モデルにも読み、モデルが読んだときのままなら読んだものを、モデルで変えたらモデルから書く。
   モデルの `Debug` の表現のハッシュで比べ、読んだものは `Debug` に出さない。
   - グラフ（`Chart::kept_unchanged`）、図形と画像のアンカー（`TwoCellAnchor` / `OneCellAnchor::kept_unchanged`。`KeptShape`）、
     テーマ（`Theme::kept_unchanged`）、メモの枠（VML の `Shape::kept_unchanged`。位置はハッシュから除き、書くときにモデルの位置に置き換える）。
   - 行・列の挿入と削除では、モデルと読んだものの両方を動かし、変えていない扱いのままにする。
   - 画像のアンカーの画像の部品はモデルが書く（同じ部品を 2 つ持たない）。
4. **本家の修正を取り込む**。
### 3.1 部品や描画ごと落ちていたもの（読んだまま持つ）

- グラフ（chartEx も）、ピボットテーブルとキャッシュ、外部リンク、スライサー、タイムライン、フォームコントロールのプロパティ、カスタム XML
- ActiveX コントロール（`xl/activeX`）。以前はシートが残ったコントロールの `r:id` を指したまま部品だけが落ちていて、Excel が修復を求めた。コントロールの代わりの絵（`controlPr` の `r:id`）も残る
- SmartArt（`xl/diagrams`）
- VBA プロジェクトの署名（`vbaProjectSignature*.bin`）
- 色のパレット（`styles.xml` の `colors`）、ブックのテーブルスタイル
- 外部データの接続（`xl/connections.xml`）とクエリ（`xl/queryTables`。テーブルとシートから）、データモデル（Power Pivot）
- パッケージが参照する部品（`_rels/.rels`）: 秘密度ラベル（`docMetadata/LabelInfo.xml`）、リボンのカスタマイズ（`customUI`）、作業ウィンドウのアドインなど。
  ブックとそのプロパティ・サムネイル・デジタル署名（書いた中身と合わない）は除く
- SmartArt の描画のキャッシュ（`xl/diagrams/drawing*.xml`）。データの `dsp:dataModelExt` の `relId` が指すので、描画のリレーションをその id（`rIdDgm…`）で書く
- ほかのアプリの文書の埋め込み（Word・PowerPoint・マクロ付きブック）。以前は `.xlsx` と `.bin` しか書かず、`oleObject` の `r:id` が画像を指していた

### 3.2 一部が落ちていたもの

- **セル・数式**: 配列数式とスピル、セルの画像（リッチ値）、データテーブルの `ref`、数式の `ca`、計算の設定
- **書式**:
  - 縮小して全体を表示、文字の方向（`readingOrder`）、均等割り付けの最後の行
  - 差分書式の「オフ」（`<b val="0"/>`）
  - `quotePrefix`、`pivotButton`
  - セルのスタイル（`xfId`。保存で常に標準になっていた）、セルのスタイルの `hidden` / `customBuiltin`
  - 日本語版の組み込みの表示形式（ID だけのもの）
- **テーブル**:
  - 名前付きスタイルのオプション（縞模様が消えていた）
  - 見出し行の数（見出しのないテーブルに見出しが付いた）
  - 書式の番号、集計行の数式
- **シート**:
  - 表示の設定、ウィンドウ枠（列だけを固定すると `activePane="TopRight"` という Excel にない値を書いていた）
  - 印刷の枠線・行列番号
  - 先頭ページと偶数ページのヘッダー・フッター
  - オートフィルターのボタンを隠した列
  - フィルターの日付のグループ・色・アイコン
  - 行と列のグループ
  - ふりがな（セルの `rPh`、シートと文字列の `phoneticPr` の種類）
- **ブック**: シート見出し・スクロールバーの表示、シートの名前の変更に付いてくる参照
- **入力規則**: 2 つの数式の条件、停止・注意・情報の種類、IME の設定。メッセージを文字参照（`&#25968;`）で書いたものが二重にエスケープされていた
- **描画**:
  - 空の `<a:ln w="0"/>` と空の `<a:prstGeom/>`（どちらも Excel で開けなくなっていた）
  - ワードアートの文字の変形、フリーフォーム
  - 印刷しない図形、代替テキスト
  - 線の始点の矢印、図形の文字の下線・上付き
  - 反射などの効果、画像とコネクタのマクロ
  - 図形・画像のモデルにないもの（色の名前 `a:prstClr`、文字の段落の 2〜9 段目 `a:lvl2pPr`〜、Excel 2010 の互換用の隠した塗り・線・効果
    `a14:hiddenFill` など、グラデーション）。アンカーを読んだまま書く（3 の仕組み）。フォームコントロールの図形（`a14:compatExt`）はモデルから書く
- **コメント**: スレッド形式のコメントの作者・返信・解決済み。メモの枠の書式（塗り・線の矢印・文字の配置）と、Excel のテキストボックス以外の形の定義（`v:shapetype`）
- **テーマ**: 新しく描く図形の既定のスタイル（`objectDefaults`）、グラデーションの形（`fillToRect`・`path`）、拡張。読んだまま書く（3 の仕組み）
- **ブック（`workbook.xml`）**: `fileVersion`、書き込みのパスワードと読み取り専用の推奨（`fileSharing`）、`workbookPr` の属性（`showObjects`・`backupFile`・`codeName` など）、
  ユーザー設定のビュー（`customWorkbookViews`）、`oleSize`、スマートタグ、Web 公開、`fileRecoveryPr`
- **シート**: 計算の設定（`sheetCalcPr`）、範囲の編集の許可（`protectedRanges`）、シナリオ、並べ替え（`sortState`）、統合（`dataConsolidate`）、
  ユーザー設定のビュー（`customSheetViews`。ビューごとのプリンターの設定も）、エラーチェックを無視するセル（`ignoredErrors`）、スマートタグ、
  ユーザー設定のプロパティ（`customProperties`）、ウォッチ、ヘッダー・フッターの画像（`legacyDrawingHF` とその VML）、背景の画像（`picture`）、Web 公開、`sheetPr` の属性と `outlinePr`（グループの集計行の位置）、
  マクロのないブックのシートの `codeName`
- **名前の定義**: `function`・`vbProcedure`・`comment` などの属性（マクロを名前として登録したもの）
- **ハイパーリンク**: `display`
- **行・列の挿入と削除、シートのコピー**: Excel と同じように参照を付け替える（`Workbook::edit_lines` / `copy_sheet`）

詳しい中身と理由は xljs の `docs/design/pdf-design.md` 7 章（フォークで直したもの）と `docs/design/save-fidelity.md` にある。

## 4. まだ足りないもの（保存で落ちるもの）

2026-11-22 に、xljs・RAS・このクレートのテスト用のブック 107 冊を開いて保存し、部品と要素と属性を比べた結果（2026-11-23 に直したものを除いた）。比べ方は 5 章。
優先度は、起きやすさと、気づかずに業務のブックを壊す度合いで付けた。

| 優先 | 落ちるもの | 中身 |
|---|---|---|
| 低 | フォームコントロールの VML | `vmlDrawing*.vml`、シートの `legacyDrawing`、描画の `a14:compatExt`。Excel 2010 以降はシートの `controls`・`ctrlProps`・描画から読むので元どおり動く。Excel 2007 以前と LibreOffice では見えない |
| — | `.xlsb` | 読めない。RAS の `crates/ras-ovba` が VBA の部分だけを読む |

読んだまま持つシートの要素は、行・列の挿入と削除で範囲とセルが動く（`KeptRelationships::shift_elements`。`edit_lines` でも `insert_new_row` などでも）:
範囲の編集の許可・エラーチェックを無視するセル・ウォッチ・スマートタグ（範囲がすべて消えたら要素ごと消す）、並べ替え、シナリオ、
ユーザー設定のビューのウィンドウ枠・選択範囲・改ページ（消えた行・列の改ページは消し、数を数え直す）・フィルター（範囲と、列の番号 `colId`。
消えた列のフィルターは消す）。統合（`dataConsolidate`）の参照元はほかのシートのことが多いので動かさない。

### 4.1 落ちても害のないもの

比較には出るが、直さなくてよい。

- `xl/calcChain.xml`（Excel が作り直す）、サムネイル（`docProps/thumbnail.*`）、空の描画。
- 既定値と同じ属性（`b val="1"`→`<b/>`、`quotePrefix="0"`、`headerRowCount="1"`、LibreOffice の `hidden="false"` など）。
- 空の要素（`dxfs`、`numFmts`、`definedNames`、`evenHeader` など）。
- 接頭辞の違い（`x:sheet`→`sheet`、`wsDr`→`xdr:wsDr`）、部品の名前の番号（`theme.xml`→`theme1.xml`）。
- インライン文字列（`<is>`）が共有文字列になること。
- 共同編集の ID（`xr:uid`、`a16:creationId`）、コメントの `shapeId`。
- 中身のないユーザー設定のプロパティ（`docProps/custom.xml`）、`date1904="false"`。
- Id も Target もないリレーション（Google スプレッドシートがピボットキャッシュに書く `externalLinkPath`）。
- どこからも参照されない部品（参照のない埋め込み・テーマの `theme.xml` の名前の違いなど）。

## 5. 調べ方

- 部品・要素・属性の比較は、xljs の `tools/excel-verify/partdiff.py`（`py -I partdiff.py 元.xlsx 保存後.xlsx`）。手元のブックをまとめて調べるなら、`examples/roundtrip.rs` で開いて保存してから（`cargo run --release --example roundtrip -- 元1.xlsx 保存後1.xlsx 元2.xlsx 保存後2.xlsx ...`）、1 冊ずつ比べる。
- Excel で開けるか、Excel から見て同じかは、Windows の実機の Excel で xljs の `tools/excel-verify/features.ps1` などを使う（xljs の `docs/design/excel-verify.md`）。
- 直したものには、元に戻ったら分かるテストを付ける:
  - `tests/excel_compat.rs`: Excel で開けるか、描画。
  - `tests/excel_features.rs`: Excel が作ったブックの機能、ActiveX、SmartArt。
  - `tests/format_fidelity.rs`: モデルから読めるもの、属性。
  - `tests/vba_project.rs`: VBA と署名。
  - `tests/kept_as_read.rs`: 読んだまま持つもの（接続とクエリ、パッケージの部品、ユーザー設定のビューとシート・ブックの要素、ヘッダー・フッターの画像、図形・画像・テーマ・メモの枠、埋め込み）。
