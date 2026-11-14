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
| An untaken message can be cancelled | failed in the Desktop trial (C-20); observed in the isolated engine (C-16) | observed (X-15) | observed (A-15) | observed (D-11, isolated Desktop Host) |
| A running turn can be interrupted | observed (C-10); also at a permission boundary (C-7) | observed (X-16) | observed (A-16) | observed (D-12, isolated Desktop Host) |
| A new session can be started | observed (C-15, CLI seed and Desktop import) | observed (X-14) | observed (A-14) | observed (D-10, isolated Desktop Host) |
| The sending session can be named for MCP | observed (C-14, Desktop Code tab) | observed (X-17, bundled app-server) | observed (A-13, Desktop) | observed (D-16, isolated Desktop Host) |

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
| A plugin-only path for Claude permission requests | observed (C-19, Desktop refusal) |

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
  `CLAUDE_CODE_DESKTOP_APP_VERSION` in its environment. For the MCP relay,
  C-13 covers the bundled binary in CLI mode and C-14 covers the Desktop
  Code tab.
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

- **C-13. Claude Code gives its MCP relay the CLI session ID.** In
  two independent runs, the Desktop-distributed Claude Code 2.1.321
  executable was started in CLI mode with a temporary `CLAUDE_CONFIG_DIR`,
  `--strict-mcp-config`, and an isolated local model endpoint. Each run
  used two different `--session-id` values. Each MCP tool call had empty
  arguments and its relay's `CLAUDE_CODE_SESSION_ID` matched the
  corresponding CLI session exactly. The two sessions launched separate
  relay processes. The `_meta` keys were `claudecode/toolUseId` and
  `progressToken`.
  [Reference sender metadata](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L26)
- **C-14. The Desktop Code tab gives its MCP relay the session ID.** The
  owner configured a probe in a test project. Two local Code-tab sessions
  each called `identify` once with a distinct fixed `case` marker as its
  only argument. They started separate relay processes. Both received
  `CLAUDE_CODE_ENTRYPOINT=claude-desktop`. Each relay's
  `CLAUDE_CODE_SESSION_ID` matched the ID in the filename of a separate
  project transcript. Each transcript had its own marker and only its own
  marker in the user events. The `_meta` keys were `claudecode/toolUseId`
  and `progressToken`.
