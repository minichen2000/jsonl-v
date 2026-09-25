# jsonl-v

English | [简体中文](README.zh-CN.md)

A JSONL (one JSON object per line) file viewer with enhanced rendering for Kimi Code `wire.jsonl` session logs.
A single-file exe: double-click and go, no registry writes (unless you explicitly enable the context menu).

- **User guide**: below
- **Build guide**: see [BUILDING.md](BUILDING.md)

## Opening files

Pick any of the four:
- **Drag a file into the window**
- Menu "File → Open File" (shortcut `Ctrl+O`)
- Command line argument: `jsonl-v.exe path\to\file.jsonl`
- Right-click a file in Explorer → "Open with jsonl-v" (register once in "Settings" first)

"File → Recent Files" keeps the last 10 opened files.

## Layout

```
┌────────────────────────────────────────────────────┐
│ Menu: File / Settings / Help   search box  filter  │
├───────────────┬────────────────────────────────────┤
│ Request       │ Detail: Tree | Pretty | Raw        │
│ timeline      │ (llm.request lines also have the   │
│ (wire mode)   │  "Full Context (Rebuilt)" tab)     │
│               │                                    │
│ Line list     │                                    │
│ (virtual      │                                    │
│  scrolling)   │                                    │
├───────────────┴────────────────────────────────────┤
│ Status: lines | size | current line | wire mode    │
└────────────────────────────────────────────────────┘
```

- **Line list**: each row shows the line number, a summary (from the `type`/`role` fields) and the byte count
  (KB in orange, >100KB in red). Single-click selects (the list doesn't jump); right-click to copy the raw line /
  pretty text / view as plain text.
- **Tree view**: collapsible JSON tree with key/string/number/boolean coloring; "Expand all / Collapse all"
  buttons sit on the right of the tab bar.
- **Long strings**: when a value is too long or contains newlines, a "📄 View as text" button appears next to it —
  a popup window shows the unescaped text with **real line breaks** (monospace, wrappable, one-click copy).
  This is the right way to read systemPrompt, thinking content and tool outputs.
- **Pretty / Raw tabs**: the formatted JSON / the original unparsed line.

## Search

- Type in the top search box to search as you type (200ms debounce, background thread, UI stays smooth)
- `Aa` toggles case sensitivity; "Matches only" makes the line list show only hit lines
- `F3` / `Shift+F3` jump between hits
- Supports `key:value` syntax, e.g. `type:tool.call` matches only lines whose JSON `type` field is `tool.call`

## wire.jsonl enhanced mode

Enabled automatically when opening a Kimi Code session log (the status bar shows "wire mode: N requests"):

- **Event coloring**: llm.request yellow, tool.call blue, tool.result green, think purple, text gray,
  usage.record cyan, user-approval interactions orange, broken lines red
- **Request timeline** (top left, collapsible): lists each LLM request's turnStep, messageCount and the
  following token usage (input/output/cache hits); click to jump to the corresponding line
- **Full Context (Rebuilt)**: select an llm.request line and switch the detail pane to the
  "Full Context (Rebuilt)" tab — an approximate reconstruction of the full request body actually sent:
  the system prompt and tool definitions come first (taken from `profile.bind` / `llm.tools_snapshot`;
  the placeholder lines can be opened to read the original text or jumped to their source line), followed by
  the full message sequence (user messages, injected reminders, thinking, tool calls and results paired and
  indented by toolCallId), with a check that the rebuilt message count matches messageCount
- **Event filter**: the dropdown in the top bar shows only the chosen event type

## Settings (menu "Settings")

- Font size (10–24, takes effect immediately)
- Dark/light theme (light by default)
- UI language: 中文 / English
- Register/unregister the Explorer context menu
- Open the config file folder (`%APPDATA%/jsonl-v/config.json`)

All settings are saved automatically and take effect on the next launch.

## Shortcuts

| Key | Action |
|---|---|
| Ctrl+O | Open file |
| F5 | Reload (after the file was modified/appended externally) |
| Ctrl+F | Focus the search box |
| F3 / Shift+F3 | Next / previous search hit |
| ↑ / ↓ | Move selection |
| PgUp / PgDn | Move selection by page |
| Ctrl+C | Copy current line (pretty) |


## License

[MIT](LICENSE) © 2026 minichen2000
