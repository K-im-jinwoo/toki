# Toki — Technical Specification (`docs/SPEC.md`)

**Date:** 2026-11-18  
**References:** RunCat (tray animation loop), `statusLine` hook pattern (per-tool status + 5h/weekly/Argon remaining quota)

---

## 1. Goals & Scope

Toki is a lightweight, cross-platform desktop menu-bar / system-tray pet and quota monitor built with **Tauri v2 (Rust + React/TypeScript)**.

### Core Capabilities
- **Tray Mascot (Icon Only):** Single animated **minimal flat silhouette rabbit ("Toki")** in the menu bar / system tray (macOS template-friendly monochrome silhouette so it stays crisp at 18–22pt and adapts to light/dark menu bars).
- **Popover Dashboard:** Clicking the tray icon opens a compact, frameless popover featuring **soft pastel chibi/flat 2D Toki characters** (SVG/high-DPI) with per-tool accent colors (Codex blue, Antigravity purple, Claude coral), showing:
  1. **Codex**, **Antigravity (`agy`)**, and **Claude Code** status cards (mini character + status badge).
  2. **5-hour (`5h`)**, **Weekly (`7d`)**, and tool-specific model pool (e.g. Antigravity **`Argon`**) remaining quota (`%`) progress bars with reset countdowns (`quotas: QuotaWindow[]`).
  3. **Active & recent sessions** across tools.
  4. **1-click `statusLine` hook setup** (for Claude Code & Antigravity), custom watch paths, and **Autostart** toggle.

---

## 2. Folder Structure

```text
toki/
├── AGENTS.md
├── docs/
│   ├── SPEC.md
│   └── PROMPTS.md
├── scripts/
│   └── gen_placeholder_sprites.mjs   # Generates clean pixel-art PNG sprites for Phase 1
├── assets/sprites/
│   ├── sleep_1.png ... sleep_2.png
│   ├── idle_1.png  ... idle_2.png
│   ├── run_1.png   ... run_4.png
│   ├── sprint_1.png ... sprint_4.png
│   ├── alert_1.png ... alert_2.png
│   └── exhausted_1.png
├── resources/hooks/
│   ├── claude-statusline-hook.sh     # Cross-platform / POSIX hook (plus .ps1/.cmd for Win)
│   └── antigravity-statusline-hook.sh
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── tests/fixtures/               # Sample Codex JSONL, Claude/Agy hook JSON, SQLite fixture
│   └── src/
│       ├── main.rs / lib.rs
│       ├── schema.rs
│       ├── config.rs
│       ├── hooks.rs                  # Safe statusLine installer & uninstaller
│       ├── state.rs                  # Snapshot aggregator + MascotState machine
│       ├── animator.rs               # Preloaded Image frame ticker -> tray.set_icon
│       └── collectors/
│           ├── mod.rs
│           ├── codex.rs
│           ├── antigravity.rs
│           └── claude.rs
└── src/
    ├── types/usage.ts                # Mirrors schema.rs 1:1
    ├── components/
    │   ├── ToolCard.tsx
    │   ├── QuotaBar.tsx
    │   ├── SessionList.tsx
    │   └── SettingsView.tsx
    └── App.tsx
```

---

## 3. Mascot State Machine (`animator.rs` & `state.rs`)

Because `Antigravity` does not log token numbers locally and `Codex` only emits `token_count` periodically during a turn, **animation state must be driven by agent execution & quota state rather than TPM alone**:

Priority order (highest wins for the combined tray mascot):
1. **`alert` (`!` badge / bounce):** Any active session is waiting for user approval/input, OR an error/rate-limit just triggered.
2. **`sprint` (fast run, ~100ms/frame):** 2+ tools/sessions actively `running` simultaneously.
3. **`run` (normal run, ~180ms/frame):** At least 1 session is actively `running`.
4. **`exhausted` (hungry/dizzy, ~500ms/frame):** Any enabled tool has `5h` or `weekly` `remainingPercent <= 5%` while not actively running.
5. **`idle` (awake breathing, ~450ms/frame):** Activity within the last 10 minutes, nothing currently running.
6. **`sleep` (slow sleep, ~900ms/frame):** No activity across all tools for >= 10 minutes.

*(Note: Individual `ToolCard`s in the popover still display each tool's own independent status badge and mini sprite.)*

---

## 4. Data Collection Strategy per Tool

| Tool | Activity & Sessions | 5h / Weekly Remaining `%` | Watch / Refresh Mechanism |
| :--- | :--- | :--- | :--- |
| **Codex** | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (`session_meta`, `turn` events) | Same JSONL (`event_msg` -> `token_count` -> `rate_limits.primary` & `secondary`) | `notify` recursive watcher on `~/.codex/sessions/` + 2s debounce/rescan |
| **Antigravity (`agy`)** | `~/.gemini/antigravity-cli/conversation_summaries.db` (`CASCADE_RUN_STATUS_RUNNING` / `IDLE`, `not_fully_idle`) | `~/.toki/hooks/antigravity.json` written by `statusLine` hook (`stack_with_default: true`) | Read-only `rusqlite` query on 1.5s interval + `notify` on `~/.toki/hooks/` |
| **Claude Code** | `~/.claude/projects/*/*.jsonl` (recent file mtime / tail events) | `~/.toki/hooks/claude.json` written by `statusLine` hook | `notify` on `~/.toki/hooks/` + `~/.claude/projects/` |

### Why `statusLine` Hooks
- Claude Code and Antigravity CLI do not persist account-wide `5h` / `7d` quota percentages into local conversation logs.
- Both CLIs natively pipe live session + rate-limit JSON to a configured `statusLine` command via `stdin`.
- Toki's hook script:
  1. Reads `stdin` once,
  2. Writes an atomic temp file → renames to `~/.toki/hooks/<tool>.json` (preserving raw payload + timestamp),
  3. Passes the original `stdin` through to any pre-existing user `statusLine` command so the user's terminal status line is never broken.
- This avoids macOS Keychain password prompts and works identically across macOS, Windows, and Linux.