- **C-15. A new local session can be created and imported into the
  Desktop Code tab.** Claude Code 2.1.325 ran in stream-JSON print mode
  with a fresh session ID, an empty test workspace and a loopback model
  fixture. The first fixed reply was recorded and the creator exited.
  Opening `claude://resume?session=<id>` through Windows made Desktop
  5.11492.5 create its own `local_<id>` record. Its CLI session ID and
  workspace matched the seed and it was marked as imported from a CLI
  transcript. The reference starts a background job and uses this
  same link after stopping the job.
  [Reference handoff](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/starts/handoff.rs#L116)
- **C-16. The bundled engine can withdraw one pending prompt.** A
  stream-JSON session on Claude Code 2.1.325 held its first model
  response on a local fixture. Two further inputs were sent with distinct
  UUIDs. A `cancel_async_message` control request for one UUID returned
  `cancelled: true` while the first reply was still held. On releasing
  the holder, the other queued prompt ran and got its fixed reply. The
  removed prompt never reached the model. Desktop 5.11492.5 implements
  a renderer-scoped `cancelQueuedMessage` handler and its SDK client
  sends this engine request. C-20 covers a separate trial of Desktop's
  queue. The reference withdraws requests from its plugin delivery
  folder before they are taken.
  [Reference queued delivery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/docs/agent-messages.md#L15)
- **C-17. A plugin can hold the bundled engine's permission decision.**
  The 2.1.325 stream-JSON engine ran with an isolated profile and
  the probe supplied through `--plugin-dir`. A module's
  `tool.check` handler awaited a loopback decision endpoint for
  11 s on each of two `Write` calls for which the engine's underlying
  decision was `ask`. Neither test file existed while the decision
  was held. With denial it remained absent, and with
  allowance a new file appeared. There was no settings-file
  `PermissionRequest` hook. The decisions were fixture responses.
  C-19 covers the Desktop Code tab's plugin dialog and refusal.
  [Reference settings-hook route](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/claude/hooks.rs#L5)
  [Claude Code mod events](https://code.claude.com/docs/en/plugins/mods/events#approve-or-refuse-a-tool-call-before-the-user-is-asked)
- **C-18. The first Desktop permission attempt used the native card.**
  On Desktop 5.11492.5 with Claude Code 2.1.325, the owner placed the
  probe under the test workspace's `.claude/skills/` and submitted
  the fixed prompt to the imported session. Its one `Write` call
  matched the expected path and content. The owner chose Deny, the
  matching tool result recorded the native refusal, and the test file
  stayed absent. The probe matched the supplied files on disk, but its
  pending and decision markers were both absent. The transcript then
  recorded a successful `/reload-plugins` command and a second matching
  `Write` request. The owner reported only Desktop's native options:
  Deny, Allow once, and Allow and switch to auto. No plugin marker
  appeared on the retry. The cause of the project copy's failure to
  intercept was not established. C-19 covers the user-directory copy.
  [Claude Code plugin reload](https://code.claude.com/docs/en/plugins/cli-reference#reload-plugins)
- **C-19. A plugin holds a tool decision in the Desktop Code tab.**
  On Desktop 5.11492.5 with Claude Code 2.1.325, the owner loaded a
  mod from the user's skills directory and reloaded plugins in the
  disposable session. The mod matched the session ID, workspace,
  and exact `Write` target and content. Its `tool.check` handler
  recorded a pending marker and awaited `$.ui.ask`. The owner saw the
  mod's question and chose to refuse. Its decision marker followed the
  pending marker by 23.32 s. The exact call had one matching error
  result carrying the mod's refusal reason; the target file was absent.
  The probe supplied only a mod module and used no settings-file
  `PermissionRequest` hook. The allow branch was exercised in the
  isolated engine in C-17.
  [Claude Code mod decision handling](https://code.claude.com/docs/en/plugins/mods/events#approve-or-refuse-a-tool-call-before-the-user-is-asked)
- **C-20. A Desktop queue withdrawal did not prevent delivery in this
  trial.** On Desktop 5.11492.5 with Claude Code 2.1.325, the owner
  reported removing DROP from the queue and receiving the KEEP reply.
  The test transcript had a separate enqueue record for each marker
  while the previous turn was still running, followed by two dequeues.
  One subsequent user message contained both prompts. Its message
  blob contained both markers and was referenced by the next SDK
  model request. A descendant assistant message replied with the
  KEEP marker. The precise timing of the owner's removal action
  relative to dequeue was unavailable, so the trial does not establish
  which part of the Desktop queue path allowed DROP through.

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
- **X-12. Isolated Windows app-server: thread creation and queued-message
  removal.** In a fresh `CODEX_HOME` with `codex-cli` 0.162.1, the probe
  connected over stdio and opted in with `experimentalApi: true`. With
  `ephemeral: false`, `thread/start` returned a thread id. After the
  creator exited, `thread/queue/add` failed with `no rollout found` for
  the new unnamed thread. For a second thread, calling `thread/name/set`
  before the creator exited made the thread readable with zero turns and
  let a separate process add two queued messages. A third process listed
  both, got `deleted: true` when deleting one by its `queuedSubmissionId`,
  and could not delete that id again. After it exited, a fourth process
  still listed only the other message. This checks the persistence step
  in the reference's
  [start sequence](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/starts/clients.rs#L61).
  The bundled executable repeated these isolated results. X-14 and X-15
  cover the Desktop runs.
- **X-13. Isolated Windows app-server: interrupt is local to the turn's
  owner in this test.** In the same isolated setup a synthetic turn
  reached `inProgress` with the provider pointed at an unavailable
  loopback endpoint. A second app-server sharing the same home rejected
  `turn/interrupt` for the exact thread and turn. The owner app-server
  accepted it and emitted `turn/completed` with status `interrupted` for
  that turn. The reference
  [Windows adapter](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/codex/windows.rs)
  exposes no interrupt. This probe never connected to the Desktop
  owner pipe. X-16 covers an interrupt through that pipe.
- **X-14. A new thread opens in Codex Desktop.** On Desktop
  26.1103.7773.0 with its bundled `codex-cli` 0.167.0-alpha.3.2,
  `thread/start` and `thread/name/set` made one thread in a test folder.
  Queuing while the creator still held the thread let that process take the
  first message. Its turn ended interrupted when the creator exited. For
  the second attempt, a separate app-server queued the test message after
  the creator exited, following the reference's start sequence.
  After `codex://threads/wake/<id>` the app reported the thread idle,
  but owner discovery returned `no-client-found` and the message
  remained queued. The normal `codex://threads/<id>` link then made
  owner discovery succeed. The discovered owner accepted
  `thread-follower-start-turn` (version 2) for the new thread and
  returned its turn id. Starting a thread and opening it in Desktop
  worked. X-18 covers a dedicated wake-only run.
  [Reference start sequence](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/starts/clients.rs#L61).
- **X-15. An untaken message can be withdrawn from a Desktop-owned
  thread.** While the X-14 test turn ran, a second app-server added two
  messages to its queue. A `thread-follower-remove-queued-message`
  request got a success envelope, but both items still appeared in the
  next `thread/queue/list`. With Desktop still owning the thread after
  the turn stopped, an app-server called `thread/queue/delete` with
  the exact `queuedSubmissionId`. It returned `deleted: true` and a
  fresh process listed only the control item. The thread record showed
  zero turns from the cancelled message. The control item was then removed, and a
  fresh process confirmed the test queue was empty.
- **X-16. The Desktop owner can interrupt a running turn.** The pipe
  server's process token matched the caller's user and its executable
  was the installed Desktop app. The reference's Windows adapter offers
  start, steer and queue operations. This Desktop version registers `thread-follower-interrupt-turn`
  in the installed app archive (`.vite/build/early-bootstrap.js`
  and its renderer bundle). A version 4 request to the discovered
  owner named the conversation and its exact `expectedTurnId` with
  mode `system`. The owner replied successfully. Desktop read-back
  reported that exact test turn `interrupted` after 3.2 s.
  [Reference Windows adapter](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/codex/windows.rs).

- **X-17. The Desktop app-server gives the MCP relay thread and turn
  identity.** Two independent runs started the executable bundled with
  Codex Desktop 26.1103.7773.0 (`codex-cli` 0.167.0-alpha.3.2) in an
  isolated `CODEX_HOME`, using a local model fixture. Two ephemeral
  threads each ran one turn and invoked the same stdio MCP tool with
  empty arguments. In all four calls, `_meta.threadId` and
  `_meta["x-codex-turn-metadata"].turn_id` matched the IDs returned by
  `thread/start` and `turn/start`. Both turns completed in each run.
  The test used the bundled engine over stdio.
  [Reference sender metadata](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L26)
- **X-18. A wake-only link delivers the first queued message.** With
  Windows Codex Desktop 26.1103.7773.0 already running, two trials each
  created and named a new empty thread using the bundled `codex-cli`
  0.167.0-alpha.3.2. The creator exited before a second app-server
  enqueued one fixed text message and exited. After an 8 s baseline,
  each queue held one message and its rollout had zero turns. The
  probe opened only `codex://threads/wake/<id>` through Windows. The
  first `task_started` and the matching user message were visible in
  the rollout 4.1 s and 3.2 s after the wake command returned. Both
  turns returned their exact reply markers, recorded `task_complete`,
  and left empty queues. Both test threads were archived through
  Desktop after measurement.
  [Reference start sequence](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/starts/clients.rs#L89)

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
  `--continue`, `--print`, and a `plugin` subcommand. A-12 covers the
  isolated attempt with `--print`.
- **A-12. The isolated launch stopped before a tool call.** The
  installed `agy` 1.0.3 listed the probe's MCP tool but fell back to
  account sign-in before the conversation reached the local model
  fixture. The Desktop language server 2.29.0 in standalone mode also
  waited for sign-in. No `tools/call` was observed. The installed
  language server contains the `antigravity.google/conversation_id`
  metadata key. A-13 covers the live Desktop calls.
  [Reference sender metadata](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L26)
- **A-13. Antigravity Desktop identifies callers on a shared relay.** The
  owner configured a local probe. Two Desktop conversations each called
  `identify` once with a distinct fixed `case` marker as its only argument.
  Both calls reached the same relay process. The values of
  `_meta["antigravity.google/conversation_id"]` differed and each matched
  the directory ID of a transcript whose `USER_INPUT` events contained only
  its own test marker. `_meta` also had `antigravity.google/root_conversation_id`,
  `antigravity.google/artifacts_dir` and `progressToken`.
- **A-14. A session can be started through the Windows Desktop service.**
  Antigravity Desktop and its hub both reported 2.31.0. The probe confirmed
  that the loopback listener belonged to the installed language server
  and verified it with `GetUserStatus`. `StartCascade` returned HTTP
  200 and a fresh ID using the interactive cascade source and the file
  URI of an empty test folder. `GetAllCascadeTrajectories` listed the ID
  as idle with the expected workspace. `SendUserCascadeMessage` then
  ran its first turn, and the transcript recorded one matching input and
  one reply with the requested marker.
  [Reference start sequence](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/mod.rs#L101)
- **A-15. One pending message can be withdrawn.** While a test turn
  was running, two distinct text messages were sent with
  `MESSAGE_DELIVERY_STRATEGY_WHEN_IDLE`. `GetCascadeTrajectory` omitted
  pending inputs during the baseline run. On the installed 2.31.0
  Desktop service, `StreamAgentStateUpdates` exposed both messages
  under `pendingAgentMessagesUpdate`. The probe matched each entry to
  its fixed test marker. The installed Desktop front end sends
  `DeleteAgentMessage` with the recipient cascade and the pending
  entry's ID. The same call returned HTTP 200 and a fresh stream
  snapshot showed only the control entry with its original ID while the
  holder turn was still running. After delivery, the transcript contained
  zero user steps for the withdrawn marker and one user step and one
  reply for the control. The reference's Windows path sends the
  when-idle strategy to the same owner service.
  [Reference Windows send path](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/windows.rs#L23)
- **A-16. A running invocation can be interrupted through the Windows
  Desktop service.** The probe waited for the isolated test turn's input
  and `CASCADE_RUN_STATUS_RUNNING`, then sent
  `CancelCascadeInvocation` with its cascade ID and an empty queue. The
  service returned HTTP 200. The next read, 76 ms after starting the
  cancel call, showed `CASCADE_RUN_STATUS_IDLE` with
  `EXECUTOR_TERMINATION_REASON_USER_CANCELED`. The stopped turn added no
  planner reply. This measured interruption of the invocation. The
  probe did not measure termination of an already running terminal
  command. The reference issues this RPC on Unix. The Windows test
  addressed the verified Desktop owner directly.
  [Reference cancel call](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/unix.rs#L12)

### DeepSeek Harness

The initial file survey ran with the app closed.

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
  plugin holds a request was not observed in this run. D-15 covers both
  the hold and a denial in the Desktop window.
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
- **D-10. The Desktop Host can create a session.** Two runs used the
  installed 0.2.2-rc.1 Desktop executable in Electron Node mode with its
  bundled `@deepseek-ai/dsh-desktop-host/lib/index.js`. The host reported
  `ready` over IPC with a fresh Harness home and a synthetic desktop
  profile composed from the shipped `dsh-base` and `dsh-web-app` bundles.
  A probe loaded from the isolated patch waited for `sessionController`,
  called `create({ cwd })`, and got a `session-<uuid>` for the test
  directory. The controller admitted a queued prompt into that session.
  The probe held the real turn at `agent/pre-step` so the run needed
  neither a model response nor a real account. Desktop window navigation
  and the owner's normal profile startup remain to be checked.
  [Harness session controller](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/session#ctx-sessioncontroller-sessioncontroller)
- **D-11. An untaken item can be removed from the native inbox.** With
  the turn held, two `prompt({ mode: "queue" })` calls were accepted and
  the controller's `state` returned two distinct item IDs. Calling
  `updateQueue({ sessionId, itemId, action: { kind: "remove" } })`
  returned `accepted: true`. The next read showed only the control item.
  A repeat removal was rejected. The reference's sidecar withdraws a
  delivery before its plugin hands it to Harness. This run verified
  removal from the Harness inbox after controller admission and before
  turn claim.
  [Reference plugin](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs#L140)
- **D-12. The running turn can be interrupted.** While the held
  turn was still running, `sessionController.cancel({ sessionId })`
  returned `accepted: true` and its pre-step signal was aborted with
  reason `user`. The session went idle and the control item remained
  pending. After host shutdown, an independent read of
  `session.v4.jsonl.zstd` confirmed one `turn/end` with outcome
  `aborted` and cause `user`. The interrupted turn was held before
  model streaming or tool execution.
- **D-13. A plugin denial reaches the approval audit log.** Inside
  the held turn, the probe issued one synthetic approval request
  through the installed approval service. The plugin answered
  `rejected`. The controller event page and the compressed file after
  shutdown both held one `approval/asked` and a matching
  `approval/decided` with outcome `rejected`. The approval service
  and persistence are verified. A real tool's denied execution and
  the app card's behavior while the plugin holds or races an answer
  still need a Desktop window test.
- **D-14. A plugin denial blocks a PowerShell call before dispatch.**
  The installed Desktop Host ran an isolated read-only session. A
  profile-scoped probe requested approval for two exact `pwsh`
  calls that print fixed text. In the race case the probe delegated
  to a stand-in native answerer immediately and returned `rejected`
  after 180 ms. The stand-in received cancellation, the tool returned
  a refusal, and the probe saw neither dispatch nor the test output.
  In the control case it held for 100 ms, then delegated. The
  stand-in allowed that call once and the real tool printed the control
  text. The same probe is prepared for the Desktop window with 15 s
  and 5 s waits. Native card visibility remains an operator observation.
  [Reference approval race](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs#L277)
- **D-15. A native card appears while the plugin waits.** The owner ran
  the two D-14 cases in one disposable Desktop session. In the race case
  the probe delegated after 1 ms, and the owner saw the native approval
  card during the wait. The probe answered `rejected` after 15,002 ms.
  It logged a tool error, zero `pwsh` dispatches and zero matching
  output for that attempt. The timestamp measures the plugin decision;
  the renderer's exact dismissal time was not measured. In the control
  case the probe delegated after 5,012 ms. The owner saw the native card
  and allowed the call once. The probe recorded one dispatch and the
  expected PowerShell output. An independent read of the test session's
  compressed log matched both approval pairs by ID: `rejected`, then
  `allowed-once`. The two outer `run_code` calls settled with an error
  and success, respectively. The owner then removed the profile patch
  block; its original bytes were verified.
- **D-16. A Harness plugin can identify the MCP caller.** Two independent
  runs used the installed 0.2.2-rc.1 Desktop Host with a fresh profile,
  its bundled `@deepseek-ai/dsh-mcp-client`, and a local stdio probe
  relay. Two root sessions in the same folder called the real tool
  executor. One relay process served both; its parent PID matched the
  plugin host. The `tools/call` params carried only `name` and
  `arguments`, with no `_meta`. Neither test session ID appeared in the
  relay environment or in the wire data outside tool arguments.
  A call without a plugin note remained unattributed.

  The probe plugin used the `tools/execute` event's `agent.id`. It
  published a note before dispatch and removed it when the call settled.
  The relay matched its parent, the tool and the arguments to one
  outstanding note. Identical arguments in sequential calls identified
  each session correctly. Concurrent calls with different arguments
  both resolved to the right session, including a call whose argument
  claimed the other session's ID. Concurrent calls with identical
  arguments produced two candidates and both were refused as ambiguous.
  When the probe admitted identical calls one at a time, the relay
  returned the correct identity to both callers. No notes remained
  after either run. The two personal patch files had identical hashes
  before and after the isolated tests.
  [Reference MCP relay](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L26)
  [Reference Harness plugin](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs#L215)
- **D-17. The normal Desktop profile starts after probe removal.**
  The owner opened Harness with both temporary blocks removed and
  confirmed the main window loaded. A read-only check found no Rooom
  test block in either patch. The home patch still matched its pre-test
  content apart from the already recorded trailing LF difference.

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
2. **Claude.** Words were observed with a development module (C-9, C-10).
   The MCP relay identified two Desktop Code-tab sessions (C-14).
   A new session was seeded and imported into Desktop (C-15). The owner
   refused a call in the plugin's own Desktop decision dialog (C-19).
   Both prompts reached the model in the Desktop queue withdrawal
   trial (C-20).
   Loading a plugin in every session has not been verified.
3. **Codex.** Words (X-5), requests (X-8), opening a new thread (X-14),
   withdrawing a queued message (X-15) and interrupting (X-16) were
   observed. The bundled Desktop app-server identified its MCP caller
   (X-17). A wake-only link delivered the first queued message to each
   of two new threads with Desktop already running (X-18).
4. **Antigravity.** State, words and requests were observed (A-7, A-8, A-10).
   The MCP relay identified two Desktop conversations (A-13). The
   Windows Desktop service created a conversation, withdrew a pending
   message and interrupted an invocation (A-14 to A-16).
5. **DeepSeek Harness.** The owner removed the original probe block (D-4
   to D-8). The isolated Desktop Host verified creation, queue withdrawal,
   interrupt and a synthetic denial (D-10 to D-13). The preflight
   verified rejection and dispatch using the native PowerShell tool
   with a stand-in answerer (D-14). In the Desktop window, the owner saw
   the native approval card during the race and after the delayed
   handoff. The denied command did not dispatch and the control ran
   (D-15). The installed Desktop Host and its actual stdio MCP client
   identified two callers through a probe plugin (D-16). The owner
   confirmed the normal Desktop launch after removal (D-17).
