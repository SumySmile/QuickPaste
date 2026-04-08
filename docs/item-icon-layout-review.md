# Item Icon Layout Review

## Scope

This review focuses on icon placement for the item card and the bottom action row.

Confirmed interaction granularity:
- `copy` targets `content` only
- `edit` changes `title` only
- `delete` removes the whole item/card

Reference sources:
- `E:\CodexWorkSpace\MyQuickPaste\docs\product-requirements.md`
- `E:\CodexWorkSpace\MyQuickPasteSlint\ui\app-window.slint`

## Current UI Snapshot

Current item card behavior in `ui/app-window.slint`:
- card body click copies content: lines 344-349
- copy button sits at top-right: lines 412-418
- edit button sits at bottom-right in normal mode: lines 435-442
- delete button replaces edit at bottom-right in delete mode: lines 421-432

Current bottom action row:
- delete-mode toggle at bottom-right: lines 861-867
- add-item button next to it: lines 870-872

Current PRD direction in `product-requirements.md`:
- copy affects content only: lines 56-57, 296
- edit affects title only: lines 58, 284-285, 297
- delete removes the whole item card: lines 59, 298
- copy is always visible: line 287
- edit is always visible but should be visually weaker than copy: line 288
- delete appears in delete mode: lines 290-291

## Findings

### 1. High: the current edit icon placement implies the wrong scope

The current edit icon is placed in the lower-right action slot, visually detached from the title and closer to the preview/content area.

Why this is a problem:
- the user will naturally read it as "edit this card" or "edit this content"
- that conflicts with the confirmed rule that edit changes `title` only

Evidence:
- `ItemCard` title and preview live in the text block at lines 381-409
- edit sits separately at lines 435-442

Conclusion:
- title-only edit must be visually tied to the title row, not treated as a general card action

### 2. High: putting copy, edit, and delete into one right-side stack will make delete mode crowded and semantically muddy

Delete affects the whole card.
Edit affects only the title.
Copy affects only the content.

These are three different scopes.
If all three live in one compact right-edge cluster, the UI implies they are peers with the same target.

Why this is a problem:
- `edit` and `delete` are not equivalent in scope
- `copy` is the primary action and should remain the easiest to hit
- delete mode becomes visually noisy if all three actions compete in one narrow area

Conclusion:
- keep `copy` and `delete` in a card-level action rail
- keep `edit` attached to the title row

### 3. Medium: copy needs stronger visual ownership of the content region

The current primary interaction is already content copy:
- clicking the whole card copies content
- clicking the copy icon also copies content

That is good for speed, but the icon arrangement should reinforce the same mental model.

Risk:
- if edit stays visually equal to copy, users may hesitate before clicking the card
- they may stop understanding which control is the "safe default"

Conclusion:
- copy should remain the strongest icon on the card
- edit should become a smaller, quieter title-affordance

### 4. Medium: delete mode should read as a temporary destructive layer, not as a third permanent action tier

The bottom action row currently enters delete mode from a trash icon.
This is acceptable if the card itself also makes the destructive state obvious.

Risk:
- if delete appears without enough separation from copy, users may mis-hit it
- if the card border changes but the action cluster stays dense, the card still feels overloaded

Conclusion:
- delete should appear only in delete mode
- delete should use danger styling
- delete should sit below copy with a clear vertical gap or tone separation

## Recommended Placement

## Normal mode

Recommended structure:

```text
+--------------------------------------------------+
| [grip]  Title text.................. [edit]      |
|         Content preview line 1............. [copy] |
|         Content preview line 2.................. |
+--------------------------------------------------+
```

Interpretation:
- `edit` belongs to the title row
- `copy` remains the strongest action and stays attached to the content row
- the whole card still copies content

Detailed guidance:
- `copy`
  - right side of the content row
  - normal icon-button size
  - strongest contrast among item actions
- `edit`
  - same top row as title
  - smaller visual weight than copy
  - ghost or low-contrast style
  - should feel like "rename title", not "open editor"

## Delete mode

Recommended structure:

```text
+--------------------------------------------------+
| [delete] Title text................. [edit]      |
|          Content preview line 1............ [copy] |
|          Content preview line 2.................. |
+--------------------------------------------------+
```

Interpretation:
- `edit` remains title-scoped
- `copy` remains content-scoped
- `delete` appears in the same fixed left rail as a separate destructive card-level action

Detailed guidance:
- `delete`
  - appears only in delete mode
  - replaces the normal grip in the fixed left rail
  - is vertically centered within the card
  - uses pink/red danger tint
  - should not move the card content horizontally when it appears

## Why this arrangement is better

### Scope mapping becomes spatially correct

- title row action: `edit`
- content row action: `copy`
- destructive card action in delete mode: `delete` in the fixed left rail

This matches the actual operation targets.

### Visual priority becomes clear

- primary: copy
- secondary: edit
- conditional destructive: delete

This matches the intended product flow.

### Delete mode stays readable

- `edit` stays attached to the title row
- `copy` stays attached to the content row
- `delete` moves to the fixed left rail instead of competing on the right edge
- the sense of mode change comes from color and danger styling, not from layout jump

## Bottom Action Row Recommendation

Current bottom row should remain simple:

```text
[manage-delete-toggle] [+]
```

Guidance:
- keep the add button last on the right
- keep the delete-mode toggle immediately to its left
- when delete mode is active, the toggle should visibly switch to a done/exit state
- do not add edit entry points into the bottom row, because edit is already available per card

Reason:
- add is a create action for the whole tab
- delete mode is a temporary global state
- edit is item-local and should remain on the card itself

## Implementation Guidance for the Slint Refactor

For `ItemCard`, the target internal layout should be:

```text
HorizontalLayout
+- fixed left rail
`- main content area
   +- top row: title + small edit
   `- preview block
      `- content-aligned primary copy action
```

Mode behavior:
- normal mode: left rail shows grip/reorder
- delete mode: left rail shows vertically centered delete

This removes the current ambiguity where the lower-right icon slot changes meaning between normal and delete mode.

## Recommendation

Use this icon model in the upcoming UI refactor:

1. Keep `copy` always visible and visually primary.
2. Move `edit` onto the title row and visually weaken it.
3. In delete mode, switch the fixed left rail from reorder to a vertically centered red delete action.
4. Keep the bottom-right row limited to delete-mode toggle plus add-item.

This is the cleanest arrangement for the confirmed scope rules and the least likely to mislead users.
