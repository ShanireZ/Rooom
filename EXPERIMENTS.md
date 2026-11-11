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
| Codex Desktop | 26.1103.7773.0, bundling `codex` 0.167.0-alpha.3.2; a separate `codex-cli` 0.162.1 is also installed | Store package |
| Antigravity | 2.29.0, its local service reporting 2.31.0; with `agy` 1.0.3 | Per-user installer |
| DeepSeek Harness | 0.2.2-rc.1 | Per-user installer |

## Clients

The two rows in bold are the ones D-037 judges a client by.

| Capability | Claude desktop app | Codex Desktop | Antigravity | DeepSeek Harness |
| --- | --- | --- | --- | --- |
| Sessions and their state can be read | observed (C-1) | observed (X-1) | observed (A-7) | observed (D-7) |
| Replies can be read | observed (C-2) | observed (X-2) | observed (A-1) | observed (D-1) |
| The permission mode can be read | observed (C-3) | observed (X-3) | observed (A-9) | observed (D-2) |
| **Requests reach Rooom** | observed (C-6) | observed (X-8) | observed (A-10) | observed (D-5) |
| **Rooom's words reach the session** | observed (C-9) | observed (X-5) | observed (A-8) | observed (D-6) |
| An untaken message can be cancelled | not tried | not tried | not tried | not tried |
| A running turn can be interrupted | observed (C-10); also at a permission boundary (C-7) | not tried | not tried | not tried |
| A new session can be started | not tried | not tried | not tried | not tried |
| The sending session can be named for MCP | not tried | not tried | not tried | not tried |

D-037: all four clients have both of their rows observed and stay in v1.

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

- **X-5. Words reach a thread through Codex's queue.** With a thread open and
  idle in the desktop app, the `codex` executable bundled with the app was
  run as `codex queue --thread <thread-id> --message <text>`. It printed
  "Queued message <id> for thread <id>." and exited 0 after 0.6 s. The
  thread's record then gained a new turn: `task_started`, the queued text as
  a `user` message, the assistant's reply, and `task_complete`, about 8 s in
  all. The app that had the thread open sent it; nothing was typed there. The
  queue lives in `~/.codex/queue_1.sqlite`. The printed message id is the
  receipt for "queued"; `task_started` in the record marks "started".
- **X-6. What the protocol offers, not yet used.** The same executable's
  `app-server generate-json-schema` lists these methods. Stable:
  `turn/interrupt` (taking `threadId` and `turnId`), `turn/start`,
  `turn/steer`, `thread/start`, `thread/resume`, `thread/list`, and the
  server requests `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval` and `item/permissions/requestApproval`.
  Experimental: `thread/queue/add` (taking `threadId`,
  `clientUserMessageId` and `input`, exactly as the reference sends it),
  `thread/queue/delete` (taking `threadId` and `queuedSubmissionId`),
  `thread/queue/list`, `update`, `reorder`, `start`, and the notification
  `thread/queue/changed`. So cancelling an untaken message, interrupting a
  turn and starting a thread each have a named method to try.
- **X-8. A permission request reaches a helper.** A `PermissionRequest`
  entry in the test project's `.codex/hooks.json`, written as the reference
  writes it on Windows (`command`, plus `commandWindows` holding a
  PowerShell encoded command that starts the executable), started the
  windowed logging program from a desktop thread whose approvals were
  reviewed by the user. The owner asked for four commands that write outside
  the workspace; the program was started once for each:

  | Call | The helper's answer | What the thread's record shows |
  | --- | --- | --- |
  | 1 | Nothing, exit 0 | The app asked in its own window; the owner allowed and the command ran, 5 s in all. |
  | 2 | Allow | The command ran; 1.3 s in all. |
  | 3 | Allow, after holding 20 s | The command ran; 21.3 s in all. |
  | 4 | Deny, with a message | The command failed in 0.5 s and the model was given the helper's message. |

  Keys of the payload on standard input: `hook_event_name`, `session_id`,
  `turn_id`, `transcript_path`, `cwd`, `model`, `permission_mode`,
  `tool_name`, `tool_input`. The tool name was `Bash` although the command
  was PowerShell. No suggested rules were offered.
- **X-9. A hook runs only after the owner allows it in Codex.** The owner's
  first run, in a thread already reviewed by the user, started the helper
  not once. The owner then allowed the hook in Codex's own interface and
  opened a new thread; that run is X-8. This is what the reference says:
  Codex skips a hook it has not been told to trust, without a word.
