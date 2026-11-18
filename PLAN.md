# Rooom — plan and decisions

Status: **milestones 1 and 2 complete; milestone 3 first batch passed local fixture acceptance on 2026-11-15**. The
owner approved the design on 2026-11-11. [EXPERIMENTS.md](EXPERIMENTS.md)
holds the observed results. "Open" holds the remaining product decisions.
The owner directed the first batch of milestone 2 on 2026-11-15.

## Intent

The owner runs several coding agents at once, each in its own desktop app.
What stalls the work is waiting for the owner: approvals, questions and
replies are scattered across windows. Rooom gathers them in one small window
that hides itself like a HUD, lets the owner answer on the spot, and lets the
agents exchange messages through MCP.

Rooom observes sessions that already run in their own apps. It owns no
conversation engine: when a new session is wanted, the client's own app
starts it at Rooom's request.

Only sessions running on this computer are in reach: a cloud session does not
read local settings, and a remote session reads the remote host's.

## Reference

`jazzenchen/agent-hud-open`, Apache-2.0, read at commit
`d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` (2026-11-11). It is a design
reference only (D-001): Rooom copies none of its code, assets or wording.
Size at that commit: about 140,000 lines of Rust in 11 crates and 60,000
lines of TypeScript, covering 27 agents.

Facts read from its source, used as input to the decisions below:

| Fact | Where |
| --- | --- |
| Layering: client formats as data, then a core (ledger, state), then operating-system adapters, then one runtime, then the window host; the interface and MCP only render normalized facts. | `docs/platform-architecture.md` |
| A same-user Windows named-pipe server exists and carries hook callbacks, local MCP tools and the single-instance handover. | `src/crates/platform/src/os/windows/pipe.rs`, `hooks/windows.rs`, `local_tools/windows.rs`, `instance/windows.rs` |
| The public build never installs client hooks: it has no delegated hook-owner executable. | `docs/client-hooks.md`, `docs/platform-builds.md` |
| Words reach a Claude Code session through a Claude Code plugin and a shared folder: the session checks in every 5 s, the app leaves a file, the plugin claims it and submits it when the session is idle. States: taken, started, refused, cancelled, expired. | `src/crates/platform/src/claude_plugin/` |
| Claude Code permission, plan and question requests arrive through hooks in `settings.json`; the Windows launch form is an argv pair and is marked unverified. | `src/crates/agents/src/claude/hooks.rs` |
| The Claude desktop app's own questions and next-step buttons are read from the session log and answered through the same plugin channel. | `src/crates/agents/src/claude/desktop_ask.rs`, `src/crates/core/src/clients/claude_desktop.rs` |
| Codex on Windows: start and steer through the desktop app's owner pipe, enqueue through a short-lived `codex app-server`. No interrupt. No control of a thread held only by a terminal CLI. | `src/crates/platform/src/delivery/codex/windows.rs` |
| Antigravity on Windows: sends into an existing session through its running local language server. No interrupt, no new session. | `src/crates/platform/src/delivery/antigravity/windows.rs` |
| DeepSeek Harness has no hooks. A plugin module that every Harness host loads, the desktop app's included, shares one folder with the app: check-ins, the questions and approvals a session waits on, their answers, and words left for a session. Its desktop link is not verified on Windows. | `src/crates/agents/src/deepseek/`, `src/crates/platform/src/deepseek_plugin/` |
| Agent-to-agent messages are four MCP tools (`list_sessions`, `send_message`, `start_session`, `get_message`); the relay names the sender, messages are enqueue-only, and a chain is capped (default 10 passes). | `docs/agent-messages.md`, `src/apps/mcp/README.md` |

None of this has been run on Windows by Rooom. Every row is a reading of
source and documents, not a verified behavior.

## Decisions

Decisions are dated 2026-11-11 unless a row gives another date. The owner
chose them, most of them from options
compared on performance, ease of use, resource consumption and user
experience. A later row that changes an earlier one is named in both.

| ID | Decision | Choice |
| --- | --- | --- |
| D-001 | Code origin | Design reference only, full rewrite. No reference code is copied. |
| D-002 | Integration model | Observe sessions running in their own apps, not sessions Rooom runs itself. |
| D-003 | v1 scope | Inbox core: session list and state, approvals, answers to questions, replies with queued / received / started states, cancel and interrupt, agent-to-agent messages through MCP, one small auto-hiding window. No quota or token statistics. Interrupt is offered only where D-024 and D-038 allow it. |
| D-004 | Addressing | A reply goes to the sender of the message it answers; otherwise the message goes to the one recipient shown on the chip beside the composer (D-022, D-040). No broadcast, no model-based routing, no `@name` syntax. |
| D-005 | How the rest is decided | One item at a time, each with advantages, disadvantages and a recommendation. |
| D-006 | Clients in v1 | Four desktop clients, built at the same time: the Claude desktop app, Codex Desktop, Antigravity and DeepSeek Harness. A client that fails its experiment leaves v1 (D-037). |
| D-007 | Process architecture | One resident executable owns the inbox, the window, watching and receiving within the instance scope of D-049. The hook callback and the MCP relay are the same executable started with a flag (`--hook`, `--mcp`). The hook callback lives for one request; the MCP relay lives as long as the client session that started it. |
| D-008 | Back-end language | Rust. Chosen over Go, C#/.NET and TypeScript. |
| D-009 | Window shell | Tauri 2 on WebView2. Chosen over Slint, egui and native Win32/WinUI because the window is mostly text: Chinese input, selection, Markdown and code blocks. Accepted cost: the highest memory use of the four. |
| D-010 | Rust build gate | The application and all Windows-only code build and test on the Windows host with MSVC; crates without operating-system code also build and test in a Linux container. A third gate was added by D-041. |
| D-011 | Front-end framework | React 19, built with Vite and TypeScript. Chosen over Svelte 5, SolidJS and no framework. D-050 sets the source and compiler. |
| D-012 | Components and styling | Headless components with hand-written component CSS; see "Front-end rules". |
| D-013 | Front-end state | The framework's own primitives plus one small hand-written store holding the snapshot the back end pushes. No state library. |
| D-014 | Animation inside the window | Native CSS and View Transitions first, then Anime.js, then Three.js; React Bits as a visual reference. The window's own slide is moved by Rust and is not part of this. |
| D-015 | Front-end conventions | The rules in "Front-end rules" apply in full, including React Compiler and Tailwind for tokens and layout only. |
| D-016 | Local IPC | A same-user named pipe between the resident program and its `--hook` and `--mcp` modes, scoped by D-049; no port. Chosen over loopback HTTP with a token and over exchanging files. Plugin channels use a shared folder, because a client plugin can only read and write files. |
| D-017 | Storage | Keep no conversation content. Settings are a JSON file; a send receipt records identifiers and state, never the words; the timeline is rebuilt each time from each client's own records. No database. Delivery files are the one exception (D-036). |
| D-018 | MCP server implementation | The official Rust MCP SDK, compiled into the same executable and served by `rooom.exe --mcp`. |
| D-019 | Resting form of the window | A thin strip on a screen edge with one state mark per agent. A panel about 380 px wide slides out when the pointer comes near or a request arrives, and slides back a few seconds after the pointer leaves; it stays out while the owner types or has pinned it. A tray icon and a global shortcut exist as well. |
| D-020 | How a new request announces itself | The panel opens by itself and shows the request card without taking keyboard focus, so a keystroke meant for another window cannot answer a request. |
| D-021 | Main layout | One timeline for all agents, the requests waiting for the owner fixed at the top, agent chips to filter, the composer at the bottom. Agent-to-agent messages appear in the same timeline. |
| D-022 | Choosing the recipient | A recipient chip beside the composer, pre-filled with the last recipient; the owner changes it by clicking, and Enter sends. Accepted cost: a message goes to the wrong agent if the owner forgets to change the chip. |
| D-023 | Request cards | A summary first (which agent, which project, what it wants to do), expandable to the full command or diff, plus one click back to the session in its own app. |
| D-024 | Cancel and interrupt controls | Each message and session shows only the actions that client can really perform at that moment. An action it cannot perform is disabled with the reason. No forced stop that kills a client process. |
| D-025 | Interface languages | Traditional Chinese (Taiwan), English, Simplified Chinese and Japanese. The default follows the system language. |
| D-026 | Theme | Follows the system light or dark setting, with a manual override. |
| D-027 | What agents may do through MCP | Message an existing session, read its answer, and ask another client to start a new session with a task, within the mode chosen in D-033. |
| D-028 | Connecting a client | Rooom only shows what to add (a hook entry, a plugin, an MCP server entry) and the owner adds it by hand. Rooom never writes another client's settings. |
| D-029 | License | GPL-3.0, stated by the `LICENSE` file alone, with no separate "only" or "or later" statement. |
| D-030 | Repository | Public GitHub repository `ShanireZ/Rooom`. |
| D-031 | README | Four synchronized editions: English, Traditional Chinese, Simplified Chinese and Japanese. |
| D-032 | Claude Code terminal sessions | Not blocked and not filtered: they share the desktop app's hooks and plugin, so they appear as they come. They are not a v1 target: nothing is built or verified for the terminal. |
| D-033 | Agent interaction modes | Three modes, chosen by the owner. **Default:** messages are free, and Rooom asks the owner first for four things: starting a new session, continuing past the chain cap, a message to a session whose permission mode is looser than the sender's (where Rooom can read it), and a message to a session in a different project folder. **Ask everything:** every message and every new session becomes a card. **Allow everything:** nothing is asked; only the chain cap applies. |
| D-034 | An unanswered request | It waits 10 minutes. The panel slides back as usual and the strip keeps signalling that agent with a count. After 10 minutes the client asks in its own window. |
| D-035 | "Always allow" | Offered, for the current session only. The rule lives in the client's memory and no settings file is written. |
| D-036 | Delivery files | A message waiting for a session may sit as a file in a folder only the owner's account can read. It is deleted when taken and expires after one hour. A queued message survives a restart of Rooom. |
| D-037 | When an experiment fails | A client that cannot hand Rooom its requests, or cannot be sent words, on Windows is left out of v1. A lesser ability that fails (interrupt, starting a session) is disabled with its reason, as D-024 says. |
| D-038 | Acceptable channels | Documented interfaces and undocumented private interfaces of a client. No UI Automation and no synthetic key presses. |
| D-039 | Idle budget | None. The WebView stays resident and draws the strip too, as accepted in D-009. |
| D-040 | Recipients per message | One. The chip never holds more than one session. |
| D-041 | CI gate | A Windows build and test on GitHub Actions for every push and pull request, as a third gate beside D-010's two. |
| D-042 | Client marks | Rooom's own set of abstract icons, one per client. No official logos. |
| D-043 | Three.js in v1 | v1 ships one signature shader effect. Its design is approved by the owner before any of it is written. |
| D-044 | Screen sharing | Both: the window is excluded from screen capture by default, with a setting to turn that off, and a manual privacy mode hides request details and stops the panel from opening by itself. |
| D-045 | Level of protection | The owner's own account is the trust boundary. Basic protection only: the pipe admits the owner's account alone, a request is answered only in Rooom's interface or the client's own window, and a helper mode checks that the process calling it is a known client. |
| D-046 | Markdown and code | `react-markdown` renders the timeline; Shiki colors code, loaded on demand per language. |
| D-047 | Code signing | Apply to a free signing service for open-source projects. Which one, and whether Rooom qualifies, is not yet checked. |
| D-048 | Design approval | The owner approved the sections "Architecture", "MCP", "Safety, privacy and failure rules", "Front-end rules", "Interface", "Dependencies", "Build and verification" and "Milestones" as written here. |
| D-049 | Windows resident and pipe scope (2026-11-15) | One resident per owner account and Windows session. Each session has its own panel and a named pipe keyed by the owner SID and actual Windows session ID. A second launch in the same session opens the existing panel and exits. Helpers connect to the resident in their own session. Concurrent desktop sessions each use a resident WebView2. G-18 supports the pipe choice. Refines D-007 and D-016. |
| D-050 | TypeScript compilation (2026-11-15) | Write React components in plain TypeScript with `createElement`. Compile the frontend with TypeScript 7.1, then bundle the emitted JavaScript with Vite. The available 7.1 beta is pinned as `7.1.0-beta`. Keep local declarations for the React APIs Rooom uses. Refines D-011. |
| D-051 | Default global shortcut (2026-11-15) | New profiles start with `"shortcut": null`. A user can set a binding in `settings.json` and restart. Report an invalid or unavailable binding in the panel. The tray remains available. |
| D-052 | Codex approval timeout (2026-11-15, resolves Open 8) | Follow Codex Desktop's user-reviewed native approvals through the verified owner pipe. Codex keeps its native card available throughout. Rooom offers its card for ten minutes after first observing the request. Expiry withdraws Rooom's card without submitting a decision. D-034 still governs the wait. |
| D-053 | Codex approval scope (2026-11-15, resolves Open 9) | Commands offer allow once and deny. File changes also offer a session grant for the exact paths on the verified request when eligible. Use native `acceptForSession` for that grant. Rooom never sends an exec-policy amendment or writes a rules file. Refines D-035. |
| D-054 | Harness compressed history (2026-11-15, resolves Open 7) | Pin `zstd = 0.14.4` with the vendored decoder and default features disabled. Read native concatenated Zstandard frames into a bounded in-memory timeline. No decoded transcript is stored by Rooom. |

## Design

### Architecture

```text
client apps ──hook──▶ rooom.exe --hook ──┐
client apps ──MCP───▶ rooom.exe --mcp  ──┤  same-user named pipe
                                         ▼
plugin folders ◀──files──▶  rooom.exe (resident)
client records  ──read───▶    core: sessions, requests, receipts, addressing
                              window host: strip, panel, tray, shortcut
                                         │ snapshot + events
                                         ▼
                              React interface (renders, keeps drafts)
```

Crates, each with one job:

