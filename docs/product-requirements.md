# Product Requirements

## 1. Goal

Build a lightweight Windows popup tool similar to `Win + V` for quickly copying custom content.

This tool is not a clipboard history manager.
Its purpose is to make frequently reused fixed content easier to access.

## 2. Target Scenarios

- Copy serial numbers during activation flows
- Copy frequently used URLs
- Copy account information snippets
- Copy API keys, tokens, or other local-only sensitive text
- Copy standard reply text

## 3. Product Principles

- Lightweight
- Fast to open
- Fast to copy
- Local-only
- Minimal settings
- Easy to maintain manually

## 4. Functional Scope

### 4.1 Popup

- Open from a global shortcut
- Default shortcut: `Alt + V`
- Popup should feel similar to `Win + V`
- Popup opens centered or near the active screen area
- Popup supports mouse-first use

### 4.2 Tabs

- Tabs represent groups
- Initial state contains no tabs
- The add-tab button appears as the last tab slot, like a browser tab bar
- Maximum 5 tabs
- Users can add tabs
- Users can rename tabs
- Users can delete tabs
- When a tab is deleted, its items are deleted after confirmation

### 4.3 Items

- Each tab can contain up to 10 items
- Each item has:
  - `title`
  - `content`
- Cards are displayed one item per row
- The card shows title and content preview
- Clicking the card copies `content`
- Clicking the small copy icon also copies `content`
- After copy completes, the popup closes automatically

### 4.4 Add Item Entry

- The add-item entry is a small `+` icon at the bottom-right corner of the content area
- It is visible only when a tab is selected

### 4.5 Settings

Settings contains only these features in v1:
- Change the global shortcut
- Import configuration file
- Export configuration file

## 5. UI States

### 5.1 Initial Empty State

```text
┌──────────────────────────────────────────────┐
│  Quick Paste                         [⚙] [×] │
│                                              │
│  [+]                                         │
│                                              │
│                                              │
│             点击 + 新建 Tab                   │
│                                              │
└──────────────────────────────────────────────┘
```

### 5.2 Tab Exists but No Items

```text
┌──────────────────────────────────────────────┐
│  Quick Paste                         [⚙] [×] │
│                                              │
│  [序列号] [+]                                │
│                                              │
│                                              │
│            点击右下角 + 添加                  │
│                                              │
│                                        [+]   │
└──────────────────────────────────────────────┘
```

### 5.3 Item List State

```text
┌──────────────────────────────────────────────┐
│  Quick Paste                         [⚙] [×] │
│                                              │
│  [序列号 ×] [URL] [账号] [+]                 │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Edge 登录地址                      [⧉] │  │
│  │ https://microsoftedge.microsoft...     │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Windows 序列号                     [⧉] │  │
│  │ XXXXX-XXXXX-XXXXX-XXXXX-XXXXX          │  │
│  └────────────────────────────────────────┘  │
│                                   [◫]  [+]   │
└──────────────────────────────────────────────┘
```

### 5.4 Settings Popup

The settings UI should be a small lightweight panel opened from the top-right gear icon.

```text
┌──────────────────────────────────────┐
│  设置                            [×] │
│                                      │
│  [ Alt + V                    ]      │
│  选中后按下快捷键。                   │
│                                      │
│  [⇩]                      [⇧]         │
│                                      │
│  导入将覆盖当前配置。                 │
│                                      │
│                 [保存]   [取消]       │
└──────────────────────────────────────┘
```

Design intent:
- Keep the panel narrow and compact
- Show only one editable field for the shortcut
- Use icons for secondary actions when the meaning is clear
- Place import and export side by side
- Keep the explanatory text short and product-like
- Avoid turning Settings into a full management page

### 5.5 Delete Mode

Delete mode is entered from the bottom-right action area.

```text
┌──────────────────────────────────────────────┐
│  Quick Paste                         [⚙] [×] │
│                                              │
│  [序列号 ×] [URL] [账号] [+]                 │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Edge 登录地址                 [🗑] [⧉] │  │
│  │ https://microsoftedge.microsoft...     │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Windows 序列号                [🗑] [⧉] │  │
│  │ XXXXX-XXXXX-XXXXX-XXXXX-XXXXX          │  │
│  └────────────────────────────────────────┘  │
│                                   [✓]  [+]   │
└──────────────────────────────────────────────┘
```

