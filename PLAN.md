# Rooom — plan and decisions

Status: **design approved, nothing built**. No code exists. The owner settled
the decisions below and approved the design sections on 2026-11-11. What is
still undecided is listed under "Open"; what has not been observed on a real
machine is listed under "Experiments". Work starts at milestone 1 on the
owner's word.

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

All dated 2026-11-11 and chosen by the owner, most of them from options
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
| D-007 | Process architecture | One resident executable owns the inbox, the window, watching and receiving. The hook callback and the MCP relay are the same executable started with a flag (`--hook`, `--mcp`). The hook callback lives for one request; the MCP relay lives as long as the client session that started it. |
| D-008 | Back-end language | Rust. Chosen over Go, C#/.NET and TypeScript. |
| D-009 | Window shell | Tauri 2 on WebView2. Chosen over Slint, egui and native Win32/WinUI because the window is mostly text: Chinese input, selection, Markdown and code blocks. Accepted cost: the highest memory use of the four. |
| D-010 | Rust build gate | The application and all Windows-only code build and test on the Windows host with MSVC; crates without operating-system code also build and test in a Linux container. A third gate was added by D-041. |
| D-011 | Front-end framework | React 19, built with Vite and TypeScript. Chosen over Svelte 5, SolidJS and no framework. |
| D-012 | Components and styling | Headless components with hand-written component CSS; see "Front-end rules". |
| D-013 | Front-end state | The framework's own primitives plus one small hand-written store holding the snapshot the back end pushes. No state library. |
| D-014 | Animation inside the window | Native CSS and View Transitions first, then Anime.js, then Three.js; React Bits as a visual reference. The window's own slide is moved by Rust and is not part of this. |
| D-015 | Front-end conventions | The rules in "Front-end rules" apply in full, including React Compiler and Tailwind for tokens and layout only. |
| D-016 | Local IPC | A same-user named pipe between the resident program and its `--hook` and `--mcp` modes; no port. Chosen over loopback HTTP with a token and over exchanging files. Plugin channels use a shared folder, because a client plugin can only read and write files. |
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
- If Rooom is not running, a hook callback answers nothing and the client
  asks in its own window.

### Clients (expected; [EXPERIMENTS.md](EXPERIMENTS.md) says what has been observed)

| | Claude desktop app | Codex Desktop | Antigravity | DeepSeek Harness |
| --- | --- | --- | --- | --- |
| Sessions and state | Process registry (observed) | Thread records (observed) | Transcript files and the app's local service | Plugin check-ins |
| Replies are read from | Session records (observed) | Thread records (observed) | Transcript files (observed) | Compressed session files (observed) |
| Requests arrive by | `PermissionRequest` hook; the app's own questions from the session log | The desktop app's pipe `codex-ipc`, as the reference does; its public observer is Unix-only | The app's local service, as the reference does | Plugin folder |
| Words are sent by | Plugin module and shared folder | Codex's queue (`thread/queue/add`, or the `codex queue` command), or the desktop app's pipe | The app's local service | Plugin and shared folder |
| Cancel an untaken message | Expected | To find out | To find out | Expected |
| Interrupt a running turn | No documented way from outside; a denial can interrupt at a permission boundary | Not offered by the reference on Windows | Not offered by the reference on Windows | To find out |
| Start a new session | To find out | To find out | Not offered by the reference on Windows | Not offered by the reference |

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
- Not documented: any way to interrupt a turn from outside, and a session
  identifier in the environment of an MCP server. A shell started by a
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
  D-033's comparisons need the sending *session*, not only the client. How
  each client tells its relay which session it serves is unresolved; for
  Claude Code no documented environment variable carries it.

### Safety, privacy and failure rules

- **Fail toward the client.** No answer, a timeout, a crash, a closed pipe or
  a missing Rooom all mean the client asks in its own window. Nothing ever
  defaults to allow.
- **One place answers.** A request is answered by the owner in Rooom's
  interface or in the client's own window, whichever comes first; the other
  card then disappears.
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

Versions and licenses were read from the registries on 2026-11-11. Every
license read is compatible with GPL-3.0. Exact pins are set when the first
code is written.

| Dependency | Version read | License | Why |
| --- | --- | --- | --- |
| `tauri`, `tauri-build` | 2.12.5, 2.7.4 | Apache-2.0 OR MIT | Window shell (D-009). |
| `rmcp` | 3.9.0 | Apache-2.0 | Official Rust MCP SDK (D-018). |
| `tokio` | 1.54.1 | MIT | Named pipes with cancellation and deadlines. |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | Security descriptors, window styles, focus, capture exclusion. |
| `notify` | 8.2.2 | CC0-1.0 | Watching client records and plugin folders. |
| `serde`, `serde_json` | 1.0.229, 1.0.154 | MIT OR Apache-2.0 | Records, snapshot, settings. |
| `sha2`, `uuid` | 0.11.1, 1.29.2 | MIT OR Apache-2.0 | Request fingerprints and identifiers. |
| `tauri-plugin-global-shortcut` | 2.4.1 | Apache-2.0 OR MIT | The global shortcut of D-019. |
| `tauri-plugin-single-instance` | 2.5.5 | Apache-2.0 OR MIT | One resident copy; a second launch raises the first. |
| `tauri-plugin-autostart` | 2.7.0 | Apache-2.0 OR MIT | Optional start at sign-in. |
| `react`, `react-dom` | 19.3.1 | MIT | Interface (D-011). |
| `babel-plugin-react-compiler`, `@rolldown/plugin-babel` | 1.0.0, 0.3.1 | MIT | React Compiler (D-015). `@vitejs/plugin-react` 6 names both as optional peers; it no longer runs Babel itself. |
| `vite`, `@vitejs/plugin-react` | 8.3.8, 6.3.0 | MIT | Build. |
| `typescript` | 7.0.2 | Apache-2.0 | Types. |
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
| 6 | Which clients let Rooom read a session's permission mode, which one of D-033's four questions depends on. Observed so far: Claude, Codex and DeepSeek Harness do. |
| 7 | A Zstandard decoder for DeepSeek Harness's session files: not in the approved dependency list. |

## Experiments before the interface promises anything

Each runs on Windows against the real client and ends in a table of what was
observed, by command. Only the channels of D-038 are tried.

- **The two rows of D-037**, for each client: its requests reach Rooom, and
  Rooom's words reach it.
- **Interrupt.** The reference offers none on Windows, and synthetic key
  presses are ruled out. What remains: a denial that interrupts at a
  permission boundary, and whatever a client's own interfaces offer.
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
- **Capture exclusion** (D-044) in the meeting and recording programs the
  owner uses.
- **The pipe name** when the owner has more than one Windows session open.

## Milestones

Before a milestone starts it gets done criteria that a command can check.
Client apps update often: each adapter records the client version it was
verified against, reports an unknown version as unverified, and can be
switched off on its own.

1. **Experiments.** Results are kept in [EXPERIMENTS.md](EXPERIMENTS.md).
   Done when no cell there reads "not tried":
   `grep -c "not tried" EXPERIMENTS.md` prints 0. D-037 then decides which
   clients stay, from the two rows it names.
2. **Skeleton.** The three build gates, the resident program, the strip and
   panel, tray, shortcut, the pipe, settings.
3. **The adapters, in parallel** (D-006), with the timeline, request cards
   and the composer.
4. **Agent messages** through MCP, with the three modes of D-033.
5. **Languages, themes, motion and the signature effect, privacy mode,
   packaging and signing.**