| Crate | Owns | Must not own |
| --- | --- | --- |
| `rooom-clients` | Each client's record, request and channel formats, as data and pure parsers. | Files, processes, pipes, windows. |
| `rooom-core` | Session state, pending requests, the timeline projection, receipts, addressing rules, interaction modes. | Operating-system calls, interface state. |
| `rooom-platform` | Windows adapters: the named pipe and its security descriptor, file watching, each client's delivery channel. | Parsing a client format a second time. |
| `rooom-mcp` | The MCP tools and the sender's identity. | Delivery itself; it asks the core. |
| `rooom-app` | The Tauri host: strip, panel, focus, tray, shortcut, the commands the interface calls. | Reading client records. |

Rules:

- The interface never computes state. The back end pushes one snapshot and
  events; the front end renders them.
- Shared code never branches on the operating system. A capability that
  differs gets one selector and one file per host.
- A send is never retried blindly. Once bytes may have left, an unconfirmed
  send stays "uncertain" and is shown as such.
- If there is no resident in the helper's Windows session, a hook
  callback answers nothing and the client asks in its own window.

### Clients (expected; [EXPERIMENTS.md](EXPERIMENTS.md) says what has been observed)

| | Claude desktop app | Codex Desktop | Antigravity | DeepSeek Harness |
| --- | --- | --- | --- | --- |
| Sessions and state | Process registry (observed) | Thread records (observed) | The app's local service, found through its hub file (observed) | Plugin check-ins |
| Replies are read from | Session records (observed) | Thread records (observed) | Transcript files (observed) | Compressed session files (observed) |
| Requests arrive by | `PermissionRequest` hook (observed); the app's own questions from the session log | `PermissionRequest` hook in `hooks.json` (observed), once the owner has allowed it in Codex and for threads the user reviews; the desktop owner's pipe for command and file-change requests (X-22), other card types open | The app's local service, polled for waiting steps (observed) | A plugin listening to `approval/request` (observed) |
| Words are sent by | Plugin module and shared folder (observed) | Codex's queue, through the `codex queue` command of the executable the app bundles (observed) | The app's local service (observed) | A plugin calling the session controller's `prompt` in queue mode (observed) |
| Cancel an untaken message | Desktop queue control (C-28); a Rooom-callable Desktop withdrawal remains open | Desktop queue withdrawal (X-15) | Desktop service withdrawal (A-15) | Plugin queue withdrawal in an isolated Desktop Host (D-11, D-18) |
| Interrupt a running turn | The plugin module cancels the turn (C-10); a denial can interrupt at a permission boundary (C-7) | Desktop interruption (X-16) | Desktop service interruption (A-16) | Plugin interruption in an isolated Desktop Host (D-12, D-18) |
| Start a new session | CLI seed and Desktop import observed (C-15); the Rooom entry point remains open | Desktop thread creation (X-14) | Desktop service conversation creation (A-14) | Plugin session creation in an isolated Desktop Host (D-10, D-18) |

"Requests arrive" and "Words are sent" are the two rows D-037 judges a
client by.

Claude Code facts from its documentation (the hooks reference and the plugin
"mods" pages under `code.claude.com/docs`), read on 2026-11-11 through a
delegated check that could not read the hooks page to its end:

- A `PermissionRequest` hook receives the tool's name and input and answers
  allow or deny. An allow can add a permission rule for the session, the
  project or the user. A deny can interrupt the turn.
- A command hook waits 600 seconds by default; each entry can set its own
  timeout. No maximum is documented.
- A hook whose command is missing or fails does not block the tool call,
  unless the entry asks for that.
- A hook entry can name an executable and its arguments with no shell. On
  Windows it must be a real `.exe`.
- A plugin can ship a JavaScript module with events for session start, tool
  call, prompt submit and turn complete, and with file, clock, process and
  HTTP access. It can submit a prompt once the session is idle, never into a
  running turn. Its stability is not stated.
- Hooks in the user's local settings do not reach cloud sessions.
- A plugin module can also cancel the running turn, given the id its
  `turn.start` hook received. This is in the type definitions shipped with
  Claude Code 2.1.321 and was observed to work; a shell command already
  running was left to finish in the background.
- Not documented: a session identifier in the environment of an MCP server. A shell started by a
  desktop session was observed to have `CLAUDE_CODE_SESSION_ID`.

### MCP

- Transport: stdio. A client starts `rooom.exe --mcp`, which relays to the
  resident program over the pipe.
- Tools: list sessions, send a message, read the answer, start a session.
- Off until the owner turns it on, then governed by the mode of D-033. A
  chain of passes is capped, default 10.
- Messages are enqueue-only: they wait until the receiving session is idle.
- A message from an agent is always marked as an agent's. It is never sent
  as the owner's own words.
- Every message between agents is shown in the owner's timeline.
- The relay names the sender from what the client provides about itself; an
  argument written by the agent never names it. Replies, the chain cap and
  D-033's comparisons need the sending *session*. The experiments obtained
  that identity for all four Desktop clients (C-14, X-17, X-21, A-13, D-16,
  D-18). The adapters must verify the identity path against their supported
  client versions.

### Safety, privacy and failure rules

- **Fail toward the client.** No answer, a timeout, a crash, a closed pipe or
  a missing Rooom all mean the client asks in its own window. Nothing ever
  defaults to allow.
- **One place answers.** A request is answered by the owner in Rooom's
  interface or in the client's own window, whichever comes first; the other
  card then disappears. Observed for Claude and Antigravity. Codex's
  owner-pipe channel follows the native card and leaves it available
  throughout (D-052).
- **Agent text is untrusted.** Markdown is rendered without raw HTML, scripts
  or remote loads, under a strict content security policy. A link shows its
  address and opens in the browser. A collapsed card shows the head and the
  tail of a command and its length, so a summary cannot hide what runs.
- **No network.** Rooom makes no network request.
- **No words in logs.** Logs hold identifiers and states only.
- **The trust boundary is the owner's account** (D-045). Another program
  running as the owner can already edit a client's settings; Rooom does not
  try to stop it and must not make it easier.
- **Words on disk** only as D-036 allows.
- **Out of sight when shared** (D-044).

### Front-end rules

- **Components.** shadcn/ui is the standard: generate a component with its
  CLI as the starting point, keep its API names, composition and `data-slot`
  parts, and rewrite its appearance into component CSS. The headless layer is
  Base UI. No `class-variance-authority`, `cn` or `tw-animate-css`.
- **CSS.** Plain CSS files, one per component, in a declared `@layer` order;
  native nesting; state through `data-*` and `aria-*` attributes; colors only
  through tokens named as shadcn names them, each defined once with
  `light-dark()`; `!important` only in an accessibility layer. Tailwind 4
  supplies tokens and layout utilities only. Class prefix `rm-`.
- **Motion, in tiers.** CSS first (transitions, `@starting-style`, View
  Transitions); then Anime.js in its WAAPI mode; then the Anime.js JS engine
  only for what WAAPI cannot do; then a Three.js shader in its own canvas.
  Animate only `translate`, `scale`, `rotate` and `opacity`. Load Anime.js and
  Three.js on demand. Follow the system's reduced-motion setting and offer an
  in-app switch.
- **React Bits is a visual reference only.** Its license, read on 2026-11-11,
  is "MIT + Commons Clause License Condition v1.0". Rooom rebuilds an effect
  from what it looks like and copies no source.
- **A WebGL canvas is temporary.** It exists only while the panel is shown
  and an effect plays, and its context is released when the panel hides. This
  binds the signature effect of D-043 too.

### Interface

Sketch of the open panel:

```text
┌────────────────────────────────┐
│ All  ◆  ▲  ●  ■                │  filter chips, Rooom's own icons
├─ Waiting for you ──────────────┤
│ ▲ Codex · api-server           │
│ Run pnpm test?            ▸    │  expands to the full command
│ [Allow] [Allow for session] [Deny]
├─ Timeline ─────────────────────┤
│ 10:02 ◆ Claude · Rooom         │
│   The plan is written…         │
│ 10:03 You → Codex              │
│   Run the tests first  started │
│ 10:04 ▲ Codex → ◆ Claude (MCP) │
│   Please review this change    │
├────────────────────────────────┤
│ To: [▲ Codex ▾]  Message…      │
└────────────────────────────────┘
```

- **Recipient chip.** One session (D-040). The keyboard reaches it without
  the pointer. Replying to a timeline message sets the chip for that one
  message. A last recipient that is gone leaves the chip empty.
- **Session names.** Client, then project folder, then a short title; two
  sessions that would read the same get a number.
- **Tray menu.** Open, pause alerts, privacy mode, settings, quit.
- **Settings.** Per client: connection state and the text to add by hand
  (D-028). Agent messages: the switch, the mode (D-033) and the chain cap.
  Language, theme, motion, capture exclusion, which edge and display, the
  shortcut, start at sign-in.
- **Languages.** Hand-written message catalogs, one file per language; no
  internationalization library.

### Dependencies

The initial versions and licenses were read from the registries on
2026-11-11. Every license read is compatible with GPL-3.0. D-050 records
the TypeScript pin adopted on 2026-11-15.

| Dependency | Version read | License | Why |
| --- | --- | --- | --- |
| `tauri`, `tauri-build` | 2.12.5, 2.7.4 | Apache-2.0 OR MIT | Window shell (D-009). |
| `rmcp` | 3.9.0 | Apache-2.0 | Official Rust MCP SDK (D-018). |
| `tokio` | 1.54.1 | MIT | Named pipes with cancellation and deadlines. |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | Security descriptors, window styles, focus, capture exclusion. |
| `notify` | 8.2.2 | CC0-1.0 | Watching client records and plugin folders. |
| `serde`, `serde_json` | 1.0.229, 1.0.154 | MIT OR Apache-2.0 | Records, snapshot, settings. |
| `zstd` | 0.14.4 (D-054, 2026-11-15) | BSD-3-Clause | Harness compressed history, vendored with default features disabled. |
| `sha2`, `uuid` | 0.11.1, 1.29.2 | MIT OR Apache-2.0 | Request fingerprints and identifiers. |
| `tauri-plugin-global-shortcut` | 2.4.1 | Apache-2.0 OR MIT | The global shortcut of D-019. |
| `tauri-plugin-single-instance` | 2.5.5 | Apache-2.0 OR MIT | One resident copy; a second launch raises the first. |
| `tauri-plugin-autostart` | 2.7.0 | Apache-2.0 OR MIT | Optional start at sign-in. |
| `react`, `react-dom` | 19.3.1 | MIT | Interface (D-011). |
| `babel-plugin-react-compiler`, `@rolldown/plugin-babel` | 1.0.0, 0.3.1 | MIT | React Compiler (D-015). `@vitejs/plugin-react` 6 names both as optional peers; it no longer runs Babel itself. |
| `vite`, `@vitejs/plugin-react` | 8.3.8, 6.3.0 | MIT | Build. |
| `typescript` | 7.1.0-beta | Apache-2.0 | Type checking and JavaScript emission (D-050). |
| `@tauri-apps/api`, `@tauri-apps/cli` | 2.12.4 | Apache-2.0 OR MIT | Calls into the host; build. |
| `@base-ui/react` | 1.9.0 | MIT | Headless components (D-012). |
| `tailwindcss`, `@tailwindcss/vite` | 4.3.5 | MIT | Tokens and layout utilities only. |
| `shadcn` | 4.23.0 | MIT | CLI used once per component, at development time. |
| `react-markdown` | 10.1.0 | MIT | Markdown in the timeline (D-046). |
| `shiki` | 4.6.0 | MIT | Code coloring, loaded on demand (D-046). |
| `animejs` | 4.5.0 | MIT | Motion tiers 2 and 3, loaded on demand. |
| `three` | 0.187.0 | MIT | The signature effect of D-043, loaded on demand. |

`rmcp` is published from `modelcontextprotocol/rust-sdk`. The `tauri` crate
and `@tauri-apps/api` must stay on the same minor version.

### Build and verification

- Three gates: the Windows host, the Linux container (D-010), and GitHub
  Actions on Windows for every push and pull request (D-041).
- A pinned stable Rust toolchain in the repository; pnpm for every package
  operation.
- Logic and protocols: a test that fails without the change. Interface: a
  real window on the host. Each client adapter: a recorded run against the
  real client, listed with what it did and did not cover. Fixtures are
  synthetic.
- Packaging: a per-user installer to one fixed path, because the hook entry
  the owner adds by hand (D-028) names that path. Built by the CI gate, with
  checksums and the source commit. No automatic update in v1.

## Open

| # | Item |
| --- | --- |
| 1 | The exact text Rooom shows for each client's manual setup (D-028), and how the owner removes it again. Waits for the experiments. |
| 2 | Which free signing service to apply to, and whether Rooom qualifies (D-047). |
| 3 | The design of the signature shader effect (D-043). |
| 4 | The design of the abstract icon set (D-042). |
| 5 | `remark-gfm` for tables in Markdown: not in the approved dependency list. |
| 6 | D-033 needs comparable session permission modes. Claude, Codex and DeepSeek Harness exposed session modes. A-9 read Antigravity's command auto-execution policy. Whether that supports the session-to-session comparison remains open. |
| 10 | Antigravity and DeepSeek Harness both record a message sent from outside as the user's own words. D-033 and the MCP rules want an agent's message marked as an agent's. Either Rooom writes the sender into the text, or another call is found. |

## Experiments before the interface promises anything

Each runs on Windows against the real client and ends in a table of what was
observed, by command. Only the channels of D-038 are tried.

- **The two rows of D-037**, for each client: its requests reach Rooom, and
  Rooom's words reach it.
- **Interrupt.** The reference offers none on Windows, and synthetic key
  presses are ruled out. Claude is settled: its plugin module cancels the
  turn. For the other three, whatever each client's own interfaces offer.
- **The Claude desktop app on Windows.** The reference marks its Windows hook
  launch form unverified and looks for desktop apps only on macOS.
- **A plugin-only Claude path.** The module has a tool-call event; whether it
  can hold a call until the owner answers decides if the hook is needed.
