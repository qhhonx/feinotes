# Fei Notes project decisions

## Visual consistency

Reuse Fei Notes' existing visual styles and components. When bringing in
Quiet.md interfaces, reuse compatible upstream visual patterns and adapt
typography, spacing, colors, and controls to fit Fei Notes' screens and themes.
Choose layouts and entry points that fit each workflow. Discuss new visual
styles with the maintainers before implementing them.

## Shared documents

Shared Documents precedes My Documents in the sidebar and starts collapsed.
Both document sections use the same fold control, batch button and plus menu.
The Shared Documents plus menus create a shared document or folder. Existing
personal folders in Shared Documents remain accessible.
The shared sidebar and Home shared overview have separate invitation buttons
that open the same form. Sidebar creation targets its named section independently
of the active Home tab.