Design intent:
- Keep delete hidden during normal use
- Enter delete mode only when explicitly requested
- Use icons instead of extra text buttons
- Keep add and mode-switch actions together in one place

### 5.6 Delete Confirmation Popup

Delete actions should use one shared compact confirmation popup.

```text
┌──────────────────────────────────────┐
│  删除                            [×] │
│                                      │
│  确认删除？                          │
│                                      │
│                 [删除]   [取消]       │
└──────────────────────────────────────┘
```

Design intent:
- Keep confirmation short and direct
- Reuse the same dialog for tab delete and item delete
- Avoid verbose warning copy

### 5.7 Add Tab Popup

The add-tab UI should be a small inline-style popup with only one input and two actions.

```text
┌──────────────────────────────────────┐
│  新建分组                        [×] │
│                                      │
│  分组名称                            │
│  [ 分组名称                    ]      │
│                                      │
│                 [创建]   [取消]       │
└──────────────────────────────────────┘
```

Design intent:
- Keep the popup minimal
- Ask only for the tab name
- Make creation fast and low-friction
- Match the lightweight feel of the main popup

### 5.8 Add Item Popup

The add-item UI should stay compact, but allow enough room for multi-line content.

```text
┌──────────────────────────────────────┐
│  新增内容                        [×] │
│                                      │
│  标题                                │
│  [ 标题                        ]      │
│                                      │
│  内容                                │
│  ┌──────────────────────────────┐   │
│  │ 内容                          │   │
│  │                              │   │
│  │                              │   │
│  └──────────────────────────────┘   │
│                                      │
│                 [保存]   [取消]       │
└──────────────────────────────────────┘
```

Design intent:
- Keep the form focused on just title and content
- Let content support multi-line text
- Avoid secondary fields or advanced options in v1
- Preserve a compact footprint

## 6. Interaction Details

### 6.1 Copy Behavior

- Card click copies content immediately
- Copy-icon click copies content immediately
- Both actions close the popup after copying
- A lightweight copied feedback may briefly appear before close if timing allows
- Icon-only actions should use hover or tooltip text where needed

### 6.2 Manage Tabs

- Add tab from the final `+` tab slot
- If 5 tabs already exist, the add-tab control is disabled or hidden
- Tab creation requires a tab name
- The active tab can show a small close icon for deletion
- Clicking the add-tab slot opens the add-tab popup
- Creating a tab switches the UI to the new tab
- Empty or duplicate-looking names should show a short inline validation message
- Deleting a tab opens the shared delete confirmation popup
- Clicking `取消` or the top-right close button closes the popup without changes

### 6.3 Manage Items

- Add item from the bottom-right `+` icon
- If 10 items already exist in the current tab, the add-item control is disabled or hidden
- Item creation requires title and content
- Item editing supports title and content updates
- Item deletion requires confirmation
- Each item card shows the copy icon as the only always-visible action
- The bottom-right action area shows a mode-switch icon and the add icon
- Clicking the mode-switch icon enters delete mode
- In delete mode, each item card shows a delete icon next to the copy icon
- In delete mode, the mode-switch icon changes to a done icon
- Clicking the done icon exits delete mode
- Clicking the add-item `+` opens the add-item popup
- Saving a new item inserts it into the current tab and updates the list immediately
- Deleting an item opens the shared delete confirmation popup
- Title and content validation errors should be shown inline
- Clicking `取消` or the top-right close button closes the popup without changes

### 6.4 Settings Interaction

- Clicking the gear icon opens the settings popup above the main window
- The shortcut field enters capture mode when focused
- The user presses a key combination to replace the current shortcut
- Invalid shortcuts should show a short inline error
- Clicking `导入配置` opens a file picker for `.toml`
- Import requires confirmation before replacing existing local data
- Clicking `导出配置` opens a save dialog and writes a readable `.toml` file
- Clicking `保存` applies the shortcut change and closes the popup
- Clicking `取消` or the top-right close button closes the popup without saving shortcut edits

## 7. Data Storage Strategy

### 7.1 Storage Principles

- Local-only
- Human-readable
- Easy to back up
- Easy to import/export
- Usable even when no config file exists yet

### 7.2 File Format Recommendation

Use `TOML` as the canonical file format.

Recommended file name:
- `quick-paste.toml`

Reasons:
- Better readability than raw JSON for manual editing
- Supports nested structures clearly
- Friendly for tabs and item lists

### 7.3 First Run Without Config File

