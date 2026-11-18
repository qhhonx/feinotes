# Website demo documents

The two public website scenes are kept here as editable native DocxXML and readable Markdown, in Chinese and English.

- `notes/zh/coastal-note.xml`, `notes/en/coastal-note.xml`: the seaside note, shown in the **Notes** theme for multi-device sync. The final sentence is complete; the before frame ends at “如果还早，” / “If there’s time,”. Blank paragraphs were added for the website composition.
- `feishu/zh/book-fair.xml`, `feishu/en/book-fair.xml`: the book-fair plan, shown in the **Feishu / Lark Docs** theme for collaboration. The callout, colored statuses, table and checklist retain the public site's content.

Each `notes/<language>` directory is a self-contained document library. Keep `samples/image.png` alongside the XML. It is the existing public lighthouse source capture; the XML's crop selects the lighthouse photograph without modifying the source bytes. Markdown is a text companion; DocxXML preserves the crop, highlights, table widths and colors.

The documents were reconstructed from the site's existing synthetic fixtures and public captures, then rendered with the current application code. The website uses real native app captures. The continuation animation and collaborator fixtures are illustrative; they are not evidence of a live multi-account CloudKit transfer.

See `../ASSETS.md` for capture provenance. No personal note library is included.
