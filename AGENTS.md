# AGENTS.md — Toki Project Rules & Architecture Contract

This repository (`toki`) is a cross-platform desktop menu-bar / system-tray companion app built with **Tauri v2 (Rust + React/TypeScript)**, inspired by RunCat.
It monitors **Codex**, **Antigravity (`agy`)**, and **Claude Code** for **live agent status** (`Running`, `Waiting`, `Idle`) and **5-hour / 7-day remaining quota (`%`)**.

> **Environment note:** Verify `rustc --version`, `cargo --version`, and `node --version` before building `src-tauri`. If Rust is missing, stop and ask the user to install `rustup` rather than switching frameworks.

---

## 1. Non-Negotiable Architecture Rules

1. **Strict 3-Layer Separation (Rust Backend → IPC → React Popover):**
   - `src-tauri/src/collectors/`: Tool-specific readers/parsers only. **No UI or tray logic here.** Every collector outputs the unified schema in `src-tauri/src/schema.rs`.
   - `src-tauri/src/state.rs` & `animator.rs`: Aggregates collector snapshots, evaluates the mascot state machine, preloads `tauri::image::Image` sprite frames once at startup, and swaps the tray icon via `tray.set_icon(...)`. **Do not re-decode PNG bytes on every animation tick.**
   - `src-tauri/src/main.rs` (or `lib.rs` + `tray.rs`): Tray initialization, click-to-toggle frameless popover positioning, OS notifications, and autostart wiring.
   - `src/`: React + TypeScript + Tailwind popover UI. Receives `UsageSnapshot` via Tauri events/commands. **Never parse local files or manage tray animation timers from React.**

2. **Tray & UI Contract:**
   - **Menu bar / System tray:** Show **only the animated character icon** (no text label next to the tray icon).
   - **Popover:** Show 3 tool cards (Codex, Antigravity, Claude Code) with mini character state, status badge, `5h` and `Week` remaining `%` bars + reset countdown, and active/recent session list.
   - **Metrics:** Always represent quota as **remaining percentage (`remaining_percent = 100.0 - used_percent`)** and `resets_at`. Never fabricate absolute remaining token numbers or TPM when tools only report percentages.

3. **Zero-Hallucination Collector Rules:**
   - **Never reference legacy Gemini CLI paths (`~/.gemini/tmp/*/chats/`).** Gemini CLI was replaced by Antigravity (`agy`).
   - **Never guess unverified JSON keys.** Use `Option<T>` for all quota fields so missing hooks or unauthenticated tools gracefully degrade to `status + sessions` without crashing.
   - **SQLite Safety:** Always open external SQLite DBs (e.g., Antigravity `conversation_summaries.db`) in **read-only mode** (`SQLITE_OPEN_READ_ONLY` / `?mode=ro`). Never lock or write to another tool's database.
   - **Preserve User Configs:** When installing `statusLine` hooks into `~/.claude/settings.json` or `~/.gemini/antigravity-cli/settings.json`, read-modify-write JSON cleanly, preserve all existing keys, and chain any pre-existing `statusLine.command`.

---

## 2. Verified Data Sources & Paths

All home-relative paths must resolve cross-platform (`dirs::home_dir()`).

### A. Codex (`collectors/codex.rs`) — Zero-Config Local Logs
- **Watch root:** `~/.codex/sessions/` (recursively, so `YYYY/MM/DD` date rollovers are handled automatically).
- **Files:** `rollout-*.jsonl`
- **Verified JSONL event structure:**
  - `type == "session_meta"`: `payload.id` / `payload.session_id`, `payload.cwd`, `payload.timestamp`
  - `type == "event_msg"` + `payload.type == "token_count"`:
    - `payload.rate_limits.primary`: `{ used_percent, window_minutes: 300, resets_at }` → **5-hour window**
    - `payload.rate_limits.secondary`: `{ used_percent, window_minutes: 10080, resets_at }` → **Weekly (7d) window**
    - `payload.info.total_token_usage`: optional secondary detail (`input_tokens`, `cached_input_tokens`, `output_tokens`, `total_tokens`)
  - Other `event_msg` turn events (`turn_id`, `started_at`, `completed_at`, `reason`) indicate whether a turn is currently running vs completed.

### B. Antigravity (`collectors/antigravity.rs`) — SQLite State + `statusLine` Hook Cache
- **Local state DBs (read-only):**
  - CLI: `~/.gemini/antigravity-cli/conversation_summaries.db`
  - Desktop (optional if present): `~/.gemini/antigravity/conversation_summaries.db`
  - Table `conversation_summaries`: `conversation_id`, `title`, `status` (e.g. `CASCADE_RUN_STATUS_RUNNING`, `CASCADE_RUN_STATUS_IDLE`), `not_fully_idle`, `killed`, `last_modified_time`, `workspace_uris`.
- **Quota (`5h` / `Week` %):** Not stored in local session logs. Read from Toki's hook cache file `~/.toki/hooks/antigravity.json`, written by a lightweight `statusLine` hook configured in `~/.gemini/antigravity-cli/settings.json` (with `"stack_with_default": true`).
- **Important:** Because exact `agy` `statusLine` `stdin` schema varies across versions, the hook script should pass through/store the raw `stdin` JSON alongside parsed fields, and the Rust parser must tolerate unknown/optional fields.

### C. Claude Code (`collectors/claude.rs`) — Local JSONL Sessions + `statusLine` Hook Cache
- **Local sessions (optional/when installed):** `~/.claude/projects/*/*.jsonl` for active/recent session timestamps.
- **Quota (`5h` / `Week` %):** Read from Toki's hook cache file `~/.toki/hooks/claude.json`, written by a `statusLine` hook in `~/.claude/settings.json`.
- Expected `stdin` payload (verify against captured fixture): `rate_limits.five_hour.{used_percentage, resets_at}` and `rate_limits.seven_day.{used_percentage, resets_at}`.

---

## 3. Unified Output Schema (`src-tauri/src/schema.rs`)

All collectors map into these exact Rust structs (serialized with `serde` to camelCase for TypeScript):

- `ToolId`: `"codex" | "antigravity" | "claude"`
- `AgentStatus`: `"running" | "waiting" | "idle" | "quota_low" | "unavailable"`
- `QuotaWindow`: `{ id: String, label: String, remainingPercent: f64, usedPercent: f64, resetsAt: Option<i64>, windowMinutes: u32 }`
- `SessionSummary`: `{ id: String, title: String, workspace: Option<String>, status: AgentStatus, updatedAt: i64 }`
- `ToolSnapshot`:
  - `tool: ToolId`
  - `installed: bool`
  - `hookConfigured: Option<bool>` (`None` for Codex which needs no hook)
  - `status: AgentStatus`
  - `quotas: Vec<QuotaWindow>` (supports `5h`, `Week`, and extra model pools like Antigravity `Argon`)
  - `activeSessions: Vec<SessionSummary>`
  - `lastUpdatedAt: Option<i64>`
- `UsageSnapshot`:
  - `overallState: MascotState` (`"sleep" | "idle" | "run" | "sprint" | "alert" | "exhausted"`)
  - `tools: Vec<ToolSnapshot>`
  - `generatedAt: i64`

---

## 4. Verification Requirements

- Unit-test every collector parser in `cargo test` using committed JSONL / JSON / SQLite test fixtures under `src-tauri/tests/fixtures/` (never depend solely on the developer's live `~/.codex` or `~/.gemini` files).
- Run `cargo check` / `cargo test` and `npm run build` (or `tsc --noEmit`) after each phase before reporting completion.
