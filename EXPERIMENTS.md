# Rooom — experiments

Milestone 1 of [PLAN.md](PLAN.md): what each client really offers on Windows,
found by running commands against the real client. Only the channels D-038
allows are tried. Nothing here writes a client's settings (D-028); a step
that needs a change there is done by the owner's hand and says so.

Before an experiment is designed, the reference is read for how it solves the
same problem; "How the reference does it" below records that, by path, at
commit `d14bc43` of `jazzenchen/agent-hud-open`. Rooom reads the reference
and copies nothing from it.

Every cell is one of:

- **observed** — seen by command, with the observation it points to;
- **failed** — tried, did not work, with what was tried;
- **ruled out** — not allowed by a decision;
- **not tried**.

Records were probed for structure only: file names, key names, enumerated
values and counts. No message text was read into this file.

## Versions

Runs of 2026-11-11, on Windows 11.

| Client | Version | Packaging |
| --- | --- | --- |
| Claude desktop app | 5.1231.0, running Claude Code 2.1.321 | Store package |
| Codex Desktop | 26.1103.7773.0, with `codex-cli` 0.162.1 | Store package |
| Antigravity | 2.29.0, with `agy` 1.0.3 | Per-user installer |
| DeepSeek Harness | 0.2.2-rc.1 | Per-user installer |

## Clients

The two rows in bold are the ones D-037 judges a client by.

| Capability | Claude desktop app | Codex Desktop | Antigravity | DeepSeek Harness |
| --- | --- | --- | --- | --- |
| Sessions and their state can be read | observed (C-1) | observed (X-1) | not tried | not tried |
| Replies can be read | observed (C-2) | observed (X-2) | observed (A-1) | observed (D-1) |
| The permission mode can be read | observed (C-3) | observed (X-3) | not tried | observed (D-2) |
| **Requests reach Rooom** | observed (C-6) | not tried | not tried | not tried |
| **Rooom's words reach the session** | observed (C-9) | not tried | not tried | not tried |
| An untaken message can be cancelled | not tried | not tried | not tried | not tried |
| A running turn can be interrupted | observed (C-10); also at a permission boundary (C-7) | not tried | not tried | not tried |
| A new session can be started | not tried | not tried | not tried | not tried |
| The sending session can be named for MCP | not tried | not tried | not tried | not tried |

D-037 so far: the Claude desktop app has both of its rows observed and stays
in v1. The other three are undecided.

## Across clients

| Experiment | Result |
| --- | --- |
| A windowed-subsystem program works as a stdin/stdout helper | observed (G-1) |
| A windowed helper started by the Claude desktop app as a hook | observed (C-6) |
| The same with the real Tauri-sized executable, started by each client | not tried |
| Cold start of the real executable, antivirus scan included | not tried |
| An executable under a path with a space and non-ASCII characters starts | observed (G-2) |
| The same path written into each client's hook entry | not tried |
| The host toolchain builds an MSVC executable | observed (G-3) |
| Where Store-packaged clients keep their files | observed (G-4) |
| The panel opens without taking focus, then takes it for typing | not tried |
| Capture exclusion in meeting and recording programs | not tried |
| The pipe name with two Windows sessions of one user | not tried |
| A plugin-only path for Claude permission requests | not tried |

## Observations

### Claude desktop app

- **C-1. Session registry.** `~/.claude/sessions/<pid>.json`, one file per
  live process. Keys include `sessionId`, `pid`, `procStart`, `cwd`, `name`,
  `kind`, `entrypoint`, `status`, `statusUpdatedAt`, `updatedAt`, `version`,
  and `waitingFor` on an entry that waits. Values seen: `status` of `busy`,
  `idle` and `waiting`; `kind` of `interactive`; `entrypoint` of
  `claude-desktop`. So state is readable without any hook, and the entry
  point tells a desktop session from a terminal one (D-032).
