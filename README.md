# Headroom

**Your Claude plan limits, always in view.** A small desktop widget and menu-bar item that shows how much of your Claude **5-hour session** and **weekly** limits you've used, and when each one resets. The same numbers Claude keeps under Settings → Usage, without the clicking.

![Headroom widget](docs/screenshots/widget.png)

- **Live bars** for every limit on your plan: the 5-hour session, the weekly all-models limit, and any per-model weekly cap Anthropic adds later (they appear automatically).
- **Reset countdowns** that tick on their own, plus the local reset time ("Resets in 2h 14m · 5:49 PM").
- **A pace tick** on each bar showing how much of the window has passed. If the fill is past the tick, you're spending faster than the clock.
- **Menu bar / tray readout** (`7% · 30%`) and **alerts** when a limit crosses thresholds you choose (80% and 95% by default).
- **Details window**:
  - *Limits*: big bars, plus **where this week went** (Claude Code vs. chats vs. Cowork), straight from Anthropic.
  - *Models*: which models you actually use, from Claude Code's local logs: replies and tokens per model, a per-day chart, 7 days / 30 days / all time.
- Small (~10 MB), runs on **macOS, Windows and Linux**, light and dark mode.

## How it works

Headroom borrows the sign-in that **Claude Code** already saved on your computer, read-only. On macOS that's the login keychain; elsewhere it's `~/.claude/.credentials.json`. It then asks Anthropic for your usage. It never sees a password, and it never sends your data anywhere except Anthropic.

- Usage comes from `GET https://api.anthropic.com/api/oauth/usage`, the endpoint behind Claude's own usage page and Claude Code's `/usage`. **It isn't officially documented**, so a Claude update could change it. Headroom reads the generic `limits[]` list rather than hard-coding fields, to bend rather than break.
- Claude Code refreshes its token whenever it runs. If the token has expired, Headroom asks the `claude` CLI to check its sign-in (`claude auth status`), which costs no usage. Headroom never writes credentials itself.
- Model analytics read `~/.claude/projects/**/*.jsonl` locally. Each assistant reply is counted once, with sub-agent transcripts included.

**Requirement:** Claude Code installed and signed in with a Claude subscription (Pro or Max) on the same computer.

## Install

Download the installer for your system from [Releases](../../releases). Builds aren't code-signed yet:

- **macOS:** right-click Headroom.app → Open the first time.
- **Windows:** "More info" → "Run anyway" on the SmartScreen prompt.

## Build from source

Requirements: Node 20+, Rust (stable), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # produce an installer in src-tauri/target/release/bundle
```

Tests: `npm test` (formatting and analytics logic) and `cd src-tauri && cargo test` (sign-in parsing, usage parsing against a real response, log scanning, settings).

## Project layout

```
src-tauri/src/
  claude/credentials.rs   find Claude Code's sign-in (keychain / file), nudge a refresh
  claude/usage.rs         call the usage endpoint, parse it into provider-neutral limits
  model.rs                Limit / Snapshot types shared with the UI
  poller.rs               background refresh loop, menu-bar text, threshold alerts
  analytics.rs            per-day, per-model stats from Claude Code's local logs
  settings.rs             preferences (JSON in the OS config folder)
  lib.rs                  windows, tray menu, commands
src/
  Widget.tsx              the always-on-top widget
  details/                Limits / Models / Settings tabs
  format.ts               countdowns, model names, pace (unit-tested)
```

The UI only knows about `Snapshot`s of `Limit`s, so another provider (e.g. OpenAI Codex) can be added by producing the same shape.

## Roadmap

- Sign in with a claude.ai session as an alternative to Claude Code
- More providers (Codex / ChatGPT, API-key spend)
- History: keep snapshots to chart how limits fill over a week
- Signed and notarized builds

Unofficial; not affiliated with or endorsed by Anthropic. "Claude" is a trademark of Anthropic.

MIT © Will Kline