- **X-10. While the helper holds a request, Codex shows nothing.** During
  the 20 s of call 3 the owner saw no approval prompt in the app, unlike
  Claude (C-8). The app asks by itself only once the helper has exited
  without an answer (call 1). So for Codex a request Rooom holds can be
  answered nowhere else until Rooom lets go of it. Seen by the owner on
  screen, not by command.
- **X-11. What a Codex hook may answer.** Codex's source
  (`codex-rs/hooks/src/schema.rs`, read through its published copy on
  2026-11-11) takes `allow` or `deny` and an optional message. A rewritten
  input, added permissions or an interrupt in the answer make the hook fail.
  So through this hook there is no "allow for the session" and no interrupt.
- **X-7. Who reviews approvals.** The test thread's `turn_context` had
  `approvals_reviewer` set to `auto_review`. The reference only takes a
  Codex permission request when that field is `user`; anything else stays
  with the client. The request experiment needs a thread set that way.

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
- **A-4. How this version announces its local service.** With no window
  open, three `Antigravity.exe` processes ran, none of them listening on a
  loopback port, and there was no `language_server.exe` process, which is
  what the reference looks for. `~/.gemini/antigravity/daemon/` held eight
  files named `hub_<number>.json`, each with the keys `pid`, `httpsPort`,
  `httpPort`, `lsVersion` and `csrfToken`. Every `pid` in them had ended.
  So this version may announce its service through a file; whether a live
  one appears when a window is open is the next thing to observe. The token
  values were never read out.
- **A-6. Finding the live service.** With a window open there is one
  `language_server` process and one hub file whose `pid` is alive. Its two
  ports and its credential, used as the reference uses them, passed the
  `GetUserStatus` handshake over both HTTP and HTTPS. So on this version
  the hub file is enough: no process command line has to be read.
- **A-7. Conversations and their state.** `GetAllCascadeTrajectories`
  returned one summary per loaded conversation, with `status` (seen:
  `CASCADE_RUN_STATUS_IDLE` and `CASCADE_RUN_STATUS_RUNNING`), `stepCount`,
  `workspaces`, `lastModifiedTime` and, when something waits,
  `waitingSteps`. Polled once a second it showed each run start and end.