- **C-2. Session record.** `~/.claude/projects/<project>/<session-id>.jsonl`.
  Record types seen include `user`, `assistant`, `attachment`,
  `queue-operation`, `custom-title` and `system/stop_hook_summary`. Replies
  are `assistant` records. The app's own questions appear as tool-use entries
  inside them. `queue-operation` records carry an `operation` and a
  `content` key, so queued messages leave a trace in the record.
- **C-3. Permission mode.** `user` records carry `permissionMode`. The value
  seen was `auto`. A session in that mode decides most tool calls itself, so
  it would send Rooom few requests; the request experiment needs a session
  in another mode.
- **C-4. What a child process is told.** A shell started by the session has
  `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_ENTRYPOINT` (`claude-desktop`) and
  `CLAUDE_CODE_DESKTOP_APP_VERSION` in its environment. Whether an MCP
  server started by the session gets the same was not checked, so the MCP
  row stays "not tried".
- **C-5. Hooks in the Code tab.** Hooks supplied by installed plugins ran in
  a desktop session: their output appeared in it.
- **C-6. A permission request reaches a helper.** A `PermissionRequest`
  entry in a test project's `.claude/settings.json`, written as the
  reference writes it on Windows (an empty `matcher`, `command` naming an
  executable, `args`, no shell), started a windowed-subsystem logging
  program in a Code-tab session in `default` permission mode. The owner
  asked the session for five `mkdir` commands; the program was started once
  for each and answered in a fixed order:

  | Call | The helper's answer | What the session's record shows |
  | --- | --- | --- |
  | 1 | Nothing, exit 0 | The client asked in its own window; the owner allowed after about 33 s and the command ran. |
  | 2 | Allow | The command ran 0.3 s after it was requested. |
  | 3 | Allow, after holding 20 s | The command ran 20 s after it was requested. |
  | 4 | Deny, with a message | The tool result is an error carrying the helper's message; the turn went on to the next command. |
  | 5 | Deny, with `interrupt` | The tool was rejected, the record gained an interruption entry, and the turn ended. |

  Keys of the payload on standard input: `hook_event_name`, `session_id`,
  `transcript_path`, `cwd`, `permission_mode`, `tool_name`, `tool_input`,
  `permission_suggestions`, `prompt_id`, `effort`, `scratchpad_dir`. There
  was no turn identifier and no tool-use identifier. The suggestions offered
  for `mkdir` were a rule for the project's local settings, an added
  directory for the session, and a mode change for the session.

  A tool call that needed no permission, made in the same turn, did not
  start the helper.
- **C-7. Interrupting through the hook.** Call 5 above works when the session
  is waiting on a permission request, and only then. C-10 is the general way.
- **Not yet tried for Claude requests.** The same entry in the user's own
  settings file. An allow that adds a rule for the session only (D-035).
  Questions and plans, which the reference also receives through this hook.
- **C-8. Both sides can answer.** During the 20 s of call 3 the owner saw the
  desktop app show its own approval prompt as well; it went away when the
  helper allowed. So while Rooom holds a request the client's own card is
  live too, which is what the "one place answers" rule assumes. Seen by the
  owner on screen, not by command.

- **C-9. Words reach a session through a plugin module.** A test module with
  hooks on `session.start`, `turn.start` and `turn.complete` polled a
  folder once a second. A message left there as a file was claimed within a
  second (state "taken"), waited while a turn was running, and entered as a
  turn of its own when the session went idle (state "started", 21 s later).
  The model receives it framed as "The <plugin> plugin sent a message:", so
  it is not mistaken for the owner's words. A second message behaved the
  same. The id the module reads from the session equals the session record's
  file name and the registry's `sessionId`.
- **C-10. A plugin module can cancel the running turn.** The module kept the
  id its `turn.start` hook was given. When a request file appeared while a
  shell command was running, it called the turn-abort method with that id:
  the turn ended 0.4 s after the request was written, `turn.complete`
  reported the reason "aborted", and the next queued message started 0.2 s
  later. The shell command itself was not killed: the session handed it to
  the background and it ran on to its end. So this stops the turn, and a
  command already started may outlive it.
