# Pane Drag & Split Prototype

> **Throwaway prototype. Do not ship or import into production.**

Question: which pointer interaction and visual feedback should govern local-tab reorder, moving a tab between pane groups, creating edge splits, resizing, cancelling, and collapsing an emptied source group?

## Assumptions

- One worktree only.
- Individual `WorkspaceTab` values move; pane groups do not.
- Binary split topology with local tab bars.
- A source group collapses when its last tab leaves, except for the sole root group.
- State is in memory and resets on reload.
- Visual language follows current Tiller colors, compact tab sizing, and 6-point dividers.

## Approved decision

**Edge Preview** is the canonical feedback model. It keeps the interaction direct: the pointer remains over the place being changed, and the overlay previews the exact resulting region without introducing a secondary control.

Interaction contract:

1. A pressed tab becomes a drag only after a 4-point movement threshold.
2. The drag ghost tracks the pointer 1:1 and preserves the original grab offset.
3. The destination tab bar is always a center target and shows the exact insertion position.
4. The nearest pane edge becomes a split target inside a contextual 22% edge band; its overlay previews the new 50/50 region.
5. Dropping in the center reorders locally or moves the tab atomically into that group.
6. Dropping on an edge creates one new group and split while preserving the target group's identity.
7. Moving the last tab out of a non-root source group collapses that group and its parent split in the same transaction.
8. Releasing without a legal target, pressing Escape, pointer cancellation, or loss of window focus leaves the layout unchanged.
9. Divider movement tracks directly and commits the preferred normalized fraction on release.
10. Reduced Motion removes topology animation; Increased Contrast and Reduced Transparency retain clear boundaries without relying on translucency.

## Compared alternatives

- `?variant=edge` — **approved** contextual half-pane preview at the target edge.
- `?variant=compass` — explicit center plus four-direction drop compass; rejected because it redirects the pointer away from the intended edge.
- `?variant=rails` — insertion rails at target boundaries; rejected because simultaneous rails add persistent visual noise during drag.

## Verification

- 7 transactional layout-model checks.
- 6 Chromium pointer-interaction checks.
- Zero browser console or page errors across all three variants.

## Run

From the repository root:

```bash
Scripts/run-pane-drag-prototype.sh
```

Open <http://localhost:4173>.
