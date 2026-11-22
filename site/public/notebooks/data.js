// Sample notebooks: who they belong to (the home page and /notebooks/<id>/).
// Adding a person: a library bundle in /demo/libraries/<id>.qmdb
// (web/README.md), home.webp and note.webp in /notebooks/<id>/, an entry here,
// and a page copied from an existing one.
"use strict";

const NOTEBOOKS = window.NOTEBOOKS = {
  "han-meimei": {
    name: "Han Meimei",
    stats: { notes: 53, pictures: 118, words: "52k" },
    role: {
      "zh-Hans": "科学史博士生", "zh-Hant": "科學史博士生", en: "PhD candidate, history of science",
      ja: "科学史の博士課程学生", fr: "Doctorante en histoire des sciences" },
    blurb: {
      "zh-Hans": "博士二年级，论文写 1543–1900 年间解剖图像如何成为证据。读书笔记、档案馆走访、速写本和日记都在这里：每篇都有表格、公式和她自己画的图版。",
      "zh-Hant": "博士二年級，論文寫 1543–1900 年間解剖圖像如何成為證據。讀書筆記、檔案館走訪、速寫本和日記都在這裡：每篇都有表格、公式和她自己畫的圖版。",
      en: "A second-year PhD student writing about how the anatomical image became evidence, 1543–1900. Reading notes, archive visits, a sketchbook and a journal — with tables, formulas and plates on almost every page.",
      ja: "博士課程2年目。1543〜1900年に解剖図がどのように証拠となったかを研究中。読書ノート、アーカイブ訪問、スケッチブック、日記。ほぼすべてのページに表、数式、図版があります。",
      fr: "Doctorante en deuxième année, elle étudie comment l’image anatomique est devenue une preuve (1543–1900). Notes de lecture, visites d’archives, carnet de croquis et journal — avec tableaux, formules et planches presque à chaque page." },
    quote: {"zh-Hans": "“论文在写出来之前，先在笔记本里吵完。”", "zh-Hant": "「論文在寫出來之前，先在筆記本裡吵完。」", "en": "“The thesis gets argued in the notebook before it gets written.”", "ja": "「論文は、書く前にノートの中で議論し尽くす。」", "fr": "« La thèse se dispute dans le carnet avant de s’écrire. »"},
    uses: {"zh-Hans": ["年表和标本清单用表格，血流的计算用 KaTeX 公式。", "图版和档案照片就放在它们支撑的论点旁边。", "每条论文线索一个文件夹，5 万多词全文搜索，一秒找到。"], "zh-Hant": ["年表和標本清單用表格，血流的計算用 KaTeX 公式。", "圖版和檔案照片就放在它們支撐的論點旁邊。", "每條論文線索一個資料夾，5 萬多詞全文搜尋，一秒找到。"], "en": ["Tables for chronologies and specimen lists; KaTeX for the arithmetic of blood.", "Plates and archive photos sit right beside the argument they support.", "A folder per thread of the thesis; full-text search across 52k words."], "ja": ["年表や標本リストは表で、血流の計算は KaTeX の数式で。", "図版やアーカイブ写真は、それが支える議論のすぐ隣に。", "論文の糸ごとにフォルダを分け、5万語以上を全文検索。"], "fr": ["Des tableaux pour les chronologies et les spécimens ; KaTeX pour l’arithmétique du sang.", "Planches et photos d’archives à côté de l’argument qu’elles soutiennent.", "Un dossier par fil de la thèse ; recherche plein texte sur 52 000 mots."]},
    folders: ["Thesis — Seeing Inside", "Heart", "Maps & Cities", "Field Notes", "Journal"],
  },
};
