> This is the preserved upstream Quiet.md Japanese README. Its download links refer to Quiet.md. For Fei Notes, use the maintained [English](README.md) or [Simplified Chinese](README.zh-CN.md) documentation.

<h1 align="center">Quiet.md</h1>

<p align="center">
  <b>速くて静かな、ローカルのノートアプリです。</b><br>
  Rust + GPUI でネイティブに描画します。ノートはあなたのコンピューターに、Feishu（Lark）の DocxXML 形式で保存されます。
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · 日本語
</p>

<p align="center">
  <a href="https://github.com/s1dashu/quiet-md/releases/latest"><img src="https://img.shields.io/github/v/release/s1dashu/quiet-md?label=release&color=2f6feb" alt="最新リリース"></a>
  <a href="https://github.com/s1dashu/quiet-md/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/s1dashu/quiet-md/ci.yml?branch=main&label=CI" alt="CI のステータス"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS、Windows、Linux">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-dea584?logo=rust" alt="Rust と GPUI で構築">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/s1dashu/quiet-md?color=green" alt="MIT ライセンス"></a>
  <img src="https://img.shields.io/badge/status-beta-orange" alt="ベータ版">
</p>

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.zip"><img src="docs/images/download-macos.svg" height="56" alt="macOS 版をダウンロード"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Windows 版をダウンロード"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Linux 版をダウンロード"></a>
</p>

<p align="center">
  <a href="https://quiet.md/?lang=en"><b>Web サイト</b></a> ·
  <a href="#ダウンロード"><b>ダウンロード</b></a> ·
  <a href="#パフォーマンス">パフォーマンス</a> ·
  <a href="#機能">機能</a> ·
  <a href="CONTRIBUTING.md">コントリビュート</a>
</p>

<p align="center">
  <img src="docs/images/home.png" width="100%" alt="Quiet.md のホーム画面。左にドキュメントのツリー、右にすべてのノートがグリッドで並んでいます。">
</p>

