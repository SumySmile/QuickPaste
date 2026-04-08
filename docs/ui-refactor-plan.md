# UI Refactor Plan

## 1. Goal

This document defines the recommended refactor order and a maintainable Slint layout skeleton for `MyQuickPasteSlint`.

The immediate goal is not visual polish.
The immediate goal is to replace the current fragile page layout with a stable popup-oriented structure that is easy to tune later.

## 2. Current Problems

### 2.1 Layout model is unstable

The current UI mixes layout containers with a large amount of absolute positioning.

Observed in:
- `ui/app-window.slint`
- fixed window size
- fixed settings page coordinates
- fixed confirm dialog coordinates
- hand-calculated tab and item canvas sizes

Impact:
- spacing changes ripple unpredictably
- DPI and text changes are risky
- later visual tuning becomes expensive
- drag/drop logic is tightly coupled to pixel constants

### 2.2 Interaction hierarchy does not match product intent

The current implementation behaves like a small management page:
- Home and Settings are split by `active_view`
- add/edit forms are inserted inline into the main content area

But the product direction is now:
- main popup stays focused on copy and quick management
- Settings is opened from the tray context menu
- the main popup should not contain a settings page

### 2.3 State refresh is too coarse

Most interactions end with a full `sync_window()` call and rebuilt models.

This is acceptable for v1 data volume, but it makes iterative UI work harder because:
- more UI state is reset than necessary
- hover and scroll continuity are more fragile
- layout experiments are harder to isolate

## 3. Refactor Strategy

### 3.1 Guiding principles

- Keep one main popup window for the primary workflow.
- Remove page-style Settings from the main popup.
- Use layout containers for page structure.
- Limit absolute positioning to true overlays and small icon alignment.
- Keep visual sizing values centralized and internally consistent.
- Separate "document flow" content from "overlay" content.

### 3.2 What to refactor first

#### P0. Remove Settings from the main popup structure

Target:
- Home becomes the only main view in `AppWindow`
- no `active_view == settings` content block inside the popup
- no gear button in the popup header

Reason:
- this removes the heaviest layout branch first
- it aligns implementation with the updated product direction

#### P1. Rebuild Home as a real layout-driven screen

Target:
- header
- tab strip
- content panel
- bottom action row

Reason:
- this is the main user-facing surface
- current instability mostly comes from this region

#### P2. Convert add/edit/delete UI to true overlays

Target:
- add tab
- rename tab
- add item
- rename item title
- confirm delete

Reason:
- these should not push main content around
- this simplifies the main content layout dramatically

#### P3. Reduce layout-coupled magic numbers

Target:
- tab slot size
- item card height
- item list gap
- drag/drop target calculation

Reason:
- visual tuning should not silently break behavior

#### P4. Optimize state synchronization only after structure is stable

Target:
- keep current `sync_window()` while layout refactor is underway
- optimize update granularity after the structure is correct

Reason:
- architecture cleanup is valuable, but not the first blocker
- layout instability is currently the bigger problem

## 4. Proposed Home Layout Skeleton

### 4.1 High-level tree

Recommended structure:

```text
AppWindow
+- RootSurface
   +- MainCard
   |  +- HeaderRow
   |  +- TabStripSection
   |  +- ContentPanel
   |  |  +- EmptyState or ItemList
   |  |  `- BottomActionRow
   |  `- StatusToastSlot
   `- OverlayLayer
      +- AddTabDialog
      +- RenameTabDialog
      +- AddItemDialog
      +- RenameItemTitleDialog
      `- ConfirmDeleteDialog
```

### 4.2 Home page sections

#### HeaderRow

Contains:
- title
- optional short subtitle
- close button

Does not contain:
- settings button
- settings/back page navigation

Recommended layout:
- `HorizontalLayout`
- left content stretches
- right side holds only close action

#### TabStripSection

Contains:
- horizontal tab list
- add-tab slot as the last slot

Recommended layout:
- outer vertical section with bottom divider
- inner horizontal scroll region if tabs overflow
- tab spacing and drop slot width must come from the same source

Important:
- if we keep drag reorder, visual slot width and reorder math must match exactly

#### ContentPanel

Contains:
- main list area
- empty state when no items
- bottom action row anchored within the panel

Recommended layout:
- `VerticalLayout`
- top area uses `vertical-stretch: 1`
- bottom action row uses fixed or minimum height without absolute positioning

Important:
- the action row should stay stable whether the list is empty or full
- add/edit overlays should not consume content height

#### EmptyState

Contains:
- title
- hint

Recommended layout:
- center it inside the content area using a wrapper with stretch
- do not use fixed `x/y`

#### ItemList

Recommended layout:
- `ScrollView`
- inside it, use a vertical flow structure rather than a manually sized canvas if possible
- keep item card spacing defined in one place