- The app must start normally even if no config file exists
- Missing config file is treated as first-run state, not an error
- On first run, the app uses an in-memory default configuration
- The initial UI shows the empty state and allows creating tabs and items immediately
- The config file is created only after the first successful save, shortcut change, or import

Recommended in-memory default:

```toml
version = 1
hotkey = "Alt+V"
tabs = []
```

### 7.4 Sensitive Data Consideration

The user may store passwords, keys, or tokens.

Because the user also wants manual editing and readable import/export, v1 should keep the file readable instead of encrypted.
This means:
- Data stays local only
- The app should not upload anything
- The app should clearly communicate that exported files may contain plain text sensitive content
- Future versions may add optional encryption, but not in the initial version

## 8. Import / Export Requirements

### 8.1 Export

- Export current data to a `.toml` file
- The exported file should be immediately readable and editable
- Export should preserve tab order and item order

### 8.2 Import

- Import from a `.toml` file
- Imported data replaces current local data after user confirmation
- Validation should detect:
  - invalid file structure
  - more than 5 tabs
  - more than 10 items in a tab
  - missing tab names
  - missing item titles or item contents
- Validation errors should be shown in clear language

### 8.3 Direct Manual Editing Flow

Supported workflow:
1. Export configuration
2. Edit the `.toml` file manually
3. Import the file back into the app
4. The UI reflects the changes immediately

## 9. Recommended Configuration Schema

```toml
version = 1
hotkey = "Alt+V"

[[tabs]]
name = "URL"

[[tabs.items]]
title = "Edge 登录地址"
content = "https://microsoftedge.microsoft.com/addons/detail/authenticator-2fa-client/ocglkepbibnalbgmbachknglpdipeoio"

[[tabs.items]]
title = "测试环境"
content = "https://test.example.com/login"

[[tabs]]
name = "账号"

[[tabs.items]]
title = "管理员账号"
content = "admin@example.com"
```

## 10. Validation Rules

- `version` is required
- `hotkey` is required
- `tabs` count must be between 0 and 5
- each tab `name` is required
- each tab item count must be between 0 and 10
- each item `title` is required
- each item `content` is required

## 11. Out of Scope for v1

- Clipboard history
- Search
- Cloud sync
- Multiple user profiles
- Encryption UI
- Rich text or file attachments
- Paste simulation

## 12. Suggested Technical Direction

Recommended stack for implementation:
- Tauri
- React
- Local TOML file storage
- Windows global shortcut support

Reasoning:
- Small app size
- Good Windows desktop fit
- Easy local file access
- Easy lightweight popup UI

## 13. Performance Requirements

The app should feel instant in normal use.

### 13.1 User-Visible Targets

- Hotkey to visible popup should usually complete within 150 ms on a normal Windows desktop
- Clicking a card or copy icon should copy content within 50 ms and then close the popup immediately
- Switching tabs should feel immediate and should not trigger disk reads
- Opening Settings, Add Tab, and Add Item popups should feel immediate

### 13.2 Data Scale Assumptions

The first version has a very small upper bound:
- Up to 5 tabs
- Up to 10 items per tab
- Up to 50 total items

Because the total data size is small, virtualization is not necessary in v1.
The preferred approach is to optimize for simplicity and responsiveness.

### 13.3 Recommended Implementation Strategy

- Keep parsed configuration in memory while the app is running
- Load local configuration once during startup or first open
- Do not re-parse the TOML file on every popup open
- Keep the popup window alive and hide/show it instead of recreating it each time
- Keep CRUD operations local and lightweight
- Persist changes to disk after edits, not during every render path
- Avoid heavy animation, large shadow effects, or unnecessary blur that may slow popup rendering
- Avoid heavyweight runtime dependencies for a very small UI surface

### 13.4 Import / Export Performance

- Import should validate the full file before replacing in-memory data
- Import should update memory first and then persist to disk
- Export should write directly from the in-memory model
- Import and export should remain responsive for normal configuration files

### 13.5 Startup and Background Behavior

- The app should be lightweight when idle
- Register the global shortcut once during app startup
- Prefer a single hidden popup window over creating and destroying windows repeatedly
- Minimize background work when the popup is not visible

### 13.6 Error Handling and Stability

- A failed config write should not freeze the popup
- Invalid import files should fail fast with a clear message
- Shortcut registration failure should show a clear error and preserve the last valid configuration
