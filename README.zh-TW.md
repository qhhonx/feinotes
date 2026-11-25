<h1 align="center">Quiet.md</h1>

<p align="center">
  <b>又快又安靜的筆記，就放在你自己的電腦上。</b><br>
  以 Rust + GPUI 原生繪製的筆記應用程式：真實的資料夾、輸入 Markdown 即自動排版，筆記裡還有一塊白板。免費且開源。
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · 繁體中文 · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a> · <a href="README.es.md">Español</a> · <a href="README.fr.md">Français</a>
</p>

<p align="center">
  <a href="https://github.com/s1dashu/quiet-md/releases/latest"><img src="https://img.shields.io/github/v/release/s1dashu/quiet-md?label=release&color=2f6feb" alt="最新版本"></a>
  <a href="https://github.com/s1dashu/quiet-md/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/s1dashu/quiet-md/ci.yml?branch=main&label=CI" alt="CI 狀態"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS、Windows 與 Linux">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-dea584?logo=rust" alt="以 Rust 與 GPUI 打造">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/s1dashu/quiet-md?color=green" alt="MIT 授權條款"></a>
  <img src="https://img.shields.io/badge/status-beta-orange" alt="Beta 測試版">
</p>

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="下載 macOS 版"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="下載 Windows 版"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="下載 Linux 版"></a>
</p>

<p align="center">
  <a href="https://quiet.md/?lang=zh-Hant"><b>官方網站</b></a> ·
  <a href="#下載"><b>下載</b></a> ·
  <a href="#主要功能">主要功能</a> ·
  <a href="#效能表現">效能表現</a> ·
  <a href="CONTRIBUTING.md">參與貢獻</a>
</p>

<p align="center">
  <img src="docs/images/readme-home.webp" width="100%" alt="像素風草坡背景上的 Quiet.md：左側側邊欄列出資料夾，首頁將每個資料夾顯示成一疊附圖的筆記卡片。">
</p>