- **Starting a session** in each client at Rooom's request (D-027).
- **Focus.** Opening the panel without taking focus (D-020), then taking it
  only when the owner clicks into the composer. Tauri 2.12 documents a
  `focusable` window option and `set_focusable`; whether Chinese input works
  right after the switch is the thing to observe.
- **One executable in three roles** (D-007). A windowed program started as a
  hook or as a stdio MCP server: whether each client waits for it and reads
  its output, that nothing else is ever written to standard output, and how
  long a cold start takes with the antivirus scan included. If it fails, the
  hook and the relay become a small console program.
- **Where packaged clients keep their files.** A client installed from the
  Store may redirect its data folders.
- **The install path** with spaces and non-ASCII characters in the user name,
  through each client's hook entry.
- **Capture exclusion** (D-044). On 2026-11-15 the owner accepted the
  Windows Graphics Capture and GDI controls in [G-13](EXPERIMENTS.md)
  for milestone 1 and waived separate trials in meeting and recording
  programs.
- **The pipe name** when the owner has more than one Windows session open.

## Milestones

Before a milestone starts it gets done criteria that a command can check.
Client apps update often: each adapter records the client version it was
verified against, reports an unknown version as unverified, and can be
switched off on its own.

1. **Experiments. Complete on 2026-11-15.** The
   [experiment table](EXPERIMENTS.md) has zero result cells marked
   "not tried". On Windows,
   `(rg -o --fixed-strings '| not tried' EXPERIMENTS.md | Measure-Object).Count`
   prints `0`. For each of the four Desktop clients both D-037 rows
   were observed. All four stay in v1, subject to the capability and
   version checks in this plan.
2. **Skeleton. Complete on 2026-11-15.** The resident program, strip and
   panel, tray, shortcut, pipe and settings passed local acceptance.
   The hosted Windows gate passed on both required events.
3. **The adapters, in parallel** (D-006), with the timeline, request cards
   and the composer.
4. **Agent messages** through MCP, with the three modes of D-033.
5. **Languages, themes, motion and the signature effect, privacy mode,
   packaging and signing.**

### Milestone 2 preparation

The first runnable slice is a Windows resident executable with a small
edge strip and panel rendered by one WebView. The tray can open the panel
or quit. The UI renders a back-end snapshot with fixture data for
verification. This slice also establishes JSON settings and the three
build gates. Milestone 2 then adds the shortcut, strip behavior and the
same-user pipe. Milestones 3–5 own adapter and client configuration,
production cards and composer, MCP tools, and distribution.

#### First batch

Proposed file layout and work order, based on D-007–D-017, D-039 and
D-041. The build and portable settings work can proceed together.
Run the Windows shell check once both are available.