- **C-11. How the module was loaded.** As a development module written by
  the session itself and picked up by its hot reloading. Claude Code's
  `plugin validate` command accepted it and listed its hooks and calls. How
  an owner installs such a plugin for every session is not tried.
- **C-12. What the module cannot do.** Its file access reads, writes, lists
  and checks existence; it has no delete. A delivery file therefore has to be
  removed by Rooom once the module marks it taken (D-036).

### Codex Desktop

- **X-1. Thread records.**
  `~/.codex/sessions/<year>/…/rollout-<timestamp>-<thread-id>.jsonl`. Record
  types seen: `session_meta`, `turn_context`, `event_msg/task_started`,
  `event_msg/task_complete`, `event_msg/item_completed`,
  `response_item/message`, `response_item/reasoning`, `token_usage_record`.
  A turn's start and end are in the record, and `session_meta` names the
  `originator` and `source` of the thread.
- **X-2. Replies.** `event_msg/task_complete` carries `last_agent_message`;
  `response_item/message` carries each message.
- **X-3. Permission mode.** `turn_context` carries `approval_policy`,
  `approvals_reviewer`, `sandbox_policy` and `permission_profile`.
- **X-4. Surfaces that exist, not yet used.**
  - A named pipe `codex-ipc` exists while the app runs.
  - `~/.codex/thread-writer-locks/<thread-id>.lock` exists.
  - `~/.codex/.hook-trust/` exists; there is no `hooks.json`.
  - `codex --help` lists `queue` ("Queue a message for an existing session"),
    `agents`, `app-server` (experimental) and `remote-control`
    (experimental). None was run.
  - The desktop app's process is `ChatGPT.exe`, with a bundled `codex.exe`.

### Antigravity

The app was not running during the survey, so only files were seen.

- **A-1. Transcripts.**
  `~/.gemini/antigravity/brain/<id>/.system_generated/logs/transcript.jsonl`
  exists for each conversation, where the reference says it is. A line has
  the keys `type`, `source`, `status`, `step_index`, `created_at` and
  `content`; step types seen: `USER_INPUT`, `PLANNER_RESPONSE`, `GENERIC`,
  `SYSTEM_MESSAGE`, `CHECKPOINT`. Replies are the `PLANNER_RESPONSE` steps.
- **A-3. Other files.** `conversations/<id>.db` is one SQLite database per
  conversation, beside `conversation_summaries.db`, `annotations/<id>.pbtxt`
  and a `daemon/` folder. `~/.gemini/config/mcp_config.json` has an
  `mcpServers` key, and `~/.gemini/config/plugins/` exists.
- **A-2. Command line.** `agy --help` lists `--conversation <id>`,
  `--continue`, `--print`, and a `plugin` subcommand. None was run.

### DeepSeek Harness

The app was not running during the survey.

- **D-1. Session records.**
  `~/.dsh/sessions/<project>/<session>/session.v4.jsonl.zstd`: JSON lines
  compressed with Zstandard. Line types seen include `session`,
  `turn/start`, `step/start`, `step/end`, `user/message`,
  `assistant/message`, `tool/call`, `tool/result`, `approval/asked` and
  `approval/decided`. Replies are the `assistant/message` lines. Whether a
  session that is still open keeps an uncompressed file was not seen.
- **D-2. Permission mode.** The record has `permission/preset`,
  `sandbox/mode` and `approval/policy` lines.
- **D-3. Other files.** `~/.dsh/cordis.patch.yml` exists, beside
  `profiles/desktop/` and `mcp/`.

### Across clients

- **G-1. A windowed program as a helper.** A minimal Rust program built for
  the Windows subsystem (no console) read its standard input and wrote its
  standard output correctly, and its exit code reached the parent, when
  started in each of these ways: directly with an argument list and piped
  handles; through a PowerShell pipeline; through a `cmd` pipeline; through a
  Git Bash pipeline; with empty input; with no handles at all. Start to exit
  took about 15 ms at the median over 30 runs. This says nothing yet about a
  Tauri-sized executable or about how each client starts a hook.
