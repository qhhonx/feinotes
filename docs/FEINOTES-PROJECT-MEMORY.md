# Fei Notes project decisions

## Interface reuse — 2026-11-17

Prefer existing interface styles and interactions when adding or changing features.
Shared Documents must reuse the My Documents list, controls and menu primitives.
Discuss and align any proposed new visual style with the user before implementing it.

Shared Documents precedes My Documents in the sidebar and starts collapsed.
Both document sections use the same fold control, batch button and plus menu.
The Shared Documents plus menu contains Accept Invitation. Existing personal
folders in Shared Documents remain accessible.
Home also offers Accept Invitation. Both entry points open the same invitation
form. Open the shared overview through the Shared Documents tab on Home.