| Work | Files and responsibility | First check |
| --- | --- | --- |
| Locked workspace and gates | Root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.nvmrc`, `package.json` and `pnpm-lock.yaml` pin the toolchain and dependencies. Rust 1.100.0 builds on x64 MSVC and in the Linux test image. Node 26.13.0 and pnpm 12.16.1 build the frontend. `Dockerfile` builds the portable test image. `.github/workflows/windows.yml` uses `scripts/check-windows-build.ps1` on push and pull request. | A frozen frontend install, frontend snapshot test, locked Rust workspace tests and a release executable build on Windows. Linux runs the portable tests from the same lockfile. |
| Snapshot and settings | `crates/rooom-core` owns the serializable snapshot and settings fields for edge, display, shortcut and capture exclusion. `src-tauri` owns the OS settings file and the UI bridge. Store settings only, with atomic replacement and an error for malformed input. | Test defaults, round trip and malformed input with synthetic fixtures. Test that a failed replacement leaves the original settings bytes readable. |
| Resident Windows slice | `src-tauri` is package `rooom-app`. It owns the Windows window, tray, one resident event loop and a snapshot query and update event. `src/` is the React, Vite and TypeScript frontend. Keep one WebView for the strip and panel. Use the approved CSP and default capture exclusion from the first visible window. | `scripts/check-skeleton.ps1 -Suite FirstBatch` starts a disposable instance of the real host, observes the strip and snapshot, invokes the same open and quit handlers the tray uses, and checks the process exits. |

The first batch loads `%APPDATA%\com.rooom.desktop\settings.json` on
launch. It creates defaults at the right edge of the primary display
with capture exclusion active. A saved display that is unavailable
falls back to the primary display. Malformed settings keep their bytes
and produce a notice in the panel. The strip and the tray's Open command
open the panel. Collapse returns it to the strip and the tray's Quit
command ends the process.

#### Complete the skeleton

- Complete the edge and panel behavior (D-019, D-020). Use the same Rust
  transition for a tray open, shortcut open and automated fixture event.
  Verify that an incoming fixture request shows the panel without taking
  the foreground. Verify retraction after leaving, retention while
  focused or pinned, and that a registered shortcut opens the panel.
  Test a competing shortcut registration so a conflict is reported and
  the tray still works. Use a fixture binding for the gate. See the
  [Tauri shortcut interface](https://v2.tauri.app/plugin/global-shortcut/).
- Add `crates/rooom-platform` for Windows pipe transport and resident
  handover. The reference at `d14bc43` selects by account SID and
  canonical data path and hands over to a resident holding a file lock.
  D-049 uses the account SID and actual Windows session ID for both
  resident ownership and pipe routing, backed by G-18. On the interactive
  host, launch twice within one session and verify the original resident
  PID survives, its panel opens, and the second process exits. In an
  isolated Windows environment, use the same account in two different
  Windows sessions and verify simultaneous resident endpoints. Each
  health request must return the expected resident PID and Windows
  session ID even while the other endpoint is live. Also verify a
  first-instance collision, a rejected remote or unauthorized caller,
  and a bounded return on a missing server, disconnect or malformed
  input. The skeleton exposes health and resident handover only.
  The listener rejects remote clients and limits local access to the
  owner SID. It also checks the caller's kernel session ID. Share
  `settings.json` across sessions and give each session a separate
  WebView2 user data directory.
  [WebView2 user data folders](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder#how-many-udfs-are-created)
  [Reference pipe](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/os/windows/pipe.rs)
  [Reference instance lock](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/instance/mod.rs)
- Restart the resident with disposable settings. Read back the saved
  edge, display, shortcut and capture preference, and verify the actual
  window affinity at its default and after a toggle. Remove the fixture
  settings, window and process after the check. Keep native window checks
  on the interactive Windows host; use the same handlers in the test
  harness and observe the actual foreground and lifecycle through Win32.

#### Planned acceptance commands

Run these from the Rooom root. `FirstBatch` covers the first runnable
slice. `Full` covers the remaining skeleton work when it lands.
Each check exits nonzero on a missing or failed required case.

| Gate | Command | Passing evidence |
| --- | --- | --- |
| Windows build and tests | `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Verifies the Rust pin and `x86_64-pc-windows-msvc`, then runs `pnpm install --frozen-lockfile`, `pnpm test`, `pnpm exec tauri build --no-bundle --target x86_64-pc-windows-msvc -- --locked` and `cargo test --workspace --locked --target x86_64-pc-windows-msvc`. Produces `target/x86_64-pc-windows-msvc/release/rooom.exe`. [Tauri CLI](https://v2.tauri.app/reference/cli/#build) |
| Linux portable gate | `docker build --target test -t rooom-m2-core-gate .` then `docker run --rm rooom-m2-core-gate` | The container runs `cargo test --locked -p rooom-core`. Settings and snapshot tests are discovered and pass. |
| Windows resident | `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch`, then `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite Full -SessionProof <guest-proof.json>` | The first suite proves the window and tray lifecycle and snapshot delivery. The full suite checks passive show, timed retraction, pin and focus, shortcut registration and conflict, capture affinity across restart, pipe failure cases, and same-session handover. The fixture invokes the shortcut's Rust handler after confirming registration. |
| Isolated session routing | Build with `cargo rustc --locked -p rooom-platform --release --target x86_64-pc-windows-msvc --example pipe_fixture -- -C target-feature=+crt-static`. Copy the resulting `pipe_fixture.exe` and `scripts/check-pipe-sessions.ps1` to a disposable Windows guest. In the elevated guest console, run `powershell -NoProfile -File check-pipe-sessions.ps1 -Fixture .\pipe_fixture.exe -Proof .\guest-proof.json -Isolated`. | The production transport serves concurrent instances in a console session and a scheduled, noninteractive session of the same account. Each health response matches the expected PID and session ID. A caller from the other session cannot use the explicit pipe name, and an ordinary pipe open by LocalSystem receives Win32 error 5. Full verifies the receipt against the fixture executable's SHA-256. The fixture links the MSVC runtime statically for clean guests. |
| GitHub Actions | `gh run list --workflow windows.yml --commit (git rev-parse HEAD) --json headSha,event,status,conclusion` | The Windows job succeeds for the checked commit on the required push and pull-request events. Hosted evidence follows the owner's separate authorization for remote writes. [Windows runner image](https://github.com/actions/runner-images) |

First-batch local verification passed on 2026-11-15. The Windows gate
built the release executable and passed eight Rust tests and one
frontend snapshot test. The Docker gate passed four core tests.
`FirstBatch` exercised default, saved and malformed settings. Win32
confirmed the configured edge and capture affinity, and the WebView
reported strip, panel and strip again in the same window. Each case
exited successfully through the tray quit handler. The Windows
workflow passed local `actionlint` during the first batch.
D-050 now uses `tsc` 7.1.0-beta to check and emit all frontend source in
`src/`. Vite bundles the emitted JavaScript. The snapshot test imports
the compiler output, and the development server recompiles edited
`.ts` files. The Windows build and all three `FirstBatch` cases passed
again with this compiler.

The remaining skeleton's 2026-11-15 Windows release build passed 14
Rust tests and one frontend test. The Docker gate passed all six
portable core tests, and the three `FirstBatch` profiles passed.
The interactive part of `Full` observed passive edge reveal and a
passive pipe wake without a foreground change, three-second retraction,
pin and native focus retention, real shortcut registration and conflict,
tray fallback, and the capture affinity changing from 0 to 17 after
restarting the same profile. In the duplicate launch case the new
process exited 0, and the original PID and HWND opened the panel.
The focus check grants the disposable resident native activation
permission through a short debugger attachment, then invokes its
focus handler. The shortcut check invokes the registered handler
through a fixture. The window bounds change immediately, and the
panel content slides in. Native transition motion stays in milestone 5.

The full resident command exited 0 with a receipt from an isolated
Windows Server 2025 guest. The same SID held both pipe listeners at
once in console session 1 and scheduled noninteractive session 0.
Each returned its own PID and Windows session ID. The listeners
rejected requests to the other session's explicit endpoint, and the
LocalSystem probe received `ERROR_ACCESS_DENIED` (5). The full command
matched the guest receipt to the SHA-256 of the statically linked
probe. This check covers the transport across two Windows sessions.

Hosted Windows verification passed for implementation commit
`fc055864d180919eefde7932be54e9ed1b92f98a` on 2026-11-15. Both
`build` jobs reported success for the `Build and test` step of
`.github/workflows/windows.yml`.

- `push` on `verify/milestone-2`.
  [Windows run 43684743144](https://github.com/ShanireZ/Rooom/actions/runs/43684743144)
- `pull_request` on [draft PR #1](https://github.com/ShanireZ/Rooom/pull/1).
  [Windows run 43684763201](https://github.com/ShanireZ/Rooom/actions/runs/43684763201)

### Milestone 3 first batch

The owner started this batch on 2026-11-15, with all four adapters in
one fixture gate. Later installed-channel work is recorded below, including
Codex native approvals under D-052–D-053 and Harness history under D-054.

Discover one existing session per Desktop adapter in D-006. Surface
one pending owner request and deliver one message to that session
through each adapter's fixture. Connect the resident snapshot to the
timeline, request cards and composer.
Each adapter can be exercised against disposable inputs. Live version
verification is a separate step before enabling that adapter for an
installed client.

#### Shared work

1. In `crates/rooom-core/src/inbox.rs`, key a session by client and
   source session ID. Key a request by session and source request ID.
   Apply session, request and delivery events to the resident snapshot.
   Repeated events must leave one session or card. The first confirmed
   answer closes the card. An enqueue receipt yields `queued` and
   advances to `received` or `started` only with client evidence. If a
   send may have reached the client and lacks confirmation, record
   `uncertain` and do not resend. Persist receipt IDs and state under
   D-017. Restrict delivery files to the owner, delete a file when its
   message is taken, and expire it after one hour under D-036.
2. Put each native parser in `crates/rooom-clients/src/` and each
   Windows channel in `crates/rooom-platform/src/adapters/`. The
   parser decodes the client's fixtures. The adapter maps that typed
   data to the core events. Record the exact supported Desktop version
   and engine version where applicable. Unknown versions and disabled
   adapters expose a reason and issue zero mutating calls. Reuse the
   owner and session checks of D-045 and D-049, and verify the hook's
   caller. Under D-028, Rooom displays setup instructions for each client.
3. Extend the snapshot bridge in `src-tauri/` and the plain TypeScript
   renderer in `src/`. Render the current session, request and delivery
   state from the core. Resolve a request only after an explicit
   owner action or a confirmed native answer. The composer sends to
   the selected session key. Show unsupported actions with a reason,
   and use the existing passive panel reveal for arrivals.

#### Adapter work that can run in parallel

The four adapters can proceed once the shared event shapes and fixture
IDs are fixed. The reference paths below are relative to `src/crates/`
at `d14bc43`. Rooom's [experiment records](EXPERIMENTS.md) describe the
observed Windows behavior.

| Desktop client | First channel and fixture | Pass condition |
| --- | --- | --- |
| Claude | Read a Code session from the process registry. Receive `PermissionRequest` through `rooom.exe --hook`. Use the plugin folder for the outgoing message (C-6, C-9, C-24 through C-27). Reference `agents/src/claude/hooks.rs` and `platform/src/claude_plugin/`. | A stale registry entry cannot create an active session. One fixture decision answers the matching hook. A missing resident returns no approval. The plugin claims one queued message for that session. |
| Codex | Read one Desktop-owned thread, surface a fixture command approval, and enqueue through the bundled `codex queue` route. Keep Desktop owner-pipe approval semantics tied to X-20 and X-22 through X-27. Reference `platform/src/delivery/codex/windows.rs` and `platform/src/clients/codex/desktop/reply.rs`. | Queue receipt targets the original thread. A terminal-only thread is ineligible. A missing owner or changed request identity produces zero writes. Command approvals stay in fixtures until Open 8 and 9 are resolved. |
| Antigravity | Verify the existing loopback service and read a waiting step. Use the matching conversation for the owner decision and outgoing message. Exercise this with a fixture endpoint (A-6 through A-11). Reference `platform/src/clients/antigravity/service.rs`. | A stale or foreign service cannot receive a write. The verified service receives one decision for the waiting step and one send for the matching conversation. Diagnostic output holds IDs and state. |
| DeepSeek Harness | Use plugin check-ins, `approval/request` and the controller's queue mode, with a disposable plugin folder (D-4 through D-6, D-8, D-18). Reference `platform/src/deepseek_plugin/agent-hud.mjs`. | A stale check-in disables sends. The request is answered once and the reply is claimed once. The first timeline entries come from plugin events. Compressed history waits on Open 7. |

#### First-batch acceptance commands

The named tests and `check-adapters.ps1` are deliverables of this batch.
Commands run from the repository root and must exit nonzero for a
missing adapter, case or required receipt.

| Command | Required result |
| --- | --- |
| `cargo test --locked -p rooom-clients --test adapter_fixtures` | All four synthetic formats parse a supported-version session, owner request and delivery receipt. Each rejects malformed input without producing a mutating action. |
| `cargo test --locked -p rooom-core --test inbox_projection` | Four fixture clients can reuse the same source IDs without colliding. Replayed requests yield one card each. A confirmed native answer removes the card once. Queue, receipt, start and uncertainty have independent assertions, including zero resend after an uncertain write. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | Start the release fixture resident on Windows with a disposable profile. Inject one source session, request and outgoing message per adapter. Assert four sessions, four matching cards, four sends to the selected targets and a receipt per send. Compare the frontend and Rust snapshots. Verify Claude history order and reconstruction after restart. Require zero Rooom foreground activation and native focusability during passive arrival. Check zero writes on disabled or unknown versions, no automatic allow after missing or timed-out IPC, byte-for-byte preservation of fixture client settings, and zero fixture message text or credentials in logs. |
| `pwsh -NoProfile -File scripts/check-focus.ps1 -Repetitions 10` | Controlled external foreground changes pass. Real and transient activations of an isolated Rooom copy fail the shared passive-focus assertion. All native control windows, processes and profiles are removed. |
| `pwsh -NoProfile -File scripts/check-harness-receipts.ps1 -Repetitions 10` | Exercise the real synthetic Harness Host with healthy publication, a transient check-in lock, a transient receipt lock and a persistent check-in lock. Require actual sharing-failure evidence, atomic recovery, exactly one controller admission and bounded failure without a false receipt. Remove all test processes and profiles. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` and the Docker gate above | Build with the locked MSVC toolchain and TypeScript compilation. Discover the new Rust tests in the workspace, and run the portable `rooom-clients` and `rooom-core` suites in Docker. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | The default, saved and malformed settings profiles still open and close the panel in the same window, retain the expected capture affinity, and exit through the tray handler. |

Before enabling real actions for a client, capture one native request
and one received message against its exact installed Desktop build.
Capture the version, IDs and result states. The shared fixture gate can
run before manual client setup. Later slices add history coverage,
session creation, cancel and interrupt where the client supports them.
MCP agent messaging remains milestone 4.

#### Fixture implementation and version boundary

The parsers in `rooom-clients` are portable. The Windows channels and
`--hook` exchange are exercised by the `first-batch-adapters` resident,
with a separate pipe name and an owner-only disposable profile. The
regular build shows the setup guidance and verification reasons; the
fixture transports do not locate, configure or send to installed apps.

| Channel | Exact fixture version pins from the experiments | Evidence exercised locally |
| --- | --- | --- |
| Claude Desktop Code | Desktop 5.11492.5 / Code 2.1.325 (C-24 to C-27) | Live fixture process identity and creation time, a native-shaped PermissionRequest over the scoped named pipe, then plugin queue, claim and start evidence. |
| Codex Desktop | Desktop 26.1111.2228.0 / engine 0.169.0-alpha.11.1 (X-19 to X-27) | Desktop ownership and original command request identity; an isolated subprocess accepts the X-5 queue arguments and returns a thread/item receipt. Only a later correlated event advances that receipt. Approval acknowledgements are synthetic. |
| Antigravity | Desktop 2.29.0 / language service 2.31.0 (A-6 to A-11) | Same-owner loopback socket and service handshake, decision for the waiting step, and the matching conversation's queue receipt. The synthetic nonce and delivery correlation fields are fixture contracts. |
| DeepSeek Harness | Desktop/Host 0.2.2-rc.3 (D-18), fixture plugin channel 1 | Plugin check-in, live event and approval, exclusive prompt claim, controller queue acceptance and distinct turn-start evidence. No compressed segment is read. |

The source reference was read at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`.
In addition to the adapter paths above, `docs/platform-architecture.md`
separates normalized state from platform channels;
`src/crates/platform/src/os/windows/{acl,pipe}.rs` gates local transport
and storage by owner; and `src/crates/agents/src/claude/{processes,registry_file}.rs`
binds registry entries to a process creation time. Rooom uses its own
implementation and synthetic records.

The inbox preserves a receipt containing only IDs and state before a
transport call. Without a matching acknowledgement it remains
`uncertain`; polling observes evidence and never replays the write.
The Claude and Harness plugin queues alone temporarily hold outbound
text. Taken text is removed; an untaken packet expires after one hour.

#### Local acceptance — 2026-11-15

| Check | Result |
| --- | --- |
| `cargo test --locked -p rooom-clients --test adapter_fixtures` | 5 passed across four formats, including bad input and mismatched identities. |
| `cargo test --locked -p rooom-core --test inbox_projection` | 3 passed: scoped identities and replays; confirmed answers and monotonic evidence; an uncertain send cannot be repeated after reloading the receipt. |
| [`scripts/check-adapters.ps1 -Suite Fixtures`](scripts/check-adapters.ps1) | Passed on Windows. The WebView DOM matched the Rust projection: four sessions, four request cards with explicit decisions, four sends to `session-1` under their respective client keys and four persisted queue receipts. Claude, Codex and Harness separately reached `received` and `started`; Antigravity remained `queued`. |
| [`scripts/check-windows-build.ps1`](scripts/check-windows-build.ps1) | Locked Windows release build, TypeScript build, snapshot store test and workspace tests passed. The fixture-only Windows tests ran in the adapter gate. |
| Docker `test` target | 16 tests passed across `rooom-clients` and `rooom-core`; the disposable image tag was removed. |
| [`scripts/check-skeleton.ps1 -Suite FirstBatch`](scripts/check-skeleton.ps1) | Passed defaults, saved bottom edge and malformed settings; each reused the same strip/panel window, retained its capture affinity and quit through the tray. |

The adapter gate also checked no foreground change on request arrival,
no hook decision on absent or timed-out IPC, stale and changed source
identity rejection, zero additional writes for disabled or unknown
versions, settings byte preservation and no fixture text or token in
diagnostics. Queues and receipts survived restart, take removed the
Claude and Harness text packets, and an expired packet was deleted
without submission. A separate Antigravity transport case exercised a
lost acknowledgement after one native-shaped write.

This closes the first batch's local fixture scope. Installed-client
verification is recorded below. Production activation, Codex live
approvals (Open 8–9), Harness compressed history (Open 7) and the later
milestone 3 actions remain open.

#### Installed-client protocol verification — 2026-11-15

These checks used the installed binaries and fresh test sessions. The
probe matched the session, request and receipt identities in memory and
kept only the comparison results here. Activation is a separate step
after the remaining checks and production adapter work.

| Installed build | Session discovery | Request | Delivery |
| --- | --- | --- | --- |
| Claude Desktop 5.11492.5 / Code 2.1.325 | In the Desktop Code tab, the registry and plugin check-in identified one local session. The entry point was `claude-desktop`. The PID matched the bundled Code executable, owner, Windows session and process creation time. | A native `PermissionRequest` supplied a prompt ID for the scoped write. The Code transcript contained the matching denial and the file was absent. | The plugin claimed the packet for that session. Its start receipt matched the delivery ID, and the transcript contained one matching input and the fixed reply. The bundled engine also passed an independent `sdk-cli` control with a loopback model. |
| Codex Desktop 26.1111.2228.0 / engine 0.169.0-alpha.11.1 | `thread/read` found the named empty test thread in its disposable workspace. The native rollout identified Codex Desktop as origin. | Deferred under Open 8–9. | The bundled `codex queue` returned the selected thread and queue item IDs. After the wake link, the same rollout recorded one matching user input, one turn start and the fixed reply, and the item left the queue. The test chat was archived. |
| Antigravity Desktop and language service 2.32.1 | The same-owner service passed the loopback handshake. Its trajectory list returned the newly created conversation with its disposable workspace and idle state. | With command review enabled by the owner, one `waitingSteps` entry matched the conversation and command. The service acknowledged one denial for that exact step. | `SendUserCascadeMessage` returned HTTP 200 with no message ID. The matching user input and a new tool/planner step in that trajectory independently established receipt and start. Both successful probe conversations and the initial model-error control were removed. |
| DeepSeek Harness Desktop Host 0.2.2-rc.3 | The installed Host in Electron Node mode started with a fresh profile. The session controller listed the created session and the plugin matched the live root agent. | The native approval service emitted one `approval/request` during the held test turn and resolved it as `rejected`. | The controller returned `accepted: true` for the selected session and its matching agent entered pre-step. The probe held it before model dispatch. Compressed history remains Open 7. |

The reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
informed these probes. In addition to the paths in the first-batch
table, `platform/src/delivery/starts/clients.rs` names a Codex thread
before handing it to Desktop, and
`platform/src/delivery/antigravity/cascade_config.rs` resolves the
conversation's model from its own input or the service default. Those
paths are under `src/crates/` in the reference.

Antigravity 2.32.1 is a new native observation. Its HTTP acknowledgement
does not carry the synthetic delivery fields used by Rooom's 2.29.0 /
2.31.0 fixture. The installed service's read-only `GetUserStatus`
response used chunked HTTP framing.

Claude Desktop Code stored `procStart` as a decimal Windows FILETIME
string. It matched the creation time of the actual Code process. The
first-batch fixture used a formatted timestamp.

After verification, the native Claude CLI removed the temporary plugin
and marketplace. Readback confirmed both registry entries were absent
and the plugin was disabled in the test workspace.

The installed-client probes exercised the scoped native channels.
They ran separately from the Rooom fixture resident. The production
version gates therefore still require adapter integration before
activation.

#### Native format compatibility — 2026-11-15

Claude registry verification compares the decimal `procStart` to the
exact creation FILETIME returned by `GetProcessTimes`. One tick of
difference rejects the entry even when the PID and executable match.
The disposable Windows hook fixture produces the same decimal format.
The owner, Windows session, executable and hook caller checks still
apply. The reference's `src/crates/agents/src/claude/registry_file.rs`
at `d14bc43` used a formatted timestamp. The installed Windows result
above supplies the FILETIME format.

The Antigravity adapter accepts the pinned 2.32.1 Desktop/service pair
in the fixture resident. The verified socket can return either a
length-prefixed or a chunked HTTP response. An empty native HTTP 200
acknowledgement records `queued` without a message ID. Rooom compares
later `GetCascadeTrajectory` reads with the pre-send trajectory. One
new exact `USER_INPUT` for the selected conversation establishes
`received`. A planner or command step in that input's turn establishes
`started`. A later turn's steps and a changed pre-send history cannot
advance that receipt. Multiple matching inputs or indistinguishable
pending sends also leave the last confirmed state in place. The existing 2.29.0 / 2.31.0
fixture pair keeps its correlated queue acknowledgement.

If an acknowledgement is lost, the send remains `uncertain` until
matching native evidence arrives. Rooom polls the conversation and
never repeats the write. It tracks pending comparisons in memory for
up to one hour. The latest confirmed receipt and any event position
already observed survive restart, but an in-progress text comparison
cannot resume after restart. Disk receipts store IDs and state under
D-017.

The reference's `src/crates/platform/src/clients/antigravity/service/message.rs`
and `src/crates/platform/src/delivery/antigravity/cascade_config.rs` at
`d14bc43` informed the single-write route and model configuration.
This slice connects the native formats to the resident fixture gate.
Installed-client activation, Codex live approvals (Open 8–9) and
Harness compressed history (Open 7) remain separate.

| Local check | Observed result |
| --- | --- |
| [Windows build and tests](scripts/check-windows-build.ps1) | Locked MSVC release build, TypeScript build, snapshot store test and Windows workspace tests passed. |
| [Adapter gate](scripts/check-adapters.ps1) with `-Suite Fixtures` | All 9 Windows platform unit tests and 5 loopback transport tests passed. The WebView showed four correctly routed sends progressing through `queued`, `received` and `started`. Restart retained the Antigravity start receipt. Stale or unrelated events, lost acknowledgements, disabled or unknown versions, and diagnostic redaction were checked. |
| Docker `test` target | 17 portable client and core tests passed, including reload of the native Antigravity queue receipt without a message ID. |
| [Resident window gate](scripts/check-skeleton.ps1) with `-Suite FirstBatch` | Default, saved and malformed settings profiles passed with the same strip/panel window, expected capture affinity and tray exit. |

#### Windows installed channels — 2026-11-15

The regular resident loads the two opted-in Windows channels and uses the
same inbox commands as the fixture resident. Each channel reads a
`live.json` under the owner-only adapter directories in
`%APPDATA%/com.rooom.desktop`. Start
Rooom once to create the folders, add the desired records, then restart
the resident. The records contain absolute paths and optional test
selectors. The regular resident keeps separate ID-only delivery ledgers
for each Windows session.

| Rooom configuration | Fields |
| --- | --- |
| `adapters/claude/live.json` | `desktopExecutable` is the Claude Desktop application executable, `engineExecutable` is its bundled Claude Code executable, and `registryDirectory` is the account's `.claude/sessions` folder. Set `workspace` to restrict discovery and hook replies to one workspace. |
| `adapters/antigravity/live.json` | `desktopExecutable` is the Antigravity application executable, `serviceExecutable` is its bundled `language_server.exe`, and `hubDirectory` is the account's `.gemini/antigravity/daemon` folder. Set `conversation` to restrict discovery and actions to one cascade ID. |

Claude Desktop 5.11492.5 with Code 2.1.325 is the pinned pair. The
registry can omit session status on startup. A host-qualified
`win32:<host>` PID domain must match the local host. Rooom checks the
registered PID, exact creation FILETIME, executable, Windows owner and
session. For hooks invoked through the system Git for Windows Bash
launcher, it checks the identity and creation order of each process
between the Rooom helper and the registered Code engine.

The owner adds a Claude `PermissionRequest` command hook pointing to the
absolute Rooom executable with the `--hook` argument. To receive messages,
the owner installs a local Code plugin with its manifest and a
`hooks/hooks.json` containing `{"modules":["./receive.ts"]}`. The
[receiver module](assets/claude/receive.ts) takes the absolute
`adapters/claude/plugin` directory as a JSON string in place of
`__ROOOM_CHANNEL_DIRECTORY__`. The plugin checks in for the current
session and writes an ID-only `taken` receipt before submitting. It
records `started` when Code accepts the submission at idle. Rooom
deletes the consumed message. A missing, timed-out or unverified hook
leaves the decision in Claude.

This slice pinned Antigravity Desktop and its language service to 2.32.1.
Rooom reads the service PID from the native hub record. The numeric
suffix of the announcement filename can identify a launcher. Before
sending a credential or request, Rooom verifies the configured service
image, its Windows owner and session, creation time and ownership of
the exact loopback connection. A permission decision sends one
native interaction for the selected waiting step. Confirmation
requires the native acknowledgement and a new read in which that
step has cleared.

A live trajectory containing a terminal turn returned a chunked
response of about 456 KiB. The pinned trajectory RPC now has a 4 MiB
wire limit. The other RPCs retain the 256 KiB limit. The delivery
comparison keeps the previous user inputs at their observed positions
in the same trajectory. A native tool step can change from generic to
its resolved type while a message waits for idle. A changed prior user
input, another trajectory or multiple matching new inputs cannot
establish receipt.

Reference behavior at `d14bc43` came from
`src/crates/platform/src/os/windows/antigravity.rs` for process-owned
loopback discovery, `src/crates/platform/src/clients/antigravity/service.rs`
for sharing the validated service, `src/crates/agents/src/hooks/windows.rs`
and `src/crates/platform/src/hooks/windows.rs` for the hook boundary,
and `src/crates/platform/src/claude_plugin/channel.rs` and
`src/crates/platform/src/claude_plugin/register.ts` for the plugin's
file and submission lifecycle. The installed programs supplied the
Windows process and native reply formats.

Codex live approvals (Open 8–9) and Harness compressed history (Open 7)
remain deferred.

The Windows check used the resident and its WebView with a disposable
profile. The WebView selected and acted on the verified requests and
sessions. The native reads independently checked the client outcomes.

| Installed pair | Discovery and request | Delivery and receipt |
| --- | --- | --- |
| Claude Desktop 5.11492.5 / Code 2.1.325 | One scoped session and its native Write request rendered. Rooom denied the request. Code recorded the matching rejected tool result and no test file was created. | The plugin produced a fresh check-in and Rooom rendered its `started` receipt. The matching queued file was removed. Code recorded exactly one matching user input and one assistant response. |
| Antigravity Desktop / language service 2.32.1 | One scoped trajectory and its native terminal permission rendered. Rooom denied the selected waiting step. The step cleared and the native command step recorded the denial. | Rooom showed `queued`, `received` and `started` for a new message. The persisted event position matched the one native user input and the following planner step in its turn. The earlier send that exhausted the old frame limit kept its last confirmed receipt. |

| Local regression | Result |
| --- | --- |
| [Windows build and tests](scripts/check-windows-build.ps1) | Locked frontend build and snapshot test, MSVC release build, and Windows workspace tests passed. |
| [Adapter gate](scripts/check-adapters.ps1) with `-Suite Fixtures` | Four sessions, four requests and four correctly routed sends reached `queued`, `received` and `started`. The Windows transport fixture covers a trajectory above the old limit, an in-place tool step update and a changed prior user input. |
| [Resident window gate](scripts/check-skeleton.ps1) with `-Suite FirstBatch` | All three starting settings profiles passed with one strip/panel window and a clean tray exit. |
| Docker `test` target | 18 portable client and core tests passed. The new native list test covers a missing title and a selected conversation with unrelated records present. |

The isolated Antigravity conversation was deleted and no longer appeared
in the service listing or its local conversation directory. Readback
confirmed removal of the temporary Claude hook, plugin and marketplace
and exit of the test engine. The verification profiles, copied
executables and temporary delivery records were removed. Windows
retained one empty workspace directory due to an open handle.

#### Codex Desktop and Harness installed channels — 2026-11-15

The regular resident can also opt in to these two Windows channels. Add
the following records under the owner-only adapter folders in
`%APPDATA%/com.rooom.desktop` and start Rooom.

| Rooom configuration | Fields |
| --- | --- |
| `adapters/codex/live.json` | `desktopExecutable` is the Store package's `app/ChatGPT.exe`. `engineExecutable` is its sibling `app/resources/codex.exe`. `dataDirectory` points to the Desktop-owned Codex home. An optional `thread` limits discovery and sends to one UUID. |
| `adapters/harness/live.json` | `hostExecutable` is the installed `DeepSeek Harness.exe`. An optional `session` limits the channel to one Harness root session. For the plugin's `channel` use the absolute Rooom `adapters/harness` directory. |

Codex Desktop 26.1111.2228.0 and its bundled 0.169.0-alpha.11.1 engine
are pinned. Rooom checks up to 128 recent rollout headers to find at
most 16 eligible Desktop rollouts. With a selected UUID, it checks only
files named for that UUID. It verifies the rollout's Desktop origin and
engine version, then asks the Desktop router which client owns that
thread. Before speaking to the pipe, Rooom verifies the actual server
executable, account and Windows session. A thread needs a live Desktop
owner to receive words.

Rooom rechecks ownership before running the bundled `codex queue`,
checks the returned thread and queue item, and wakes that thread using
the Windows Desktop URI. A `queued` acknowledgement proves admission
to the queue. A matching `UserMessage` item or native user-input event
after the pre-send file position must belong to a started turn in the
selected thread before Rooom advances the receipt. Duplicate matching
inputs remain ambiguous. In-progress correlation stays in memory and
an unconfirmed write is never automatically repeated. Codex's native
approval cards and decisions are covered by the later check under
D-052 and D-053.

Harness Desktop Host 0.2.2-rc.3 is pinned. The owner adds the
[Rooom receiver module](assets/harness/receiver.mjs) as an `insert`
entry in the Harness patch with an absolute `name` path to the module
and a `config` object containing `channel`. Rooom reads one check-in
per root session. It verifies the running Host image, account, Windows
session and that its check-in follows process creation. The plugin
waits on a native approval only while Rooom maintains a fresh
check-in for that Host and session. It consumes one staged answer
and removes request material after handling the approval.

For deliveries, the plugin removes a prompt packet before passing it
to the session controller in queue mode. Controller `accepted: true`
proves receipt. A native `agent/pre-step` message carrying the same
`source.rpcId` proves start. This path uses the running client and its
events. Harness compressed history remains deferred under Open 7.
The Harness [live session event reference](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/session#session-event-emit)
describes the event feed used in the isolated verification.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
uses `src/crates/platform/src/clients/codex/desktop_io/windows.rs`
for pipe ownership checks,
`src/crates/platform/src/delivery/codex/desktop.rs` for thread-owner
discovery and `src/crates/platform/src/delivery/codex/windows.rs` for
enqueue. Harness plugin injection, approvals and the controller route
are in `src/crates/platform/src/deepseek_plugin/agent-hud.mjs` and
`src/crates/platform/src/clients/deepseek/patch.rs`. The installed
clients supplied the exact rollout and `pre-step` evidence fields.

| Installed-client check | Result |
| --- | --- |
| Codex Desktop 26.1111.2228.0 / engine 0.169.0-alpha.11.1 | In an isolated workspace, Rooom found the Desktop-owned thread and showed the native queue acknowledgement followed by `started`. The rollout contained one matching input and one start for the same turn, followed by the fixed reply. The earlier probe exposed a `UserMessage` item on this Desktop path, so the regression checks its exact thread, turn and text. |
| Harness Desktop Host 0.2.2-rc.3 | The installed Host ran with a fresh Desktop profile in Electron Node mode. Rooom's WebView showed its listed root session and native approval, and the Host reported `rejected` after Rooom denied it. Controller admission and `pre-step` matched Rooom's delivery RPC ID. The prompt packet was gone. The test held and cancelled the turn before model dispatch. |

| Local regression | Result |
| --- | --- |
| [Windows build and tests](scripts/check-windows-build.ps1) | The locked frontend and MSVC release built. The snapshot test and Windows workspace tests passed. |
| [Adapter gate](scripts/check-adapters.ps1) with `-Suite Fixtures` | Ten Windows platform tests and five loopback tests passed. Four fixture sessions and requests rendered. All four queued sends reached `received` and `started`. |
| [Resident window gate](scripts/check-skeleton.ps1) with `-Suite FirstBatch` | Default, saved and malformed settings profiles passed with one strip/panel window and exit code zero. |
| Docker `test` target | Twenty portable client and core tests passed. Codex's new checks reject a mismatched thread or turn, missing start evidence and ambiguous input. |

The Codex test queue was empty before the isolated chat was archived.
Harness used a disposable profile and the installed Host runtime. The
verification did not change either client's usual settings. The temporary
profiles, test workspaces, copied executables and probe logs were removed.

#### Codex Desktop native approvals — implementation and acceptance

Set `"approvals": true` in Rooom's owner-only `adapters/codex/live.json`
to opt in. Rooom verifies the Desktop process, Windows account and logon
session before following the current owner. It matches the native request
to its in-progress user-reviewed turn and item, and shows the command or
complete file changes. The resident holds the first-observed deadline in
memory through repeat snapshots and owner changes (D-052).

Before sending one explicit decision, Rooom rechecks the owner and exact
native request, including the file changes in the approval card. A
confirmation requires a correlated success from that owner and a native
snapshot showing that the request cleared. An uncertain send is never
retried. D-053 limits the decisions available in Rooom.

The clean-rewrite reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
uses a follower subscription to read the owner state and directs decisions
back to that owner. The installed Desktop 26.1111.2228.0 with bundled CLI
0.169.0-alpha.11.1 exposed stream v13, turn records under
`turnHistory.history.entitiesByKey`, and a flat `result.ok: true` on the
correlated decision response.
[Reference subscription](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop/stream.rs)
[Reference replies](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop/reply.rs)
[Codex file approval cache at the tested CLI version](https://github.com/openai/codex/blob/rust-v0.169.0-alpha.11.1/codex-rs/core/src/tools/approvals.rs#L296-L309)

Installed-client acceptance on 2026-11-15 used disposable Desktop chats and
an isolated Rooom WebView2 profile. The test turns used a read-only sandbox
and user review.

| Check | Observed result |
|---|---|
| Command | Rooom displayed the native approval and confirmed allow once. Codex recorded one exit-zero command with the expected stdout. An identical command in the next turn required a new approval, and Rooom confirmed its denial. A separate command denial also cleared its native request. |
| File change | Rooom confirmed the session grant and Codex wrote the requested file. Another patch to the same path in that chat completed without a second decision. A different path in the same chat and the original path in a fresh chat each required a separate approval. Rooom confirmed both denials, and the files matched the expected state. |
| Expiry | With the production timeout, Rooom withdrew the card 600.1 seconds after the WebView first displayed it. The native request was still pending and Rooom showed no decision confirmation. A separate decline through the native owner channel received its own acknowledgement. |
| Configuration | Codex settings and rules had identical hashes before and after the run. No hook or plugin was installed. |

All five disposable chats were archived and the temporary test profiles,
workspace, copied executables and probe files were removed.

Local regression on the same source passed `scripts/check-windows-build.ps1`,
`scripts/check-adapters.ps1 -Suite Fixtures`,
`scripts/check-skeleton.ps1 -Suite FirstBatch`, and the Docker `test`
target (22 portable tests). The adapter tests cover the ten-minute
deadline across owner changes and the allowed command and file scopes.
Harness compressed history is covered by D-054 and the following work.

#### Harness compressed history — implementation and acceptance

D-054 selects `zstd 0.14.4` over `ruzstd 0.9.1`. The former handles
concatenated frames through its streaming reader and adds C compilation
to the existing MSVC and Docker gates. The latter has a pure Rust decoder
and requires the caller to handle multiple frames. The owner approved
the former before implementation.
[Decoder documentation](https://docs.rs/zstd/0.14.4/zstd/stream/read/struct.Decoder.html)
[Alternative decoder documentation](https://docs.rs/ruzstd/0.9.1/ruzstd/decoding/struct.StreamingDecoder.html)

Implementation and verification steps:

1. Add the pinned decoder to `rooom-clients`. Decode bounded native v4
   JSONL, require the exact root session header and contiguous event
   sequences, and project user and assistant text with stable message IDs.
   Verify concatenated frames, duplicate identities, version and session
   mismatches, truncation and size limits with synthetic records.
2. Extend the live Harness adapter with an optional `historyDirectory`
   pointing to the native sessions root. Read the selected live session's
   matching file, merge by message ID with plugin events, and cache only
   unchanged file observations in memory. No history record creates an
   approval or advances a delivery receipt.
3. Run the installed 0.2.2-rc.3 Desktop Host in a disposable profile.
   Verify native compressed messages, appended history, session isolation,
   repeat refresh and Rooom restart in the resident WebView. Run the host,
   adapter, portable and FirstBatch regression gates, record outcomes
   here and remove disposable material.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
reads compressed frames through a bounded streaming decoder and extracts
text blocks from Harness user and assistant records. Its code remains a
read-only design reference. The installed Host supplied the v4 header,
event sequence and message fields used by Rooom.
[Reference compressed reader](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/core/src/compressed.rs)
[Reference Harness projection](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/deepseek/transcript.rs)

To enable history on the live Harness channel, set `historyDirectory`
in Rooom's `adapters/harness/live.json` to the absolute native sessions
root (the default Harness location is `~/.dsh/sessions`). Existing
configurations without this optional field keep using plugin events.
Only sessions with a verified live Host check-in are eligible.

The reader matches a single project/session directory, the v4 root-session
header and contiguous sequence numbers. It handles concatenated frames,
includes user-authored and assistant text, and excludes reasoning, tool
blocks and runtime context. It retains the latest 256 text messages per
session. Limits are 8 MiB compressed, 32 MiB decoded, 1 MiB per record and
a 128 MiB decoder window. An unsupported newer generation, ambiguous
project match or changed file during a read is refused.

Only the file path, metadata stamp and verified message projection are
cached in memory. A read failure clears that session's cache and shows
a connection notice while the live plugin channel remains usable. The
next valid read rebuilds the messages. Recovered messages use native
per-session order, with surviving positions preserved between sessions.

Installed acceptance on 2026-11-15 used Desktop Host 0.2.2-rc.3 in
Electron Node mode, a disposable Harness home and an isolated Rooom
WebView2 profile. Deterministic user and assistant messages were appended
through the installed Session API and persisted by its native JSONL
backend. The temporary `native-acceptance.ps1` and
`native-recovery-acceptance.ps1` drivers exercised the live adapter.

| Check | Observed result |
| --- | --- |
| Native history | The WebView rendered the selected session's user and assistant message IDs in native order. A second root session's history stayed separate. |
| Append and deduplication | Two appended messages produced four ordered entries. A plugin event with the same native message ID and repeated refresh produced one copy. |
| Restart and recovery | Restarting Rooom rebuilt history. After cold startup with a truncated copy and two pre-existing plugin IDs, restoring the valid file recovered all four entries in native order. |
| Invalid source | A truncated copy, a file from another session and an unavailable replacement root each raised a notice. The verified live session and its plugin events remained available. Returning to the native source restored history. The original compressed file had identical bytes throughout. |
| Regression fixes | Failing tests reproduced cold-recovery ordering, reversed plugin IDs, stale cache after source changes, cross-session append and rolling-window eviction. The corrected parser, cache and inbox paths passed. |
| Cleanup | The isolated Host and Rooom instances exited. Test homes, sessions, workspace, history copies, source inspection files, scripts, copied executables and logs were removed. The usual Harness configuration hashes matched their pre-test baseline. |

Final local commands on the reviewed source:

| Command | Result |
| --- | --- |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend and MSVC release build, snapshot test and Windows workspace tests passed. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | Twelve platform and five loopback tests passed. Four sessions, cards, queued messages and correlated received/started receipts rendered. One attempt failed the passive-focus assertion; the isolated rerun passed. The trigger of that focus change was not established. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | Defaults, saved bottom edge and malformed settings profiles passed, with one resident window and exit code zero. |
| `docker build --target test -t rooom-m3-harness-history-final-20261115 .` then `docker run --rm rooom-m3-harness-history-final-20261115` | Twenty-seven portable tests passed. The task's temporary image tag was removed. |

The independent read-only review's findings were reproduced and fixed.
Its final inspection found no remaining actionable defect. This completes
the compressed-history work under D-054.

#### Passive-focus fixture regression — 2026-11-15

The old four-client check sampled the foreground before starting Rooom
and compared that handle with a later WebView receipt. Any outside window
switch during startup failed the assertion. The two endpoints also could
miss a temporary activation that ended before the receipt.

A disposable native control window reproduced the old failure. On the
same receipt the old assertion rejected the outside-only switch, while
Rooom's show and reveal observations kept the control window in the
foreground with `WS_EX_NOACTIVATE` set. The specific outside window from
the earlier failed run cannot be reconstructed from its old diagnostics.

The smoke build now records bounded native observations at window
creation, show, reveal, explicit focus, focus events and WebView render.
The shared `scripts/focus-proof.ps1` assertion requires the expected
boundaries, zero observed Rooom foreground activation and zero native
focusability during passive arrival. Other applications can change
foreground during the observation. The four-client gate uses this proof.

The targeted `scripts/check-focus.ps1` gate uses owned native control
windows and a disposable Rooom executable copy. It verifies the
outside-only switch, then grants foreground to that copy, exercises its
real focus entry point and returns foreground to the control. The final
receipt must reject the earlier activation even after the panel returns
to its nonactivating strip. The controls release their input-queue
attachment immediately and restore the prior foreground only while they
still own it. Every case removes its processes, windows and profile.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
shows with focus disabled and enables focus for explicit editing. Rooom
also read the locked Tauri 2.12.5, Tao 0.37.2 and Wry 0.57.2 sources.
During the probe, a foreground notification and a Tauri focus event could
name the shell while the synchronous foreground handle still belonged
to the outside window. The gate therefore correlates native foreground
and activation style at Rooom's own boundaries.
[Reference window placement](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/generic.rs)
[Reference explicit focus](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/windows.rs)
[Windows foreground handle](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow)

| Local verification | Observed result |
| --- | --- |
| `scripts/check-windows-build.ps1` | Locked frontend, MSVC release and Windows workspace tests passed. |
| `scripts/check-focus.ps1 -Repetitions 10` | Ten outside-only foreground switches passed. Ten actual activations of the isolated Rooom copy were detected after foreground returned to the control and the strip regained its nonactivating style. |
| Defect injection in a script copy | Replacing the shared assertion with an empty function made the real-activation control fail with `The focus check accepted an actual activation.` |
| `scripts/check-adapters.ps1 -Suite Fixtures` | The final normal entry-point run passed all four sessions, requests, decisions, sends and correlated received/started receipts. Twelve platform and five loopback tests passed. |
| `scripts/check-skeleton.ps1 -Suite FirstBatch` | Defaults, saved bottom edge and malformed settings profiles passed with one window and exit code zero. |
| Docker `test` target | Twenty-seven portable tests passed. The task's temporary image tag was removed. |

One earlier fixture repetition timed out waiting for the Harness
`received` receipt. Its focus assertion had passed. A diagnostic copy
capturing Host exit state, receipt stages and the last WebView state on
timeout completed three consecutive full runs without reproducing it.
The final normal gate also passed. The cause of that separate receipt
wait was unverified at that point; the Harness fixture receipt regression
below records its later reproduction and fix.

The native control windows, disposable Rooom copies and profiles were
removed by their test drivers. Probe scripts and the read-only reference
copy were removed after the results were recorded here. Existing
milestone-3 work was preserved.

#### Harness fixture receipt regression — 2026-11-15

A diagnostic four-client run reproduced the `harness received receipt`
timeout. The fixture Host had exited with code 1 after its three-argument
`File.Move` publication threw `Access to the path is denied`. The outgoing
prompt was still unclaimed, the native receipt directory was empty and
both the persisted projection and WebView remained `queued`. The stale
check-in notice appeared after the Host stopped. The passive-focus proof
passed. The diagnostics do not identify the process holding the original
file handle.

An isolated reader holding `check-in.json` without delete sharing
reproduced the same Host exit and missing receipt before the fix. Windows
requires compatible delete sharing for rename operations. The fixture's
one-shot atomic replacement could therefore terminate the Host during
an otherwise valid delivery.
[Windows file-sharing rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)

`tests/fixtures/harness/host.ps1` now stages each JSON value once and
retries only that atomic replacement on Win32 errors 5, 32 or 33, at
20 ms intervals for at most two seconds. It preserves the previous file
while blocked and removes its staging file on failure. Other errors and
persistent locks still fail the Host. Retry diagnostics contain only the
file name and error code. The controller submission is outside the retry
loop; the fixture exposes its admitted request ID and count so recovery
can prove exactly one submission.

The four-client gate now holds a non-delete-sharing check-in reader
until a real publication retry is observed, releases it, then requires
the correlated `received` and `started` WebView receipts. Its stale-source
case waits for a Host acknowledgement that check-ins have stopped before
altering the timestamp, replacing the old fixed sleep. The independent
Harness gate also locks the accepted-receipt target and proves unchanged
old bytes, successful replacement after release and no resubmission.
A persistent check-in lock must fail within six seconds, leave the prompt
unclaimed, retain the previous check-in and remove its staging file.

The reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` marks a prompt
claimed before invoking the queue-mode controller and records its result
afterward. Rooom keeps that single-submission boundary while retrying only
the synthetic Host's file publication. No reference code was copied.
[Reference Harness submission](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs)

| Local verification | Observed result |
| --- | --- |
| `pwsh -NoProfile -File scripts/check-harness-receipts.ps1 -Repetitions 10` | Forty cases passed: ten healthy, ten check-in-lock recoveries, ten receipt-lock recoveries and ten bounded persistent-lock failures. Each recovery admitted its request once and reached accepted, then started. |
| Defect injection in an isolated copy | Restoring the original one-shot `Put` made the new gate fail on the Host exit while waiting for real sharing-failure evidence. The source fixture remained unchanged. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures`, three consecutive runs | Every run passed four sessions, cards, decisions, routed sends and correlated received/started receipts, with the injected Harness check-in collision and passive-focus proof. Each also passed twelve platform and five loopback tests. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend, MSVC release build and Windows workspace tests passed. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | Completed on 2026-11-18. The default, saved bottom edge and malformed settings profiles each used one window, retained the expected capture affinity and exited with code zero. |
| `docker build --target test -t rooom-m3-harness-receipts-20261115 .` then `docker run --rm rooom-m3-harness-receipts-20261115` | Twenty-seven portable tests passed. The temporary image tag was removed. |

This slice covers synthetic isolated fixtures. Test processes exited, and
disposable profiles, probe logs, script copies and the read-only reference
copy were removed. Earlier milestone-3 work and the existing live-cleanup
handoff are preserved.

#### Claude Desktop Code text history — 2026-11-18

Under D-017, the Claude adapter now adds recent user and assistant text
to the timeline of each verified active Desktop Code session. It finds
the unique `<sessionId>.jsonl` below the `projects` directory next to the
configured registry directory. The native record must match that session
ID and the event and message roles must agree. It uses the native record
UUID for deduplication and keeps the file order. Tool results, internal
and sidechain records, copies of API requests and incomplete tail lines
are excluded from this text projection.

The reader takes the last 4 MiB, starting at a complete line, and keeps
up to 256 text entries per session. It caches the verified projection
in memory per resident. Reparse point files and ambiguous project
matches are refused. A read or validation error clears the history
projection and reports a notice while discovery, requests and delivery
remain available. The native file is opened only for reading.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
distinguishes Claude prompts and replies from tool results and API
request copies. Rooom applies its own rules to the native JSONL format
observed in C-2 and in the isolated bundled-engine check below.
[Reference Claude transcript handling](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/claude/transcript.rs)

| Check | Observed result |
| --- | --- |
| Installed Code 2.1.325, bundled with Desktop 5.11492.5 | The engine ran in isolated print mode against one loopback model call and wrote 18 native JSONL records. A temporary integration test loaded that file with Rooom's parser and recovered exactly the two expected texts in order with distinct UUIDs. A foreign session ID failed validation and the file bytes were unchanged. This verifies the installed engine format. The Desktop Code tab was not used for this check. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | The new WebView assertion failed before the adapter read history. After implementation the four-client suite passed, including the Claude prompt and reply in order, reconstruction after restart and an unchanged source hash. Thirteen Windows platform tests and five loopback tests passed. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend, MSVC release build and Windows workspace tests passed. The new tests covered duplicate events, foreign identity, partial tail, a file larger than the read window, and recovery when an ambiguous project match was removed. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | Default, saved bottom edge and malformed settings profiles passed with the expected capture affinity, the same window and exit code zero. |
| Docker `test` target | Twenty-eight portable tests passed. The temporary image tag was removed. |

The isolated engine, loopback server, profiles and temporary proof files
were cleaned up. The previously locked empty Claude workspace and its
obsolete live-cleanup handoff were also removed.

#### Codex Desktop history window — 2026-11-18

The installed-channel adapter previously sent the entire Codex rollout to
the 128 KiB fixture parser. Once a rollout exceeded that limit, Rooom
dropped its assistant history. Under D-017, Rooom now verifies the rollout
header on the read handle and projects the latest 256 complete assistant
text records from the final 4 MiB. It skips a line cut by the start of
the window and waits for a newline before reading the last record.
Native message IDs deduplicate within the window. Messages without an ID
use their absolute byte position so appends preserve their keys.

The resident caches the projection in memory by path and file metadata.
A failed read or changed thread identity clears the cached history and
reports a connection notice while the verified session remains available.
This applies to the existing assistant-text history. X-1 and X-2 record
the Desktop format. The design reference at
`d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` also projects textual
`response_item` messages with offset-based IDs when the native item has no ID.
[Reference Codex transcript handling](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/codex/transcript.rs)

The Windows regression reproduced the missing reply on a synthetic
rollout larger than 5 MiB. The same read path now returns the complete
reply, clears it for a foreign header under the same path, and recovers
both ordered replies when the matching rollout is restored. The
synthetic source bytes were unchanged by each successful read. Parser
tests also checked an incomplete tail, duplicate IDs, stable offsets and
the 256-reply cap.

| Check | Observed result |
| --- | --- |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | The locked frontend and MSVC release build passed, along with the snapshot test and 46 Rust tests. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | Four client sessions, approvals and routed sends passed the WebView2 fixture gate. Fourteen platform tests and five loopback tests passed. The large rollout test runs through the installed-channel reader with a synthetic file. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | All three profiles reused the same window, had the expected display affinity and exited successfully. |
| Docker `test` target | Thirty portable tests passed. The disposable image tag was removed. |

The later [installed Codex Desktop acceptance](#installed-codex-desktop-acceptance--2026-11-18)
also exercised a native rollout larger than the former 128 KiB limit.
Hosted CI for this change remains unverified.

#### Antigravity Desktop text history — 2026-11-18

Under D-017, the native Antigravity channel rebuilds recent text history
for the conversations listed by the verified 2.32.1 language service.
It reads `brain/<conversationId>/.system_generated/logs/transcript.jsonl`
beside the configured `daemon` directory. The session ID comes from the
service response. The reader opens the transcript read-only and refuses
reparse points in that path.

The projection takes up to 256 distinct steps from the final 4 MiB in
file order. It includes completed explicit user input and model text,
using the step index as identity. Wrapped input contributes only the
request body. System steps, client context, tool fields, thinking and
incomplete lines are excluded. The resident caches the projection by
path and file metadata in memory. A malformed or unreadable transcript
clears the affected history and raises a notice while the service
continues to supply sessions and approvals. History reads issue no
additional RPCs and provide no delivery evidence.

The format follows the observations in A-1 and A-8. At
`d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` the design reference reads
text from explicit input and planner steps of the conversation's own
JSONL file. Rooom uses its own filtering and shares the bounded Windows
file reader with Codex.
[Reference Antigravity transcript handling](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/antigravity/transcript.rs)

| Check | Observed result |
| --- | --- |
| Installed transcript format | A disposable read-only probe called Rooom's parser on existing native JSONL. The sample parsed successfully. Only aggregate counts were emitted. |
| Windows loopback test | The new test first failed because the verified conversation had no timeline entries. It now finds the ordered prompt and reply, clears the projection on malformed JSONL, and restores it without issuing a service write. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend and MSVC release build, the snapshot test and 48 Rust tests passed. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | Four-client WebView2 gate passed, including Antigravity history before and after restart and an unchanged source hash. Fourteen platform tests and six loopback tests passed. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | All three profiles retained the expected capture affinity, reused one window and exited successfully. |
| Docker `test` target | Thirty-two portable tests passed. The disposable image tag was removed. |

The full path from an installed Antigravity service to the resident
WebView and hosted CI for this change remain unverified.

#### Codex Desktop user input history — 2026-11-18

Under D-017, the verified Codex Desktop rollout now contributes user text
when its header declares paginated history. The parser reads native
`item_completed` records whose item is `UserMessage`, requires the
current thread ID and valid turn and item IDs, and uses the item ID to
deduplicate the text. It excludes non-text parts and all user-role model
input records, which can include either copies of the prompt or context
inserted by the client.

The same bounded projection keeps the latest 256 text entries, including
assistant replies, from complete records in the final 4 MiB. Legacy,
unknown and absent history modes retain assistant text. The Windows
resident and the Codex fixture use the same parser. This read path
supplies neither approval decisions nor delivery evidence.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
distinguishes user words from client context in response items. The pinned
Codex engine persists turn items in paginated history. Rooom uses the
native user-message item observed by the Desktop delivery check.
[Reference Codex transcript handling](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/codex/transcript.rs)
[Pinned Codex persistence policy](https://github.com/openai/codex/blob/rust-v0.169.0-alpha.11.1/codex-rs/rollout/src/policy.rs)

| Check | Observed result |
| --- | --- |
| Installed rollout format | A read-only probe used Rooom's parser on existing version-matched Desktop JSONL and recovered both user and assistant text. It emitted aggregate counts only. |
| Targeted Windows regression | The fixture first failed because only its reply was visible. The parser now includes one prompt before that reply. It skips foreign or invalid IDs, model-context copies and non-text parts. The installed-channel reader passed the synthetic rollout above 5 MiB. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | The locked frontend, snapshot test, MSVC release build and 50 Rust tests passed. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | Four-client WebView2 gate passed. It showed one Codex prompt followed by one reply before and after restart, with an unchanged rollout hash. Fourteen platform and six loopback tests passed. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | Three profiles passed. Each reused one window, reported the expected capture affinity and exited successfully. |
| Docker `test` target | Thirty-four portable tests passed. The temporary image tag was removed. |

##### Installed Codex Desktop acceptance — 2026-11-18

Codex Desktop 26.1111.2228.0 and its bundled engine
0.169.0-alpha.11.1 ran a single test turn in an empty, disposable workspace.
The bundled engine created and named the read-only test thread, then its
creator exited. The normal Desktop thread link opened that exact thread.
Rooom's Windows smoke build ran with its real adapters enabled and a
disposable WebView2 profile. The live Codex channel selected only the
test thread. The probe used the same composer, Windows owner checks and
delivery path as the resident.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
persists a newly named thread before handing it to Desktop. Its Windows
adapter uses the Desktop owner channel and a separate queue path.
[Reference thread setup](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/starts/clients.rs#L89)
[Reference Windows delivery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/codex/windows.rs)

| Check | Observed result |
| --- | --- |
| Ownership | The named thread persisted with a Desktop origin, a paginated header, zero turns and an empty queue. After Desktop opened it, Rooom's live adapter displayed that one owner-verified session. |
| Composer and native turn | Rooom rendered `queued` and then `started` for the same receipt. The native rollout held one matching user item, one matching assistant reply and a completed turn with the same turn ID. The queue was empty afterward. The turn made zero tool calls. |
| Window and restart | The WebView2 DOM contained exactly two ordered rows with the expected prompt and reply text. The prompt row used the native user-item ID. A fresh Rooom process rebuilt the identical rows from the same rollout, with the same source hash. The completed rollout exceeded the former 128 KiB reader limit. |
| Cleanup | Codex configuration, hooks and rules kept their original hashes. Both Rooom processes exited normally. The disposable chat was archived and verified in the native archive. The workspace, Rooom profile, executable copy and probe files were removed. |

This verifies live session discovery, delivery and transcript display.
The native approval checks are recorded separately under D-052 and D-053.
Hosted CI for this change remains unverified.

#### Codex Desktop automatic discovery — 2026-11-18

Automatic discovery previously took the newest 16 rollout filenames
before checking their headers. Newer subagent, terminal and unsupported
rollouts could use all 16 places and hide an otherwise eligible Desktop
thread. Rooom now applies the Desktop origin, version and filename
identity checks before counting a candidate. The pass ends after 16
eligible routes or 128 checked headers. The existing explicit thread
selector spends its header budget only on matching filenames. Live
ownership is still confirmed by the Desktop router.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
identifies Codex subagents using the `source` field in the rollout header.
Rooom uses its own discovery pass and retains the existing Desktop
eligibility checks.
[Reference Codex rollout classification](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/codex/live.rs)

The new Windows regression initially returned zero candidates for a
fixture whose eligible Desktop thread followed 22 ineligible files.
The same fixture now finds the Desktop thread. Other cases verify
newest-first ordering, the 16-route and 128-header limits, and exact
selection past unrelated files. The selected lookup continues to return
both paths when the same valid thread has two files, so the existing
send and approval guards can refuse the ambiguous target.

| Check | Observed result |
| --- | --- |
| `cargo test --offline --locked -p rooom-platform --features adapter-runtime --target x86_64-pc-windows-msvc --lib discovery_tests` | All three discovery tests passed using the installed adapter's candidate lookup with synthetic files. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend, snapshot test, MSVC release build and 53 Rust tests passed. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | The four-client WebView2 gate passed with 17 platform and six loopback tests. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | All three profiles passed with one window, the expected capture affinity and a successful exit. |
| Docker `test` target | Thirty-four portable tests passed. The disposable image tag was removed. |

The new automatic discovery path was verified with synthetic rollouts.
Installed Desktop testing above used an explicit thread selector.

#### Approval card detail preview — 2026-11-18

D-023 and the safety rules require a collapsed request to show the
beginning, end and length of the command. The inbox now opens each
nonempty detail in a native disclosure that starts collapsed. A preview
of at most 96 Unicode code points shows the whole detail when it fits,
or the first and last 48 code points with an omission marker. The
character count describes the full detail. Control characters and
bidirectional formatting controls are shown as escapes in the preview.
Opening the disclosure reveals the complete detail supplied to the
approval flow as plain text.

The reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
separates a request's open card from closed rows. Its tool card caps the
detail preview by visible lines. Rooom uses the head, tail and length
required by its own safety rules.
[Reference approval rows](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/packages/hud/src/permissions/ApprovalContent.tsx)
[Reference tool preview](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/packages/hud/src/permissions/PermissionToolCard.tsx)

| Check | Observed result |
| --- | --- |
| Before-and-after Windows WebView2 probe | The prior build opened the Codex detail by default and had a generic disclosure label. The updated build showed the head, tail and 511-character count for the synthetic command. Enter expanded it in a focused WebView. The complete text matched the fixture, the end could be reached by scrolling, and collapsing restored the preview. Markup remained literal and the WebView reported zero runtime errors. Deny reached the original request and removed its card. |
| `pwsh -NoProfile -File scripts/check-windows-build.ps1` | Locked frontend, snapshot test, MSVC release build and 53 Rust tests passed. |
| `pwsh -NoProfile -File scripts/check-adapters.ps1 -Suite Fixtures` | The four-client WebView2 gate passed with 17 platform tests and six loopback tests. |
| `pwsh -NoProfile -File scripts/check-skeleton.ps1 -Suite FirstBatch` | All three resident profiles passed with one window, the expected capture affinity and a clean exit. |

This slice verifies the preview, disclosure and existing decision
controls with synthetic requests. The following slice implements
D-023's return-to-session action for Codex Desktop.

#### Approval card return to Codex Desktop — 2026-11-18

A verified Codex Desktop approval card offers **Open session**. The
renderer submits the card's request key. The resident requires that
exact card to be present and marked as navigable, and rechecks its
configured rollout and live Desktop owner. It then asks Windows to
open the fixed `codex://threads/<uuid>` destination. A declined launch
reports an error on the panel. The action does not answer the approval
or send a message.

The navigation boundary follows D-023, D-038 and D-045. Only the
Rooom panel has the Tauri command, and neither the card's text nor a
WebView-supplied URL enters the launch. The fixture has no native
launch capability. The Claude, Antigravity and Harness return routes are
recorded below. The recovery path during
the isolated test is to deny the synthetic approval, archive the two test
chats and remove the disposable Rooom profile and workspaces.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
uses a typed Codex thread destination and offers a return control only
when that session can be reached. Rooom applies its own approval-card
and Windows owner checks, then opens the normal Desktop link from X-14.
[Reference destination validation](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/navigation.rs#L126)
[Reference request navigation](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/packages/hud/src/permissions/ApprovalContent.tsx#L59)

| Check | Result |
|---|---|
| Installed Codex Desktop `26.1111.2228.0` with bundled engine `0.169.0-alpha.11.1` and the Windows Rooom build with `first-batch-adapters` | An isolated read-only turn generated a real file-write approval. The foreground Desktop window showed a separate control chat before the click and the approval's original chat after one click on **Open session**. The same request ID paired with the control chat ID was rejected. The approval stayed pending until it was separately declined in Rooom. The turn completed without creating the requested file. |
| Isolation and cleanup | The initial queued trial inherited Desktop's default permissions and created a marker in the disposable workspace. The observed trial specified read-only and user review on the isolated turn. Both chat queues ended empty, the shared Codex configuration, hooks and rules matched their before-test hashes, both test chats were archived and the scratch directory was removed. |
| Windows build and tests (`scripts/check-windows-build.ps1`) | The locked MSVC release build, 55 Rust tests and the frontend snapshot test passed. The `first-batch-adapters` build also succeeded for the isolated live test. |
| Windows adapter fixtures (`scripts/check-adapters.ps1 -Suite Fixtures`) | Passed 18 platform unit tests, six loopback transport tests and the four-adapter resident fixture covering cards, routed sends, receipts and passive focus. |
| Windows resident smoke (`scripts/check-skeleton.ps1 -Suite FirstBatch`) | Passed the default, saved bottom-edge and malformed-settings profiles. |
| Portable Docker `test` target | Passed 35 portable Rust tests. The request projection test covers unsupported, mismatched and answered requests, as well as the matching pending request. |

#### Approval card return to Claude Desktop Code — 2026-11-18

D-023's **Open session** action also uses the Windows protocol registered
by the pinned packaged Claude Desktop `5.11492.5` and Code `2.1.325` pair.
The reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` resolves the
CLI session UUID against Desktop's Code session metadata, then uses
`claude://claude.ai/epitaxy/<local-id>` for an existing session. The distinct
`claude://resume` import route was only used to prepare test chats.
[Reference session lookup](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/navigation.rs#L31)
[Reference native destination](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/navigation.rs#L142)

Rooom requires a unique, non-archived Desktop Code record whose CLI UUID,
workspace and Desktop record ID agree with the live registry and record
filename. On each click it rechecks the exact pending hook and its
same-account, same-session Code process, then opens the validated fixed
destination. The card keeps its pending decision. This path needs no new
Rooom configuration and does not change Claude's settings or hooks.

Two test chats were seeded with the bundled CLI against a loopback fixture
and imported into Desktop. The owner's screenshots show their distinct
titles and fixture messages. The target screenshot followed a direct
launch of the existing-session URI. The device gate requires a separate
verification from the actual Rooom approval card. The test hook belongs
only to the disposable target workspace. The cleanup path declines the
test write, stops the isolated resident and removes the fixture workspace.
The first device run reached the isolated Rooom panel. The fixture had
written its channel configuration after launch, so its native hook listener
was absent. The resident was restarted with the configuration in place and
the isolated hook pipe was verified. The target Claude transcript still
contained only its seed after the initial wait for a test prompt. The
next run received the target's real `Write` approval and the Rooom WebView
reported the matching request card. That hook connection expired before
navigation, leaving the native Claude approval prompt in place.

The owner declined the expired prompt and submitted the test again. In
the same installed Claude Desktop and isolated Rooom build, the new live
card displayed **Open session**. Rooom rejected the control session ID
paired with that card's request ID. Opening the separate control chat
updated only its native Desktop focus record. One click on the target
card's **Open session** then updated the original chat's focus record.
The same approval stayed pending in Rooom and its test file was absent.
The resident and both disposable chats were kept for the owner's capture.

The first successful button run moved on as soon as the control record
changed, so the owner missed the intervening screen. After that Rooom
hook expired, a native Windows replay of the same fixed destinations
waited 15 seconds between selections. Windows reported Claude Desktop
as the foreground application at the control and target selections.
Claude Desktop did not own the foreground at the end of the wait, just
before the target selection. The owner's screenshot after the replay
showed the target title and the pending Write prompt.

The locked Windows MSVC release build, 56 Rust tests and the frontend
snapshot test passed. The new lookup test covers mismatched record IDs,
archived sessions, workspace mismatch, invalid input and ambiguous records.
The four-adapter Windows fixture, its 19 platform unit tests and six
loopback tests passed. The resident smoke suite passed all three settings
profiles. The Linux Docker `test` target passed 35 portable Rust tests.

#### Approval card workspace context — 2026-11-18

D-023's approval summary now shows the workspace folder name between the
client and action when the adapter can verify it. Claude reads `cwd` from
the verified Code process registry entry. Codex reads it from the matching
Desktop rollout's session metadata, including for file change approvals.
Two earlier isolated Desktop rollouts had an absolute `cwd` in that
header. The card carries the final directory component as display data.
The native decision payload has the same scope and identity checks as before.

Older metadata can omit `cwd`. Unsupported or ambiguous source values
leave the label absent. Harness check-ins have no workspace field. A
later slice uses the native approval event for Harness workspace context.
Antigravity's trajectory summaries expose `workspaces` (A-7). At this
point the installed 2.32.2 build was outside the adapter's pinned 2.32.1.
The next section records the compatibility and label work.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
uses the request's known path context near its action heading. Rooom uses
the verified workspace name in the heading and omits the parent path.
[Reference approval context](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/packages/hud/src/permissions/ApprovalContent.tsx#L139)

The WebView2 fixture failed at the new workspace-label assertion before
the implementation. The smoke observer compares rendered label text with
the Rust snapshot and retains only the IDs of cards with a label in the
diagnostic artifact.

The locked Windows release build, 57 Rust tests and the frontend snapshot
test passed. The fixture build passed 20 platform tests and six loopback
transport tests. The Windows resident rendered four cards with labels on
exactly the Claude and Codex cards. Approval, delivery, passive focus, and
diagnostic redaction assertions also passed. The resident smoke test
passed all three settings profiles. The Linux Docker `test` target passed
35 portable tests and its disposable image tag was removed.

#### Antigravity 2.32.2 and approval card workspace — 2026-11-18

The Windows adapter accepts matching Desktop and language service pairs
at 2.32.1 and 2.32.2. Windows reads the Desktop product version, matches it
to the verified bundled service announcement, and carries that version in
the native handshake and send metadata. Mixed pairs and unsupported
versions fail during discovery. The existing Windows owner, session,
process creation time and exact socket ownership checks apply to both.

Before the change, an isolated conversation on the installed 2.32.2 pair
passed the native handshake and returned its workspace in
`workspaces[].workspaceFolderAbsoluteUri`. It produced one matching held
terminal step. A one-time denial returned an empty acknowledgement and
cleared the native step. A separate native send produced an empty queue
acknowledgement followed by exactly one matching input and a planner step.
The probe used the existing local service and a disposable workspace.

Under D-023, one local Windows file URI supplies the approval card's
workspace folder name. The adapter decodes UTF-8 escapes and validates the
local drive and folder components. Missing, malformed and multi-root
metadata leave the label absent. The decision recheck binds the displayed
label and request content to the same native conversation and waiting step.
The smoke observer compares the displayed label and stores only card IDs.

At `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`, the reference acquires the
running language server, opens an isolated cascade with `workspaceUris`,
and shows known path context near an approval. Rooom keeps its existing
loopback and request identity checks and uses the verified summary field
for the label. [Reference service acquisition](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/antigravity/service.rs)
[Reference cascade setup](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/mod.rs#L101)
[Reference approval context](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/packages/hud/src/permissions/ApprovalContent.tsx#L139)

| Check | Observed result |
| --- | --- |
| Installed Desktop and service 2.32.2 with the isolated Rooom WebView2 build | Rooom discovered only the selected new cascade. Its visible approval card showed the exact workspace folder name with spaces and Chinese characters and matched the real waiting request. One denial from Rooom removed the card and was confirmed in the native trajectory. One composer send produced a `started` receipt backed by the native input position, and the input appeared in Rooom's history. An independent native read found the single matching input and a subsequent planner step. |
| Windows build and adapter fixture | The locked release build, 59 Rust tests and the frontend snapshot test passed. The Antigravity label assertion failed before implementation. Afterward the resident fixture verified three labelled cards, the 2.32.2 metadata, approval and delivery correlation, passive focus and diagnostic redaction. The 21 platform tests and six transport tests passed, including rejection for a changed workspace label. |
| Windows resident smoke | The default, saved bottom edge and malformed settings profiles each reused one window, retained the expected capture affinity and exited successfully. |
| Linux Docker `test` target | Passed 36 portable Rust tests, including the exact-version and mixed-pair cases. The temporary image tag was removed. |
| Cleanup | Both test cascades were deleted through the native service. Readback confirmed their removal from the trajectory list and per-cascade files. The isolated Rooom process and WebView2 debug listener exited and the task workspace was removed. The test used disposable conversations in the running service, with no changes to shared Antigravity settings. |

#### DeepSeek Harness approval card workspace — 2026-11-18

On installed Desktop Host 0.2.2-rc.3, two isolated root sessions returned
different absolute working directories in the native session list. Each
entry matched the `session.header.cwd` of its own agent, and both differed
from the Host process working directory. A native `approval/request`
carried the target agent and the same session header, with the header ID
matching the agent ID.

Under D-023, the receiver includes the work directory in the temporary
approval event when the header and root agent IDs match. Rooom derives the
folder name with its Windows workspace validation and displays it in the
card heading. Old receiver events and unusable paths leave the name
absent. The check-in and decision records carry no work directory.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
routes the approval sidecar by agent ID. Rooom keeps that session binding
and takes display context from the matching native session header.
[Reference Harness approval routing](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs#L277)

The end-to-end run used the installed Desktop Host in Electron Node mode,
a fresh Harness home and a real Rooom WebView2 window. The selected
session's approval card showed the exact folder name with spaces and
Chinese characters. The control session and its label were absent from
the view. One denial from the card removed it and settled the same native
request as `rejected`. The corresponding event and staged answer were
consumed. The test cancelled the turn before model dispatch.

The Harness parser fixture first failed on the new field. With the change
it accepted the field and kept parsing legacy and malformed optional
workspace values. The locked Windows release build, 60 Rust tests and
frontend snapshot test passed. The Windows fixture rendered a workspace
label on each of the four cards and passed the approval, delivery and
diagnostic redaction checks. Its 21 platform and six transport tests
passed. The resident smoke suite passed all three settings profiles.
The Linux Docker gate passed 37 portable tests.

The isolated Host, Rooom and WebView2 processes exited. The disposable
profiles, sessions, probe files and Docker image tag were removed.

#### Approval card return to Antigravity Desktop — 2026-11-18

D-023 uses the existing local Desktop window for Antigravity 2.32.2.
The isolated probe selected a control conversation, the target and then
the control again through the Desktop's built-in loopback DevTools
endpoint. The renderer showed each conversation's own marker and the
target's approval stayed waiting. A Win32 foreground request raised the
verified Desktop window. The registered shell scheme has a renderer
handler for opening the home view and passing a prompt.

The design reference at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66`
discovers the existing Desktop window and sends a fixed CDP
`Page.navigate` to `/c/<uuid>`. Rooom additionally binds the renderer's
HTTPS origin to the same verified language-service announcement that
supplied the waiting request. It uses the exact pending request key and
limits navigation to the installed version verified by this test.
[Reference Antigravity route](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/navigation/antigravity.rs)
[Reference Windows window selection](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/navigation/antigravity_windows.rs)

Navigation is confined to the existing same-owner local Desktop and its
matching service. It uses a checked local page target and a fixed route
derived from the cascade ID. Recovery for this probe is to deny the test
approval, restore the earlier Desktop route, delete the two disposable
cascades and close the isolated Rooom resident.

The card offers **Open session** when the Windows 2.32.2 Desktop has one
verified local window and one matching page in its built-in DevTools
inventory. The adapter reads the existing port file and checks the
listening and connected socket owners. The click re-reads the exact
waiting step and checks that the same page reached the requested route
before asking Windows to foreground the window. The navigation sends
neither an approval decision nor a message.

During the isolated Rooom WebView2 run, Antigravity first displayed the
control conversation. Rooom rejected the target request ID paired with
the control ID. One trusted pointer event on the target's **Open session**
button then opened the target conversation. Its own test marker rendered,
Windows reported Antigravity as foreground, and the exact native execution and
source step stayed pending on both sides. A separate denial cleared the
step and card, and the stale request key was rejected.

The initial WebView run exposed Chromium keeping the inventory socket
open after sending the response. The adapter now reads its bounded
`Content-Length` frame and has a regression test for an open connection.
Tests also cover invalid cascade IDs and ambiguous or mismatched
DevTools pages. The probe restored the previous Desktop route and
verified removal of both owned conversations from the service list and
native session files. The isolated Rooom process and its WebView2
listener exited. The probe directory and temporary Docker tag were removed.

The locked Windows release build, 63 Rust tests and the frontend snapshot
test passed. The four-client fixture passed 24 platform and six transport
tests and rendered four workspace labels. The resident smoke suite passed
all three settings profiles. The Linux Docker gate passed 37 portable tests.

#### Approval card return to DeepSeek Harness Desktop — 2026-11-18

The installed 0.2.2-rc.3 shell handles `dsh://open` as window activation.
Its session ID is selected within the renderer. An isolated launch of the
installed Desktop first selected a control Session, then a target with a
held native approval, then the control again. The renderer confirmed both
Session IDs and displayed the target title. The native approval stayed
unresolved until the probe cancelled the held turn before model dispatch.

D-023 uses the existing owner-authenticated Desktop Host connection and
the installed renderer's `uiWorkspace.openSession`. The reference plugin
at `d14bc43fe117f2f5d4e9143d5ebcf92c62225a66` uses the root agent ID to
route approval sidecar files. Rooom keeps that binding for navigation.
[Reference Harness approval channel](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs)
[Harness Desktop](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md)

The Host advertises a fresh renderer only after a local authenticated
browser-peer request. Rooom associates that Host with its actual Windows
parent process and one visible Desktop window. A click rechecks the
matching pending approval and publishes a short-lived request in the
private per-session channel. The Host forwards it while holding the same
approval, and Rooom waits for the exact renderer selection confirmation
before asking Windows to bring the verified Desktop forward. The packet
carries routing IDs, an expiry and confirmation state.

The owner can keep [the Harness receiver](assets/harness/receiver.mjs) in
the same patch entry. Its package manifest and client module must sit
beside it to provide navigation. An existing receiver without the
renderer module can still carry its approval and delivery traffic. An
unavailable window or expired request leaves navigation unavailable.

The corrected Windows build passed the end-to-end rerun with the
installed Desktop and its Host in a fresh isolated profile. Rooom
rejected a target approval ID paired with the control Session. One
trusted pointer click on the target card's **Open session** selected the
original Session and brought Harness Desktop to the foreground. The
same native approval and Rooom card stayed pending after the navigation.
A separate **Deny** returned the exact approval's native `rejected`
receipt. Rooom removed the card and rejected a subsequent request to
open it.

The live run caught the approval reader treating the volatile navigation
flag as part of the decision identity. The decision recheck now compares
the exact approval fields without that capability flag. The regression
test also rejects changed action detail and an already staged decision.

The test cancelled the held turn before model dispatch. The isolated
Desktop, Host, Rooom and WebView2 processes exited, and the two test
debug listeners closed. The test sessions, profiles and probe outputs
were removed from the disposable directory.

The corrected build passed the locked Windows build and 65 Rust tests,
including the approval identity regression. The four-client fixture
passed with 25 platform and six loopback tests. The resident smoke
suite passed all three settings profiles. The Linux Docker gate passed
38 portable tests.

#### Four-client isolated approval-card regression — 2026-11-18

The same Windows Rooom adapter build passed D-023 on all four installed
clients. Each check used separate target and control sessions. The card
showed the target workspace name. One **Open session** activation brought
the target to the foreground, and the same request stayed pending until a
separate **Deny** action.

| Client | Workspace label | Native evidence |
|---|---|---|
| DeepSeek Harness Desktop `0.2.2-rc.3` | Target folder name from the native approval event | The renderer selected the original Session, and the same approval call returned `rejected`. The held turn was then cancelled before model dispatch. |
| Antigravity `2.32.2` | `target` | The target rendered with the same waiting execution and source step. The native permission service accepted the denial, and the waiting step cleared. |
| Claude Desktop `5.11492.5` / Code `2.1.325` | `target` | The native Desktop focus record changed to the target. The live Rooom card kept its request ID. The matching Write tool result recorded `Denied in Rooom.`, and the file stayed absent. |
| Codex Desktop `26.1111.2228.0` / engine `0.169.0-alpha.11.1` | `target-workspace` | Foreground window captures showed the control and target in sequence. The same file approval stayed pending, the Desktop owner confirmed the refusal, and the scoped turn ended without the file. |

An earlier Codex request expired before the separate denial and left a
marker in its disposable workspace. The completed rerun used a fresh
filename and verified that it stayed absent. Codex and Claude test chats
were archived in their native clients. Antigravity removed both cascades
and restored its previous view. Harness used a separate Desktop profile
and home. The isolated Rooom and Harness processes exited, and the test
profiles, workspace files, and temporary probes were removed.
