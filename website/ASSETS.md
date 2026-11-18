# Website scene captures — 2026-11-18

Editable source documents and attachments are in `documents/`. The seaside note
uses the Notes theme for sync; the book-fair plan uses Feishu for collaboration.
The site's existing coast background, hero, stage proportions and theme captions
are retained. The phone has no device-name caption.

Mac captures were rendered from source revision
`6983caaf9994ce640a9b926dbe6d1d81cf084206` with the repository's offscreen devtools
harness. Notes frames use a 920 × 571 point window. Feishu uses 1060 × 658 points,
resized to the existing 1840 × 1142 website asset dimensions. Presence is rendered
by the current native app using two synthetic collaborators in an isolated QA
snapshot; the website retains these native avatars, share button and selections.

Phone PNGs are unchanged iPhone simulator exports from the integrated application,
including the local `src/theme.rs` fix that selects the Apple system font on iOS
instead of falling back from unavailable Noto Sans and losing bold weight.
They use the same synthetic seaside content and existing lighthouse source. The CSS
phone frame and looping text reveal illustrate continuing the note on
one device and seeing the same words appear on another. Reduced-motion users see
the completed note immediately. Each 3.6-second reveal is followed by a 4-second
hold; playback pauses outside the viewport, in a hidden tab, or with the pause
button. Theme captions sit below the Mac capture without covering its document.

No personal library, account or live collaboration data is included. These captures
and illustrative timings are not a recording or measurement of a live CloudKit transfer.

The collaboration scene uses two staggered reveals of the native callout and table
text. Moving participant labels are CSS crops of that same unmodified capture;
only the caret and temporary masks are drawn in CSS. Both participants edit during
an overlapping interval, then the finished document holds for four seconds before
repeating. Both scene controllers share pause, visibility and reduced-motion rules.
`node website/test-theme-gallery.cjs` checks their independent lifecycle and timing.