- **A-8. Words reach a conversation.** `SendUserCascadeMessage`, with the
  body the reference sends (the conversation id, the text, a delivery
  strategy of "when idle", and the model configuration taken from the
  conversation's own last user step), returned 200 at once. The
  conversation went to running and back to idle in 4 s, and the transcript
  gained the text and the reply. The text is recorded as the user's own
  input: nothing marks it as coming from outside the app.
- **A-10. A permission request is read and answered through the service.**
  After the owner set the app to ask before running a terminal command, a
  test command made the conversation's summary show one entry in
  `waitingSteps` about 6 s after the message was sent. The entry has a
  `stepIndex` and a `step` whose `status` is `CORTEX_STEP_STATUS_WAITING`,
  whose metadata names the tool call (`run_command`) and the source step
  (`cascadeId`, `trajectoryId`, `stepIndex`), and whose
  `requestedInteraction.permission` holds `actionDescription`,
  `resource.action`, `resource.target`, `shellName`,
  `persistSuggestionType` and `suggestedPersistPattern`. The request was
  held, then answered with `HandleCascadeUserInteraction` in the body the
  reference sends (the conversation, the source step, allow or deny, and a
  scope of "once"):

  | Run | Held | Answer | Result |
  | --- | --- | --- | --- |
  | 1 | 12 s | Allow | 200; the command ran and its folder exists. |
  | 2 | 12 s | Deny | 200; the conversation went on and the folder does not exist. |
  | 3 | 25 s | Allow | 200; the command ran. |

  Rooom learns of a request by polling, not by being called: the wait
  between the request and Rooom seeing it is at most the polling interval.
- **A-11. Both sides can answer.** During the 25 s of run 3 the owner saw
  the app show its own approval buttons as well. Seen by the owner on
  screen, not by command.
- **A-9. Without that setting no request arises.** The conversation's
  configuration carries the command policy at
  `plannerConfig.toolConfig.runCommand.autoCommandConfig.autoExecutionPolicy`,
  and here it read `CASCADE_COMMANDS_AUTO_EXECUTION_EAGER`: commands run
  without asking. Three test commands ran straight away and `waitingSteps`
  stayed empty, also when one message was sent with that policy changed:
  the app's own setting decides, not the message's.
  Each command step's metadata carried `preToolHookResults` with a decision,
  which shows the app runs hooks before a tool; that route is not explored.
- **A-5. Plugins.** `agy plugin` lists `install`, `import` ("from gemini or
  claude"), `enable`, `disable` and `validate`. Not tried.
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
- **D-4. A plugin loads from the patch file.** One more top-level
  `- insert:` item, added by the owner at the end of `cordis.patch.yml`
  while Harness was closed, naming a module file by its path and giving it
  a `config`. Before Harness was started the file was checked to be the
  original followed by that block. Harness started normally and loaded the
  module: its `apply(ctx, config)` ran, and `ctx.on` accepted listeners
  for `approval/request`, `user-questions/request`, `agent/created` and
  `agent/disposed`. A lookup of the session controller at load time found
  nothing; the reference looks it up only when it is used.
- **D-5. Approval requests reach the module.** The observe-only module was
  called four times with an approval request and passed each on untouched;
  the owner answered in Harness and the command ran. A request has the
  fields `agent`, `toolName`, `callId`, `reason`, `displayReason` (one text
  per language, eight of them) and `signal`. The agent's `id` has the form
  `session-<uuid>`, and an agent without a `parent` is a session the owner
  talks to. Three such sessions were announced by `agent/created`. Only
  field names and kinds were recorded, never values.
- **D-6. Words reach a session through the plugin.** The stage 2 module,
  asked through a control file, looked up the session controller when it
  needed it (the lookup that found nothing at load time now succeeded) and
  called its `prompt` with a request id, the session id, a mode of "queue"
  and one text item. The call resolved within a second with `accepted`
  true. The session's record then showed the text as a `user/message` and
  the assistant's reply. Three messages were queued this way. The text is
  recorded with a source of "user": nothing marks it as coming from outside.
- **D-7. The session record is live.** The compressed session file is
  rewritten as the session goes on: read seconds after a turn, it already
  ended with that turn's `turn/end`. So turns can be followed from the file
  while a session is open. Its `permission/preset`, `sandbox/mode` and
  `approval/policy` lines were `workspace-write`, `workspace-write` and
  `ask`.
- **D-8. An approval is answered by the plugin.** With "allowed-once" armed
  through a control file, the owner switched the session to its read-only
  preset and typed a request for a command. The session's record shows
  `approval/asked` and `approval/decided` with the outcome `allowed-once` in
  the same second; the module's record shows it gave that answer, and the
  command ran. The owner saw no approval prompt at all, in any mode: the
  module had answered before Harness showed one. So while a plugin answers
  at once the app's own card never appears; what the app shows while a
  plugin holds a request was not observed. Denying is not tested.
  The control case followed: with nothing armed, the next request in the
  read-only preset was passed on by the module, Harness showed its own
  prompt, and the owner allowed it there six seconds later. So a plugin that
  does not answer leaves the request to the app, which is the failure
  behaviour Rooom relies on.
- **D-9. When Harness asks.** Under the `workspace-write` preset a command
  inside the workspace ran without a request, typed or queued. Requests
  arose under the read-only preset. Under the `auto` preset the sandbox
  mode read `danger-full-access` and nothing was asked.
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
| Antigravity: how is the local service called? | An HTTP POST to `127.0.0.1` on the service's port, path `/exa.language_server_pb.LanguageServerService/<Method>`, JSON body, with the credential in a request header. A `GetUserStatus` call is the handshake that proves an endpoint. | `src/crates/platform/src/clients/antigravity/service.rs` |
| Antigravity: which calls does it use? | `GetAllCascadeTrajectories` (its `waitingSteps` are the pending permissions), `GetCascadeTrajectory`, `HandleCascadeUserInteraction` (answer), `SendUserCascadeMessage` (words), `StartCascade`, `CancelCascadeInvocation`. | `src/crates/platform/src/clients/antigravity/`, `delivery/antigravity/` |
| Antigravity: where are turns and replies? | `brain/<id>/.system_generated/logs/transcript.jsonl`; a turn starts at each `USER_INPUT` step. | `src/crates/agents/src/antigravity/transcript.rs` |
| DeepSeek Harness: how are requests and words carried? | One plugin file loaded through a marked block in `cordis.patch.yml`, sharing a folder with the app: check-ins, prompts, and `asks/<id>.json` answered by `asks/<id>.answer`, first answer wins. | `src/crates/platform/src/deepseek_plugin/`, `clients/deepseek/patch.rs` |
| DeepSeek Harness: what does the plugin look like? | One JavaScript module exporting a name, the services it needs and an `apply(ctx, config)` function. It listens to `agent/created`, `agent/disposed`, `tools/pre-execute`, `tools/execute` and `tools/result`, and queues words through the session's controller. | `src/crates/platform/src/deepseek_plugin/agent-hud.mjs` |
| DeepSeek Harness: what can a plugin break? | An error that escapes one of its timers or promises stops the whole Harness host, the desktop app's too, and the app loads the file again at every launch. Every callback has to catch its own errors. | the same file's header |
| DeepSeek Harness: what must never happen to its patch file? | It must stay a top-level list; an empty or comments-only file stops every profile from starting. | `src/crates/platform/src/clients/deepseek/patch.rs` |

Two of this machine's observations agree with it: the pipe `codex-ipc`
exists while Codex Desktop runs, and Antigravity's transcripts are where it
says. Two add to it: this version of Harness compresses its session files,
and Claude Code's plugin modules can cancel a running turn (C-10), which the
reference does not use.

## DeepSeek Harness: plan and recovery

Written before anything is done, because this experiment is the one that can
break a client. It waits for the owner's word.

**What the reference tells us** (`src/crates/platform/src/deepseek_plugin/`,
`clients/deepseek/patch.rs`). Harness loads a plugin named by a row in
`cordis.patch.yml` in its home folder. The row sits in a top-level list item
of the form `- insert:`, and gives an `id`, a `name` that is the module's
file path, and a `config`. The module exports a name, the services it needs
and `apply(ctx, config)`. It listens to `agent/created`, `agent/disposed`
and `approval/request`, lists sessions through `ctx.agents`, and queues
words through a session controller's `prompt` with a mode of "queue". This
is known only from the reference's code; Harness publishes no plugin
documentation that Rooom has read.

**What can go wrong.**

- An error escaping a timer or a promise in the module stops the Harness
  host, the desktop app included, and the app loads the module again at
  every launch.
- A patch file that is no longer a top-level list stops every profile from
  starting.
- The module runs inside Harness with Harness's own rights.

**How the experiment is kept small.** Two stages, the second only if the
first is clean.

1. *Observe only.* The module writes a check-in file when a session is
   created and, on an approval request, writes the names of the request's
   fields to a file and passes the request on untouched. It has no timer,
   keeps no promise, wraps every callback in its own error handling, and
   does nothing at all if its output folder is missing. It cannot change an
   answer.
2. *Answer and send.* Only after stage 1: answering an approval, and queuing
   one test message.

**Steps.**

1. Rooom copies the patch file to a folder of its own as a backup. It does
   not write in Harness's folder.
2. Rooom writes the module to a folder of its own and shows the owner the
   exact block to add: two comment lines around one `- insert:` item.
3. The owner closes Harness, appends the block by hand, and says so.
4. Before Harness is started, Rooom checks that the file is byte for byte
   the backup followed by that block, and nothing else.
5. The owner starts Harness and opens a test conversation in the test
   folder. Rooom reads the module's output files.

**Recovery.** If Harness does not start, or behaves oddly: close it, replace
the patch file with the backup copy (or delete the block between its two
comment lines), and start it again. The module's folder can be deleted at
any time; a row that names a missing file is the one case not yet known,
so the row is removed first and the folder second.

**Leaving.** When the experiment ends the owner removes the block, Rooom
checks that the file equals the backup again, and then deletes its module,
its output folder and the backup.

## Needs the owner's hand

Each of these changes a client's settings or acts inside a live session, so
Rooom does not do it by itself.

1. **Claude, requests.** Done for a project-level entry (C-6). Still to do
   with the owner: the same entry in the user's own settings file.
2. **Claude, words.** Done with a development module (C-9, C-10). Still to
   do with the owner: installing a plugin for every session.
3. **Codex.** Words done (X-5), requests done (X-8). Still to do: cancel a
   queued message, interrupt a turn and start a thread, through the methods
   of X-6.
4. **Antigravity.** State, words and requests done (A-7, A-8, A-10). Still
   to try: cancel, interrupt, starting a conversation.
5. **DeepSeek Harness.** Both stages done (D-4 to D-8), except a denial.
   The experiment has ended: the owner removed its block, the patch file was
   checked to hold its original lines again, and the modules were deleted.