If drag reorder still needs positional math in v1:
- compute slot size from the same card height and gap constants used by the UI
- do not let Slint and Rust maintain separate spacing assumptions

#### BottomActionRow

Contains:
- delete mode toggle
- add item button

Recommended layout:
- right-aligned horizontal row
- always occupies a stable row at the bottom of `ContentPanel`

### 4.3 Overlay layer

Overlays should sit above the main card instead of being inserted into the content flow.

Recommended behavior:
- translucent scrim for destructive confirm dialogs
- compact centered card for add/rename dialogs
- overlay visibility controlled by `overlay`

Important:
- the main content height must not change when an overlay opens
- this is the main fix for the current crowded feeling

## 5. Component-Level Guidance

### 5.1 Keep

These pieces are still useful:
- `VStack`
- `HStack`
- `IconButton`
- `TextButton`
- `TextField`
- `TextAreaField`
- `HotkeyCaptureField`
- `ToggleSwitch`
- `TabChip`
- `ItemCard`

### 5.2 Change

#### `AppWindow`

Change from:
- multi-view popup with embedded settings page

Change to:
- single home popup plus overlay layer

#### `TabChip`

Needs:
- shared sizing constants with reorder math
- less reliance on invisible positional assumptions

#### `ItemCard`

Needs:
- layout-driven internal structure where practical
- action area sized consistently with list spacing
- a fixed left rail that shows reorder in normal mode and delete in delete mode
- a title row where `edit` stays visually attached to the title
- a content row where `copy` stays visually attached to the content

### 5.3 Avoid

Avoid introducing more:
- page branching inside the main popup
- `x/y` layout for whole sections
- one-off spacing fixes like `parent.width - 8px`

## 6. Implementation Order

### Phase 1. Structural cleanup

- remove Settings UI block from `ui/app-window.slint`
- remove header settings button
- keep tray-based settings callback path on the Rust side for later settings window work
- simplify popup to Home-only structure

### Phase 2. Home skeleton rewrite

- rebuild home content tree using `VerticalLayout` and `HorizontalLayout`
- keep existing colors and most existing components for now
- do not try to redesign visually during this phase

### Phase 3. Overlay migration

- move add/rename/delete forms into overlay layer
- use a title-only overlay for item rename instead of a full item-content editor
- preserve existing callbacks and draft fields where possible

### Phase 4. Sizing consistency

- define canonical tab slot width
- define canonical item card height and vertical gap
- align Rust reorder logic to the same values

### Phase 5. Interaction polish

- ensure copy closes popup
- ensure delete mode icon semantics match product direction
- use color highlight, not horizontal layout shift, to communicate delete/manage mode
- verify empty state and list state have stable action row placement

### Phase 6. Architecture follow-up

- reduce full-window resync where it is causing friction
- consider persistent models for tabs and items instead of rebuilding every time

## 7. Requirements That Need Confirmation

These points affected implementation shape and needed confirmation before code changes.

Confirmed decisions:
- Add tab, rename tab, add item, and item-title rename should use centered overlay dialogs.
- Settings is fully removed from the main popup and appears only from the tray context menu.
- Edit remains always visible on item cards, but with lower visual emphasis than copy.
- Edit changes title only, copy affects content only, and delete removes the whole item card.
- The popup window keeps a fixed outer size in v1 while the internal layout becomes flow-based.
- The item card keeps a fixed left rail: reorder in normal mode, vertically centered delete in delete mode.
- Delete/manage mode should feel different through highlight color and danger styling, not through horizontal card movement.

### 7.1 Add/Edit form presentation

Decision:
- add tab
- rename tab
- add item
- item-title rename

All four become centered overlays above Home.

### 7.2 Settings window relationship to Home

Decision:
- Settings is fully outside the main popup
- Home popup does not know about Settings layout anymore

### 7.3 Item card default actions

Decision:
- always visible: copy
- always visible: edit
- edit should be visually quieter than copy
- edit only changes the title
- copy only targets the content
- delete appears only in delete/manage mode
- delete removes the whole item card
- the left rail switches from reorder handle to delete
- `edit` stays immediately after the title text
- `copy` stays on the right side of the content row
- the card content area does not shift horizontally when modes change
- delete uses warning red styling by default in delete mode

### 7.4 Window sizing policy

Decision:
- keep a fixed popup size in v1
- but make internal layout flexible enough that content is not positioned by absolute coordinates

## 8. Recommended Immediate Next Step

The best next coding step is:

1. confirm the open requirement points in section 7
2. then refactor `ui/app-window.slint` so `AppWindow` becomes Home-only with an overlay layer
3. only after that, touch spacing and visual polish

This order minimizes churn and gives us a stable base for later UI tuning.