- **G-2. Awkward path.** The same program started from a folder whose name
  holds a space and non-ASCII characters, with an argument list.
- **G-3. Host toolchain.** `rustc` with the MSVC target compiled and linked
  that program on the host.
- **G-4. Store packages.** The Claude desktop app and Codex Desktop are Store
  packages. The Claude app's roaming data folder is not at its usual place;
  it is redirected under the package. The folders the adapters read,
  `~/.claude` and `~/.codex`, are in the user profile and an ordinary program
  reads them.

## How the reference does it

What the next experiments start from. Paths are in the reference repository.

| Question | The reference's answer | Where |
| --- | --- | --- |
| Claude: is a registry entry's process still alive? | An entry counts only while the process on its `pid` started at the entry's `procStart`; a reused pid is another program. A killed process leaves its file behind. | `src/crates/agents/src/claude/processes.rs` |
| Claude: how do words get in? | A plugin module checks in to a shared folder every 5 s, claims a prompt file before submitting it, and reports taken, started or refused in the same file. | `src/crates/platform/src/claude_plugin/` |
| Claude: how is the plugin installed? | By Claude Code's own plugin-install command from a local marketplace folder; the reference only builds the arguments. | `src/crates/platform/src/claude_plugin/records.rs` |
| Codex Desktop: where do approvals and questions come from? | The desktop app's named pipe `codex-ipc`, which streams each followed thread's state; a decision is sent once to the thread's owner. The pipe's server is validated by its user identity, never by its name. | `src/crates/platform/src/clients/codex/desktop_io/windows.rs`, `desktop/` |
| Codex: how do words get in without interrupting? | The request `thread/queue/add` to a short-lived `codex app-server`; the app that has the thread open sends the item when the thread is idle. | `src/crates/platform/src/delivery/codex/mod.rs` |
| Codex: when does a hook run? | Only after the user allowed its exact command in Codex; until then Codex skips it silently. | `src/crates/agents/src/codex/hooks.rs` |
| Antigravity: where do permission requests come from? | The app's own loopback service, found from the `language_server.exe` and `agy.exe` processes of the same user and their listening ports. The process command line can hold a credential and is never logged. | `src/crates/platform/src/clients/antigravity/service.rs`, `os/windows/antigravity.rs`, `scripts/windows/antigravity-processes.ps1` |
| Antigravity: where are turns and replies? | `brain/<id>/.system_generated/logs/transcript.jsonl`; a turn starts at each `USER_INPUT` step. | `src/crates/agents/src/antigravity/transcript.rs` |
| DeepSeek Harness: how are requests and words carried? | One plugin file loaded through a marked block in `cordis.patch.yml`, sharing a folder with the app: check-ins, prompts, and `asks/<id>.json` answered by `asks/<id>.answer`, first answer wins. | `src/crates/platform/src/deepseek_plugin/`, `clients/deepseek/patch.rs` |
| DeepSeek Harness: what must never happen to its patch file? | It must stay a top-level list; an empty or comments-only file stops every profile from starting. | `src/crates/platform/src/clients/deepseek/patch.rs` |

Two of this machine's observations agree with it: the pipe `codex-ipc`
exists while Codex Desktop runs, and Antigravity's transcripts are where it
says. Two add to it: this version of Harness compresses its session files,
and Claude Code's plugin modules can cancel a running turn (C-10), which the
reference does not use.

## Needs the owner's hand

Each of these changes a client's settings or acts inside a live session, so
Rooom does not do it by itself.

1. **Claude, requests.** Done for a project-level entry (C-6). Still to do
   with the owner: the same entry in the user's own settings file.
2. **Claude, words.** Done with a development module (C-9, C-10). Still to
   do with the owner: installing a plugin for every session.
3. **Codex, words.** Run `codex queue` against a thread opened for the test.
4. **Antigravity and DeepSeek Harness.** Start each app with a test
   conversation open, so their live surfaces can be read.