> [!NOTE]
> Quiet.md 目前處於 **beta 測試階段**，版本迭代相當迅速。如果你遇到臭蟲或有任何想法，歡迎[回報 Issue](https://github.com/s1dashu/quiet-md/issues)，更歡迎直接送出 Pull Request。

## 為什麼選擇 Quiet.md

**📁 筆記就是一般檔案。** 不需要帳號，沒有雲端，預設不連網（可選的更新檢查除外；白板第一次用到中日韓手寫字型、或者你選擇可下載的文件字型時，會下載對應的字型檔）。側邊欄裡的每個資料夾都是磁碟上真實的資料夾，每篇筆記都是以標題命名的檔案：Markdown 為 `.md`，富文字文件為 `.xml`。用 iCloud、Git 或任何同步工具都可以；哪天不再使用 Quiet.md，檔案也還留在原處。

**⚡ 原生，而且快。** 整個介面由 Rust 與 [GPUI](https://www.gpui.rs/) 直接以 GPU 繪製，不是 Electron，也不是網頁套殼。在一篇 100 萬字的筆記裡打字，每格畫面約 5 ms；120 Hz 螢幕給每格畫面的時間是 8.3 ms。

**✍️ 寫字與畫圖，在同一個地方。** 輸入 `#`、`-`、`[]` 或 `>`，Markdown 就會自動排版。文字說不清楚時，直接在筆記裡放一塊白板。富文字筆記以 Feishu（Lark）DocxXML 格式儲存，從 Feishu 或網頁貼上都能保留結構。

## 主要功能

### 書寫

<p align="center">
  <img src="docs/images/readme-write.webp" width="100%" alt="在 Quiet.md 開啟的一篇長筆記，包含標題、行內公式與一張離子濃度表。">
</p>

標題、清單、待辦事項、引文、提示框、分欄、可合併儲存格的表格、具語法醒目提示的程式碼、LaTeX 數學公式、圖片、附件與封面。邊寫邊用 Markdown 快速輸入，其餘的都在 `/` 選單裡。提及另一篇筆記即可建立連結，對方會列出反向引用。

### 繪圖

<p align="center">
  <img src="docs/images/readme-board.webp" width="100%" alt="Quiet.md 裡的一塊白板：圖片與地圖排在一起，搭配手寫標註、方框與連接箭頭。">
</p>

白板移植自 Excalidraw：幾何圖形、始終相連的箭頭、手繪線條、文字、便利貼、畫框與圖片。可以放進筆記裡，也可以存成獨立的 `.excalidraw` 檔案，excalidraw.com 能直接開啟。用觸控式軌跡板雙指捏合即可縮放；一萬個圖形的白板依然流暢。

### 用真實的資料夾整理

首頁將每個資料夾顯示成一疊紙，裡面有什麼一目了然。把筆記拖到資料夾上稍候片刻，資料夾就會自動展開；資料夾裡還能再放資料夾，移動也能復原。筆記可以釘選、收藏；按 `⌘ J` 搜尋所有標題與內文，在筆記裡尋找與取代，也可以使用規則運算式。刪除的筆記會先移到垃圾桶。電腦上其他位置的 Markdown 檔案和資料夾，可以在「本機檔案」中開啟並就地編輯。

### 帶上舊筆記

<p align="center">
  <img src="docs/images/readme-import.webp" width="100%" alt="Quiet.md 首次啟動的畫面，提示匯入它找到的 Obsidian 筆記庫。">
</p>

第一次開啟時，Quiet.md 會找到你的 Obsidian、Logseq、iA Writer 與 Typora 筆記庫並複製進來，原始檔案不會被更動。Obsidian 的 `[[雙向連結]]` 照樣能用，點一下就會開啟它指向的筆記。之後也可以把 Markdown 檔案和資料夾拖到首頁，或直接拖進側邊欄的某個資料夾。筆記也能匯出為 Markdown 或 PDF，或直接列印。

### 還有更多

- **版本紀錄**：每篇筆記在儲存時都會留下快照（預設 20 版，最多 30 版）。點開任一版就能在原處看到改了什麼，一鍵還原，也能為它命名。
- **快速筆記**：在 macOS 上，全域快速鍵（預設 `⌥ Space`）會在任何應用程式上方開啟一個小筆記視窗。
- **外觀**：內建 Craft、Feishu、Notion 與 Obsidian 四套樣式，每套皆有深色版本，也可以用 CSS 自行撰寫（見 [docs/THEMES.md](docs/THEMES.md)）。介面字型與文件字型都能選擇。
- **快速鍵**：按下 `⌘ /` 檢視所有快速鍵，每一組都能重新設定。在 Windows 與 Linux 上，以 Ctrl 對應 ⌘、Alt 對應 ⌥。
- **語言**：英文、簡體中文、繁體中文、日文、韓文、西班牙文、法文、阿拉伯文與土耳其文。阿拉伯文會正確連寫，由右至左排列。
- **清理**：「設定 › 文件」能找出不再有任何筆記使用的圖片與檔案，確認後才會移出。
- **更新**：新版本在應用程式內安裝，安裝前會驗證簽章。自動檢查可以關閉。

## 下載

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="下載 macOS 版"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="下載 Windows 版"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="下載 Linux 版"></a>
</p>

| 平台 | 安裝套件 | 系統需求 |
|---|---|---|
| **macOS**（Apple Silicon） | [quiet-md-macos-arm64.dmg](https://download.quiet.md/latest/quiet-md-macos-arm64.dmg) | macOS 13 以上版本 |
| **Windows**（x64） | [quiet-md-windows-x64-setup.exe](https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe) | Windows 10 1809 以上版本 |
| **Linux**（x86_64，實驗性） | [quiet-md-x86_64.AppImage](https://download.quiet.md/latest/quiet-md-x86_64.AppImage) | glibc 2.35 以上版本（Ubuntu 22.04 及更新版本） |
| **macOS**（Intel） | [quiet-md-macos-x86_64.dmg](https://download.quiet.md/latest/quiet-md-macos-x86_64.dmg) | macOS 13 以上版本 |

- **macOS**：打開 `.dmg`，將 `quiet.md.app` 拖到旁邊的「應用程式」捷徑上，再從「應用程式」打開。應用程式與磁碟映像都已完成 Apple 簽章與公證。Releases 頁面上的 `.zip` 是同一個應用程式，供應用程式內更新下載。
- **Windows**：執行安裝程式，預設僅為目前使用者安裝，解除安裝時絕不會刪除你的筆記。若首次啟動出現 SmartScreen 提示，請點選**其他資訊 → 仍要執行**。
- **Linux**（實驗性）：執行 `chmod +x quiet-md-*.AppImage` 後即可啟動，若系統未安裝 FUSE 則加上 `--appimage-extract-and-run` 參數。AppImage 0.1.10 以上版本支援應用程式內更新，較舊的 Linux 版本需手動重新安裝一次，目前已在 Wayland 環境下的 Omarchy 完成互動實測，詳情請參閱 [Linux 說明](CONTRIBUTING.md#linux-experimental)。

你也可以前往 [Quiet.md](https://quiet.md/?lang=zh-Hant) 官方網站下載，所有發行版本與對應的 SHA-256 雜湊值皆公布於 [GitHub Releases](https://github.com/s1dashu/quiet-md/releases)。

首次啟動時，Quiet.md 會詢問介面語言、文件庫儲存位置與快速筆記快速鍵，並提供從 Obsidian、Logseq、iA Writer 或 Typora 複製匯入既有筆記的選項。全新的空白文件庫會附帶一份 **Getting started** 入門指南，帶你在十分鐘內逛完主要功能，且絕不會覆寫任何既有筆記。

## 效能表現

在包含 **100 萬字**與 549 個章節的混合內容文件中，連續執行 100 次操作的單格畫面處理耗時如下：

| 操作項目 | P50 | P95 |
|---|---:|---:|
| ⌨️ 打字輸入 | **5.06 ms** | 5.78 ms |
| ⌫ 倒退刪除 | **4.99 ms** | 5.55 ms |
| 🖱️ 畫面捲動 | **2.08 ms** | 4.14 ms |

120 Hz 螢幕每一格畫面的時間預算為 8.3 ms，即使面對百萬字長文的 P95 峰值，依然保有充裕餘裕。

| | |
|---|---|
| macOS 下載大小 | ~18 MB |
| Windows 安裝檔 | ~14 MB |
| 執行時期環境 | 無 WebView、無 Electron、無背景常駐服務 |

<sub>測試環境：Apple M5 Pro / 48 GiB，macOS 26.5，release 建置版本。測試方法與重現腳本請見 [docs/PERFORMANCE.md](docs/PERFORMANCE.md)。</sub>

## 參與貢獻

感謝每一位協助打造 Quiet.md 的貢獻者。歡迎提交 Issue 與 Pull Request，建置流程、測試方式與程式碼規範請參閱 [CONTRIBUTING.md](CONTRIBUTING.md)。

[![Contributors](https://raw.githubusercontent.com/s1dashu/quiet-md/contributors-assets/docs/images/contributors.svg?v=1)](https://github.com/s1dashu/quiet-md/graphs/contributors)

## 近期更新

- **0.1.14**（11 月 22 日）：尋找與取代支援規則運算式。Obsidian 的 `[[雙向連結]]` 可以點擊跳轉。新增土耳其文介面，阿拉伯文由右至左排版。可以選擇介面字型與文件字型，清理沒有筆記使用的圖片和檔案。文件很多時搜尋不再卡頓，批次刪除更快。新增「退格鍵回到標題」開關；Windows 與 Linux 上「複製為 Markdown」改為 Ctrl+Shift+M。
- **0.1.13**（11 月 17 日）：新增白板（移植自 Excalidraw），可作為筆記內嵌區塊或獨立 `.excalidraw` 檔案使用，容納 10,000 個圖形依然流暢；每篇筆記皆支援版本紀錄（… › 版本紀錄）；新增法文與阿拉伯文介面；支援複製為 Markdown、複製檔案路徑，並可於「設定」切換文件庫資料夾；貼上的網頁圖片會自動存入文件庫；降低多圖頁面的記憶體用量並提升側邊欄捲動順暢度。
- **0.1.12**（11 月 14 日）：可從 Finder 或側邊欄「本機檔案」將檔案直接拖入資料夾或插入列與列之間，整頁皆可作為拖放目標；重新設計圓角首頁卡片、新增「在 Finder 中開啟」選單、麵包屑導覽自「我的文件」起算、限制最小視窗寬度以保留筆記空間，並在全螢幕模式下改用實心側邊欄；修正於資料夾內建立文件時的卡頓問題。
- **0.1.11**（11 月 13 日）：文件庫改用真實磁碟資料夾並以標題命名檔案，舊版文件庫會自動遷移並保留備份；全新首次啟動流程可設定介面語言、文件庫位置與快速筆記快速鍵，並從 Obsidian 等 Markdown 軟體匯入筆記；新增日文、西班牙文與韓文介面；支援 Markdown 區塊數學公式與記憶上次閱讀位置；加快啟動速度並降低記憶體耗用。
- **0.1.10**（11 月 10 日）：可從「新增」選單選擇文件類型、寬表格支援水平捲動、保留 Markdown 表格欄位對齊設定，並在長輸入欄中維持游標可見。
- **0.1.9**（11 月 8 日）：修正 macOS 上 `Cmd+Q` 失效、Windows 視窗控制按鈕無回應，以及選取範圍被螢光筆底色遮蓋的問題。
- **0.1.8**（11 月 9 日）：支援原位編輯本機 Markdown 檔案、匯入資料夾時保留 `.md` 格式，並以拖曳與復原管理多層巢狀資料夾。降低圖片與預覽記憶體用量，改善表格欄寬調整與選單細節，首次啟動自動選用支援的系統語言，並提供實驗性 Linux AppImage 封裝檔。
- **0.1.7**（11 月 6 日）：強化多視窗儲存安全性、衝突復原與磁碟寫入保護；限制匯出檔案存取範圍；匯出 Markdown 時保留行內附件；修正 Windows 小狼毫輸入法刪除中文字元的問題。
- **0.1.6**（11 月 6 日）：提升視窗切換與打字順暢度、加寬表格、連續打字時定期自動儲存，並在 macOS 新增 `Option+Space` 全域快速筆記；Windows 版新增貼上圖片、匯出 PDF 與列印功能，並修正逐詞刪除行為。
- **0.1.5**（11 月 5 日）：應用程式正式定名為 Quiet.md，既有文件庫與筆記保留在原處。
  - 可在首頁一次選取多份文件，批次建立副本、移動或刪除。
  - 一鍵清空資源回收桶；方向鍵可瀏覽插入選單圖示；刪光所有文件後不再自動建立空白文件。
  - 應用程式內更新完成後，側邊欄會顯示一次新版本更新摘要。
  - Windows 版快速鍵與內建指南改顯示 Ctrl / Alt，啟動失敗時會顯示原因並留下紀錄檔。
- **0.1.4**（11 月 5 日）：
  - 應用程式內更新：有新版本時側邊欄頂部會出現**更新**按鈕，下載後自動驗證數位簽章，儲存開啟中的文件並重新啟動即可完成安裝；可於**設定 › 一般**關閉自動檢查，檢查時僅抓取版本清單而不傳送任何資料（0.1.3 以前版本需手動下載安裝一次 0.1.4）。
  - 深色模式：可於**設定 › 外觀**選擇跟隨系統、淺色或深色，內建與自訂主題皆具備深色外觀，匯出與列印則維持淺色。
- **0.1.3**（11 月 4 日）：修正 macOS 深色模式下側邊欄文字難以閱讀的問題。
- **0.1.2**（11 月 4 日）：補齊標準 macOS 選單與快速鍵（隱藏、最小化、縮放、全螢幕），關閉最後一個視窗後點選 Dock 圖示可重新開啟視窗，並修正部分中文字形渲染錯誤。
- **0.1.1**（11 月 3 日）：首個 Windows x64 發行版，預設安裝於目前使用者目錄，解除安裝時保留筆記，並打磨中英文介面與內建指南。

各版本完整更新紀錄請見 [Releases](https://github.com/s1dashu/quiet-md/releases)。

## 授權條款

程式碼以 [MIT](LICENSE) 授權條款釋出，第三方程式碼、字型與素材清單詳列於 [THIRD_PARTY.md](THIRD_PARTY.md)。

以下項目不包含在 MIT 授權範圍內，建立衍生分支（Fork）時請自行替換：

- `assets/icons/` 目錄下的 69 個和 `assets/editor-icons/` 目錄下的 24 個 Nucleo 圖示受商業 Nucleo 授權保護，請勿單獨抽離或重新散佈。
- 「Quiet.md」名稱與應用程式圖示。
