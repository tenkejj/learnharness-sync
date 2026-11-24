# LearnHarness Sync desktop interface contract

Mode: Operate. A quiet, readable companion for connecting two personal computers.

The approved bitmap logo and monochrome identity are unchanged. The brief pins
near-black surfaces, minimal cognitive load and visible safety boundaries. The
connection between named computers is the organizing element, never a dashboard
of counters. Impeccable direction reference: seed:ce791bd9b3e54eca; the owner's
explicit visual and interaction brief takes precedence over concept selection.

## System

- One self-hosted family: Manrope variable, weights 450–650 for most UI. SIL OFL
  1.1 and attribution ship in `public/fonts`. No runtime font downloads.
- Main status 34px (30px on laptops), section titles 24px, device names 20–21px,
  body 16px with 1.6 line-height, secondary 14–15px, metadata 13px. No tiny
  technical values in the ordinary view. Text can wrap and scale.
- Canvas #111112; surface #19191b; raised #212124; borders #343438; primary
  #f3f3f4; secondary #b6b6be. White primary actions with dark text. State uses
  words and icons as well as surface contrast, without relying on color.
- Spacing follows 8/12/16/20/24/28/32/40px; 14px large panel radius, 9px buttons,
  46px ordinary targets and 50px main actions. One contextual primary action.
- Lucide icons, consistent 1.75 strokes on controls, 22–24px navigation/actions,
  32px device cards, 62–66px device connection diagram. Icons supplement labels.
- Navigation: Home, Devices, History. Settings is a labeled gear outside the
  main destinations. No permanent Setup, Backups or Conflicts destinations.
- Desktop sidebar becomes a compact top navigation at 800px. At 600px navigation
  takes its own row. Main content scrolls independently; the safety footer stays
  visible. Minimum native window 720 × 600; browser checks extend to 420px.

## Interactions and truth

Home distinguishes no devices, connected, offline, running test, pending result,
verified current data and preserved conflict. Connectivity is never a substitute
for a remote receipt. Both sent and received heads must match the current local
head before saying test data is up to date.

Add Device opens discovery and a bounded pairing window in one action. Invitations
appear only after asking for help finding a computer. All eight verification
groups remain visible, four per row, without truncation. Explicit approval on both
computers remains essential. Completion closes the dialog and returns to Home.
Closing it cancels unapproved requests. A previous connection may finish closing
before the engine starts another pairing attempt; users need not time a retry.

A sync test automatically prepares isolated fixtures, publishes an immutable
revision and waits for an authenticated durable remote-head receipt. Native and
React guards prevent duplicate tests. A timeout stays pending; late matching
receipts can finish it. Divergence preserves both branches and blocks tests.

History combines human-readable activity, the latest test, backup restoration and
preserved versions. Expanding a preserved version reads its verified invented
note. Technical identifiers need a further explicit details expansion. Restore
retains the exact existing confirmation phrase and backs up current test data.

Settings contains device name, reduced motion, About, Advanced diagnostics and
opt-in Developer Mode for manual invented notes. No real-data operation is offered.
A persistent footer says real LearnHarness synchronization is not enabled and
that both applications must remain open.

## Accessibility and motion

Semantic controls, native modal dialogs with explicit Tab containment, initial
heading focus, Escape dismissal and focus restoration. Dialog headers remain
visible while their bodies scroll. White 2px keyboard focus ring; 44px+ ordinary
controls; labels on all fields and icon buttons; polite status updates and alert
errors. Sensitive technical strings wrap; verification strings never truncate.

160ms hover transitions, 180ms short navigation reveal, 200ms dialog entrance.
Small opacity/vertical movement only; no decoration loops. Spinners indicate an
actual in-flight operation. Both the OS reduced-motion preference and the in-app
preference remove animation and transitions.