> [!NOTE]
> Quiet.md は**ベータ版**で、急速に変化しています。バグを見つけたり、アイデアがあったりしたら、[issue を作成](https://github.com/s1dashu/quiet-md/issues)してください。プルリクエストを送ってもらえると、もっとうれしいです。

## Quiet.md を選ぶ理由

**⚡ 速い。ネイティブならではの速さです。** インターフェースはすべて、Rust と [GPUI](https://www.gpui.rs/) によって GPU で描画されます。WebView も Electron も使っていません。100 万文字のドキュメントでも、入力、削除、スクロールはすべて P95 で 1 フレームあたり 6 ms 未満に収まります。

**🔒 ローカルファースト。ファイルはあなたのものです。** アカウントもクラウドもなく、デフォルトでは何もネットワークに送られません（オプションの更新チェックを除きます）。ライブラリはコンピューター上の普通のフォルダーで、ノートは 1 つずつ XML ファイルになっています。Git、iCloud、お好きな同期ツールで管理できます。Quiet.md を使うのをやめても、ファイルはそのまま残ります。

**📝 Feishu（Lark）の DocxXML に対応した、本格的なリッチテキストエディター。** ノートは Feishu（Lark）ドキュメントで使われている DocxXML 形式で保存されるので、コピー＆ペースト、インポート、エクスポートでも構造が保たれます。見出し、ToDo、表、段組み、コールアウト、コード、数式、画像、添付ファイルなど、ドキュメントを書くときに使うブロックがすべて揃っています。

## ダウンロード

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.zip"><img src="docs/images/download-macos.svg" height="56" alt="macOS 版をダウンロード"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Windows 版をダウンロード"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Linux 版をダウンロード"></a>
</p>

| プラットフォーム | パッケージ | 要件 |
|---|---|---|
| **macOS**（Apple シリコン） | [quiet-md-macos-arm64.zip](https://download.quiet.md/latest/quiet-md-macos-arm64.zip) | macOS 13 以降 |
| **Windows**（x64） | [quiet-md-windows-x64-setup.exe](https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe) | Windows 10 バージョン 2004 以降 |
| **Linux**（x86_64、試験的） | [quiet-md-x86_64.AppImage](https://download.quiet.md/latest/quiet-md-x86_64.AppImage) | glibc 2.35 以降（Ubuntu 22.04 以降） |
| **macOS**（Intel） | [quiet-md-macos-x86_64.zip](https://download.quiet.md/latest/quiet-md-macos-x86_64.zip) | macOS 13 以降 |

- **macOS**: 展開して、`quiet.md.app` をアプリケーションフォルダーにドラッグします。アプリは Apple によって署名・公証されています。
- **Windows**: インストーラーを実行します。デフォルトでは現在のユーザーだけにインストールされ、アンインストールしてもノートが削除されることはありません。初回の起動時に SmartScreen が表示された場合は、**詳細情報 → 実行** をクリックしてください。
- **Linux**（試験的）: `chmod +x quiet-md-*.AppImage` を実行してから起動します。FUSE がない場合は `--appimage-extract-and-run` を付けてください。更新は手動です。現在の対話的なテストは Wayland 上の Omarchy のみです。詳しくは [Linux の説明](CONTRIBUTING.md#linux-experimental)を参照してください。

[Quiet.md](https://quiet.md/?lang=en) からもダウンロードできます。すべてのリリースとその SHA-256 チェックサムは [GitHub Releases](https://github.com/s1dashu/quiet-md/releases) にあります。

初回の起動時に、Quiet.md は空のライブラリに **Getting started** ガイドを入れます。主な機能を 10 分で紹介するツアーです。既存のノートが上書きされることはありません。

インターフェースは英語と簡体字中国語で利用できます。

## パフォーマンス

**100 万文字**、549 章の混在したドキュメントで、100 回連続して操作したときのフレームの処理時間：

| 操作 | P50 | P95 |
|---|---:|---:|
| ⌨️ 入力 | **5.06 ms** | 5.78 ms |
| ⌫ Backspace | **4.99 ms** | 5.55 ms |
| 🖱️ スクロール | **2.08 ms** | 4.14 ms |

120 Hz のディスプレイでは、1 フレームに 8.3 ms が与えられます。100 万文字のドキュメントの P95 でも、まだ余裕があります。

| | |
|---|---|
| macOS 版のダウンロード | 約 18 MB |
| Windows 版のインストーラー | 約 14 MB |
| ランタイム | WebView なし、Electron なし、バックグラウンドサービスなし |

<sub>Apple M5 Pro / 48 GiB、macOS 26.5、リリースビルド。測定方法と再現用のスクリプト：[docs/PERFORMANCE.md](docs/PERFORMANCE.md)。</sub>

## 機能

- **編集**: 見出し、番号付きリストと箇条書き、ToDo、引用、シンタックスハイライト付きのコードブロック、LaTeX の数式、表（セルの結合に対応）、コールアウト、段組み、画像、添付ファイル、カバー画像。`#`、`-`、`[]`、`>` などの Markdown のショートカットと、`/` のスラッシュメニュー。
- **整理**: ドキュメントのツリー、ピン留め、お気に入り、全文検索、ゴミ箱。
- **インポートとエクスポート**: Markdown と Feishu（Lark）の DocxXML のインポート、Markdown と PDF へのエクスポート、または直接の印刷。Feishu や Web ページから貼り付けても、構造が保たれます。
- **ショートカット**: `⌘ /`（Windows では `Ctrl+/`）を押すと、すべて表示されます。どれも割り当てを変更できます。Windows では ⌘ の代わりに Ctrl を、⌥ の代わりに Alt を使います。アプリと Getting started のガイドには、お使いのシステムのキーが表示されます。
- **テーマ**: Craft、Feishu、Notion、Obsidian の 4 つのスタイルを内蔵し、それぞれにダークのバリエーションがあります。CSS で自分のテーマを書くこともできます。[docs/THEMES.md](docs/THEMES.md) をご覧ください。

## コントリビューター

Quiet.md の開発に協力してくださったすべての方に感謝します。issue とプルリクエストを歓迎します。ビルド、テスト、コードの規約については [CONTRIBUTING.md](CONTRIBUTING.md) をご覧ください。

[![コントリビューター](https://raw.githubusercontent.com/s1dashu/quiet-md/contributors-assets/docs/images/contributors.svg?v=1)](https://github.com/s1dashu/quiet-md/graphs/contributors)

## 最近の更新

- **0.1.9**（11 月 8 日）: macOS の Cmd+Q で終了できない問題、Windows のウィンドウ操作ボタンが反応しない問題、背景色付きテキストの選択範囲が見えない問題を修正。
- **0.1.8**（11 月 9 日）: ローカル Markdown の直接編集、`.md` を保持するフォルダーのインポート、階層フォルダーとドラッグ操作・取り消しに対応。画像とプレビューのメモリ使用量を削減し、表の列幅変更とメニューを改善。初回起動時は対応するシステム言語を選択。実験的な Linux AppImage パッケージを追加。
- **0.1.7**（11 月 6 日）: 複数ウインドウの保存、競合からの復元、ファイルへの書き込みを改善しました。エクスポート時に読み取れるファイルの範囲を制限し、Markdown のインライン添付ファイルを保持します。Windows の小狼毫（Weasel）入力時に文字が削除される問題を修正しました。
- **0.1.6**（11 月 6 日）: ウインドウ切り替えと入力を改善し、表の最大幅を広げました。連続入力中も定期的に保存します。macOS では Option+Space でクイックノートを開けます。Windows では画像の貼り付け、PDF エクスポート、印刷に対応し、単語削除の動作を修正しました。
- **0.1.5**（11 月 5 日）: アプリの名前が Quiet.md になりました。ライブラリとノートは元の場所のままです。
  - Home から複数のドキュメントをまとめて管理できます。まとめて複製、移動、削除できます。
  - ゴミ箱を 1 回の操作で空にできます。矢印キーで挿入メニューのアイコンの間を移動できます。すべてのドキュメントを削除しても、空のドキュメントが作られなくなりました。
  - アプリ内で更新した後、新しいバージョンでの変更点がサイドバーに 1 回表示されます。
  - Windows: ショートカットと内蔵のガイドが Ctrl / Alt の表記になり、起動に失敗したときは理由を説明してログを残すようになりました。
- **0.1.4**（11 月 5 日）:
  - アプリ内での更新: 新しいバージョンが出ると、サイドバーの上部に **Update** ボタンが表示されます。ダウンロードは署名で検証され、再起動するとインストールが完了します。開いているドキュメントは先に保存されます。自動チェックは **Settings › General** でオフにできます。チェックはバージョンのマニフェストを取得するだけで、データは何も送信しません。バージョン 0.1.3 以前では、0.1.4 を一度だけ手動でダウンロードする必要があります。
  - ダークモード: **Settings › Appearance** で System、Light、Dark を選べます。内蔵のテーマもカスタムのテーマも、すべてダークの見た目を持ちます。エクスポートと印刷はライトのままです。
- **0.1.3**（11 月 4 日）: macOS がダークモードのときも、サイドバーの文字が読めるようになりました。
- **0.1.2**（11 月 4 日）: macOS の標準のメニューとショートカット（Hide、Minimize、Zoom、Full Screen）に対応しました。最後のウインドウを閉じた後でも、Dock のアイコンをクリックするとウインドウが再び開きます。一部の中国語の文字が誤った字形で表示される問題を修正しました。
- **0.1.1**（11 月 3 日）: 初めての Windows x64 版のリリースです。ユーザーごとにインストールされ、アンインストールしてもノートは残ります。中国語と英語のインターフェースと、内蔵のガイドを磨き上げました。

各バージョンの詳しいリリースノートは [Releases](https://github.com/s1dashu/quiet-md/releases) にあります。

## ライセンス

コードは [MIT](LICENSE) のもとでライセンスされています。サードパーティーのコード、フォント、アセットは [THIRD_PARTY.md](THIRD_PARTY.md) に記載しています。

次のものは MIT ライセンスの対象外で、フォークでは置き換える必要があります。

- `assets/icons/` にある 67 個の Nucleo アイコンは、Nucleo の商用ライセンスのもとにあります。単独で取り出したり再配布したりしないでください。
- 「Quiet.md」という名前と、アプリのアイコン。
