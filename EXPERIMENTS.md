# Rooom — experiments

Milestone 1 of [PLAN.md](PLAN.md): what each client really offers on Windows,
found by running commands against the real client. Only the channels D-038
allows are tried. Product setup stays manual under D-028. A live test that
modifies client settings records the owner's authorization and the cleanup.

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
| The permission mode can be read | observed (C-3) | observed (X-3) | command auto-execution policy observed (A-9). Comparability as a session permission mode is open. | observed (D-2) |
| **Requests reach Rooom** | observed (C-6) | observed (X-8) | observed (A-10) | observed (D-5) |
| **Rooom's words reach the session** | observed (C-9) | observed (X-5) | observed (A-8) | observed (D-6) |
| An untaken message can be cancelled | observed in Desktop (C-28) and in the isolated engine (C-16, C-26); earlier Desktop trial failed (C-20) | observed (X-15) | observed (A-15) | observed (D-11, D-18, isolated profile) |
| A running turn can be interrupted | observed (C-10); also at a permission boundary (C-7) | observed (X-16) | observed (A-16) | observed (D-12, D-18, isolated profile) |
| A new session can be started | observed (C-15, CLI seed and Desktop import) | observed (X-14) | observed (A-14) | observed (D-10, D-18, isolated profile) |
| The sending session can be named for MCP | observed (C-14, Desktop Code tab) | observed (X-17, X-21, Desktop) | observed (A-13, Desktop) | observed (D-16, D-18, isolated profile) |

D-037: all four clients have both of their rows observed and stay in v1.

## Across clients

| Experiment | Result |
| --- | --- |
| A windowed-subsystem program works as a stdin/stdout helper | observed (G-1) |
| A windowed helper started by the Claude desktop app as a hook | observed (C-6) |
| The same with a Tauri Windows-subsystem executable, started by each client | observed (G-13, Claude and Codex engines, Antigravity Desktop hook, Harness Desktop Host MCP) |
| Cold start of the real executable, antivirus scan included | observed with a Tauri release probe in an isolated Windows guest, with Defender scan events in all six first-launch intervals (G-17) |
| An executable under a path with a space and non-ASCII characters starts | observed (G-2) |
| Paths with spaces and non-ASCII characters in client hook and plugin entries | observed (Claude Code and Codex engines, and Harness Desktop Host plugin and MCP in G-11. Antigravity Desktop native hook in A-19) |
| The host toolchain builds an MSVC executable | observed (G-3) |
| Where Store-packaged clients keep their files | observed (G-4) |
| The panel opens without taking focus, then takes it for typing | observed on Tauri with a physical click and Chinese IME input (G-14, including the completion-receipt limitation) |
| Capture exclusion through Windows capture APIs | observed through FFmpeg's WGC and GDI capture on the Tauri probe (G-13). The owner waived separate application trials for milestone 1 on 2026-11-15. |
| The pipe name with two Windows sessions of one user | observed in an isolated Windows guest across interactive Session 1 and noninteractive Session 0 under the same account (G-18) |
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
- C-21 and C-22 cover the user settings file, session rules, questions
  and plans in the isolated engine. The Desktop Code tab still needs
  its own check for those cases.
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
  `plugin validate` command accepted it and listed its hooks and calls. A
  user-scope installation worked in two isolated CLI sessions (C-23).
  Desktop loading through that installation route remains open.
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

- **C-21. The user hook can add an in-memory allow rule.** Claude Code
  2.1.325 was fetched from the release URL in Desktop 5.11492.5 and
  matched its packaged manifest checksum. In a fresh `CLAUDE_CONFIG_DIR`
  with only user settings enabled, a `PermissionRequest` hook received
  the first of two synthetic `Write` calls outside the test workspace.
  It sent back an allow rule with `destination: "session"`. Both writes
  succeeded and the second did not invoke the hook. In a new session
  with the same profile, the hook received both calls and refused them.
  Both target files stayed absent in that control. The user settings
  file was byte-identical and no local settings file was created.
  [Windows launch form in the reference](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/hooks/windows.rs#L24)
  [Claude permission update entries](https://code.claude.com/docs/en/hooks#permission-update-entries)
- **C-22. Questions and plans take different hook paths in the fixture.**
  The same engine ran in print mode using a loopback model and an
  MCP permission host that denied as a fallback. For `AskUserQuestion`
  and `ExitPlanMode`, the `PermissionRequest` hook received each event
  and returned allow with `updatedInput`. Both calls still went to the
  fallback and were denied. With `PreToolUse` and the same input updates,
  the next model request received the chosen answer and, in a separate
  plan-mode run, the approved synthetic plan. Both plan hooks received
  the injected `plan` and `planFilePath` fields. This verifies the
  isolated engine's print-mode behavior with that permission host.
  [Reference question and plan responses](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/hooks/request/decision.rs#L74)
  [Claude tool hook input and output](https://code.claude.com/docs/en/hooks)
- **C-23. A user-scope plugin loaded in two fresh CLI sessions.** Claude
  Code 2.1.325 installed a fixture plugin from a local marketplace with
  `plugin install --marketplace <path> --scope user` in an isolated
  profile. Two subsequent sessions in different empty workspaces each
  recorded a distinct marker from its `session.start` module. A clean
  profile without the plugin recorded none. The runs used the
  installed plugin with no `--plugin-dir`. Desktop Code-tab loading
  of the user-scope install still needs a live test.
  [Reference installation](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/claude_plugin/records.rs#L64)
- **C-24. Desktop Code loaded user settings and returned a hook answer.**
  In Desktop 5.11492.5, the owner sent a prepared draft once in a
  disposable workspace. The scoped user settings hook recorded one
  `SessionStart` and one `PreToolUse` for `AskUserQuestion`. The tool
  result for that call contained the fixture's chosen answer. Both
  synthetic writes to the sibling fixture folder succeeded under
  `auto` mode, with zero `PermissionRequest` events. This leaves the
  session-only rule unverified in Desktop. Plan approval and installed
  user-scope plugin loading were outside this run. The original user
  settings were restored and verified byte for byte.
  [Claude tool hook input and output](https://code.claude.com/docs/en/hooks)
- **C-25. Desktop Code approved a plan and loaded an installed plugin.**
  Desktop 5.11492.5 ran Claude Code 2.1.325 in a disposable workspace.
  A workspace `PreToolUse` hook received the fixed plan and plan path
  from `ExitPlanMode`, returned allow with updated input, and
  `PostToolUse` observed the transition from `plan` to `default`.
  The subsequent first write invoked `PermissionRequest`; it ran with
  the fixed contents. The second write still prompted and was denied.
  An installed user-scope plugin from a local marketplace independently
  wrote a session-start marker that matched the Code-tab transcript ID.
  A loopback repeat in the isolated engine changed the rule's tool from
  `Write` to `Edit`. Its control write was denied before any grant.
  The first write after the control installed the session rule, and the
  next write completed without a new `PermissionRequest`. This repeat
  verifies the corrected fixture in the bundled engine.
  [Claude plan hooks](https://code.claude.com/docs/en/hooks#exitplanmode)
  [Reference user-scope install](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/claude_plugin/records.rs#L64)
- **C-26. The held-turn fixture passed in the bundled engine.**
  Claude Code 2.1.325 ran with an isolated profile and a loopback model.
  The control write was denied, the first fixture write installed an
  `Edit` allow rule for the scratch directory at session scope, and
  the second write completed without another permission request.
  A fourth tool call held in `PreToolUse` while two marked prompts
  were queued. The fixture sent `cancel_async_message` with the DROP
  message's UUID in `message_uuid`. The engine returned
  `cancelled: true` before the hold was released. The loopback model
  received KEEP and never received DROP. Using `uuid` alone did not
  remove DROP in the first trial.
  [Claude permission updates](https://code.claude.com/docs/en/hooks#permission-update-entries)
  [Reference queued delivery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/docs/agent-messages.md#L15)
- **C-27. Desktop Code honored a scoped in-memory edit rule.**
  In Desktop 5.11492.5 with Claude Code 2.1.325, a disposable Manual-mode
  session attempted three separate writes to the same sibling fixture
  directory. The control hit `PermissionRequest` and was denied. The
  first test write hit the hook, which allowed it and returned a scoped
  `Edit` rule with destination `session`. The second test write reached
  `PreToolUse` and `PostToolUse` with no permission request. Both test
  files matched the expected contents and the control file stayed absent.
  [Claude file permission rules](https://code.claude.com/docs/en/permissions#read-and-edit)
- **C-28. Desktop withdrew a queued message while a tool call was held.**
  In the C-27 session, a fourth write waited in `PreToolUse` on a
  fixture signal. The owner queued DROP and KEEP and reported withdrawing
  DROP. Both marked enqueues appear during the hold. The probe released
  the hook after the owner's confirmation. The next recorded SDK model
  request referenced a message blob containing KEEP. No linked message
  blob contained DROP, and the transcript recorded KEEP as
  `absorbed_mid_turn`. The exact withdrawal was not logged as a
  standalone DROP removal. This run confirms the native Desktop control
  under a held call, with a separate isolated protocol check in C-26.
  [Reference queued delivery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/docs/agent-messages.md#L15)

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

- **X-19. The bundled app-server can approve a command for one thread.**
  Codex Desktop 26.1111.2228.0 bundles codex-cli 0.169.0-alpha.11.1.
  In an isolated app-server with a loopback model, user-reviewed
  approvals and a read-only sandbox, a synthetic PowerShell command
  prompted through `item/commandExecution/requestApproval`. Answering
  `acceptForSession` ran the command. The identical command in a second
  turn of the same thread ran with no request. A different command in
  that thread and the original command in a new thread each prompted
  again, and declining each produced no matching output. The fixture
  configuration was byte-identical and no rules directory was created.
  This exercised app-server stdio. X-20 covers the Desktop owner pipe,
  and X-11 covers the separate hook response schema.
  [Reference Desktop answers](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop/reply.rs#L270)
- **X-20. The Desktop owner honored `acceptForSession` for a fixed command.**
  In Desktop 26.1111.2228.0, the probe verified the pipe server's package,
  user and Windows session. The native approval request offered once,
  an exec-policy amendment, and cancel. The first probe's matcher
  rejected the PowerShell launcher wrapper and interrupted the trial.
  After matching the underlying command and its thread and turn IDs,
  the probe sent `acceptForSession` to the owning Desktop client and
  received a successful RPC reply. Two completed turns in that thread
  each recorded exit 0 and the expected output for the same command.
  The probe saw no second approval, but the live stream and a subsequent
  durable read disagreed on the second turn's status. A fresh thread
  prompted for the command again. The decline acknowledgement was
  inconclusive, and the fresh thread recorded no command execution.
  Both disposable threads were archived. This establishes behavior for
  one command through the Desktop owner pipe. Card eligibility and the
  Desktop-owned MCP handshake remain open.
  [Reference owner pipe validation](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop_io/windows.rs)
  [Reference Desktop answers](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop/reply.rs#L270)
- **X-21. Desktop completed a stdio MCP handshake and identified two callers.**
  With Desktop 26.1111.2228.0, the probe used a fresh workspace and a
  fixed-response stdio relay. The app-server listed the project layer
  as disabled while the workspace was untrusted. Once trusted, a
  separate app-server read showed the entry active; the Desktop trials
  with only that entry produced no recorded handshake. With the relay
  temporarily registered in user configuration, its process recorded
  `initialize` and `tools/list` from the Desktop-owned app-server,
  verified by parent PID. Under the default tool approval behavior,
  two fresh Desktop-owned turns discovered the tool in code mode but
  no call reached the relay. The successful repeat advertised the
  fixture tool as read-only and set `default_tools_approval_mode`
  to `approve` for that relay alone. In two fresh Desktop chats the
  relay handled `initialize`, `tools/list`, and one `tools/call`
  each. Each call's `_meta.threadId` and
  `_meta["x-codex-turn-metadata"].turn_id` matched its completed
  Desktop turn. The two changes were tested together. The original
  Codex configuration was restored byte for byte.
  [Reference registration and caller metadata](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L31)
  [Codex MCP configuration](https://learn.chatgpt.com/docs/extend/mcp)
- **X-22. Desktop honored declines on command and file-change requests.**
  Two disposable chats used the same bundled engine, read-only sandbox
  and user review. The PowerShell request offered `accept`,
  `acceptWithExecpolicyAmendment`, and `cancel`. The file-change request
  had no `availableDecisions` field. The probe replied through the
  owner pipe with each request's matching method and ID. Both replies
  received `ok: true`. Durable read-back showed no execution item for
  the command. The file-change item was recorded and the requested file
  stayed absent. Card rendering, file-change session approval, and
  other request types remain open.
  [Reference owner replies](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/codex/desktop/reply.rs)
- **X-23. The two inline desktop cards show the same primary controls.**
  Screenshots of pending command and file-change requests on Desktop
  26.1111.2228.0 showed “拒绝” (Esc), “允许一次”, and a dropdown arrow
  next to “允许一次” on both cards in the Simplified Chinese UI. The
  dropdown contents were not observed. At the protocol boundary, the
  command again exposed `accept`, `acceptWithExecpolicyAmendment`, and
  `cancel`. The file request omitted `availableDecisions`. Both
  screenshots were taken while the requests were pending. Both requests
  in that pair were then declined through the owner pipe. A separate
  earlier pair settled during the owner-assisted check: the fixed
  command exited 0 and the fixed file was created in the scratch
  workspace. Its cards were gone before the screenshot check.
  [Codex app-server approval requests](https://learn.chatgpt.com/docs/app-server)
- **X-24. The installed renderer assigns different scopes to the two menus.**
  Read-only inspection of Desktop 26.1111.2228.0 found the shared
  two-item menu component in `webview/assets/async/43006.d70a2b612c.js`
  and `478806.1fcc90f6a3.js`. The caller in `722374.20aae48d2d.js`
  supplies “允许类似命令” for the ordinary command branch when an
  exec-policy amendment is proposed. It sends
  `acceptWithExecpolicyAmendment`. The matching engine source appends
  that amendment to the user's default rules file, even though the
  renderer tooltip calls it conversation-scoped. For a file-change
  card, the caller supplies “允许所有修改” and sends
  `acceptForSession`. Both menus
  also include the primary “允许一次” action. Simplified Chinese
  strings were checked in `1561.1b3fb9ee2c.js`. These menu entries
  were read from the installed bundle. X-23 remains the live visual
  observation of the collapsed cards.
  [Codex app-server approval decisions](https://learn.chatgpt.com/docs/app-server)
  [Bundled engine's exec-policy write](https://github.com/openai/codex/blob/rust-v0.169.0-alpha.11.1/codex-rs/core/src/exec_policy.rs#L516)
- **X-25. Desktop reused a file-change approval for the same path.**
  In Desktop 26.1111.2228.0, the verified owner pipe received a
  `thread-follower-file-approval-decision` with `acceptForSession`
  for a matched one-file patch. It acknowledged the decision, and
  the patch completed. A new turn in that chat changed the same
  file to a different marker without another request. A different
  path in the same chat prompted again. A fresh chat editing the
  original path also prompted. In a third chat, an `accept` decision
  applied the first edit, and the next edit to the same path prompted
  again. All three control requests were declined, and durable
  `thread/read` and file contents agreed. The configuration and
  rules hash stayed unchanged. The renderer label “允许所有修改”
  (X-24) does not describe a blanket grant for unapproved paths in
  this fixture. The matching engine source keys patch approval by
  environment and file path.
  [Engine approval cache keys](https://github.com/openai/codex/blob/rust-v0.169.0-alpha.11.1/codex-rs/core/src/tools/approvals.rs#L282)
- **X-26. The owner-assisted menu check did not record the expanded labels.**
  The installed Desktop raised both disposable approval requests
  (one fixed-output command and one scratch-file patch). The owner
  reported completion. No expanded screenshot or transcription was
  received, and the foreground capture found a different application.
  In the durable records the command and the patch had already
  completed, even though the subsequent decline calls were
  acknowledged. The expected scratch file was present. The
  configuration and rules hashes remained unchanged. This evidence
  leaves live menu labels open and does not change the static result
  in X-24 or the controlled session-grant result in X-25.
- **X-27. Both Codex Desktop approval menus were photographed while expanded.**
  On Windows Desktop `26.1111.2228.0` with the Simplified Chinese UI,
  opening the arrow beside the allow-once button showed these rows in order:

  | Card | First row | Second row |
  | --- | --- | --- |
  | Command | `允许一次` | `允许类似命令` |
  | File change | `允许一次` | `允许所有修改` |

  `拒绝` was a separate card button in both cases. The initial command
  printed its fixed marker and the initial file patch wrote only its
  scratch fixture. The file menu was captured on a fresh retry, which
  was then declined. The retry file was absent. The configuration and
  rules hashes matched the baselines. X-24 and X-25 cover the actions
  behind these labels and the scope of the file grant.

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

- **A-17. The isolated Antigravity 2.32.1 language server still needs
  sign-in before a tool run.** With temporary Gemini data, telemetry
  disabled and model service endpoints set to loopback, the installed
  standalone language server 1.11.0 announced its local listeners but
  asked for sign-in even with `--use_mocked_data`. A bounded
  `GetUserStatus` check did not complete. No model turn or
  `PreToolUse` event was observed. A local fixture invoked the documented
  workspace hook command with synthetic stdin and checked that an exact
  command was denied, a control passed through and unmatched inputs
  left no marker. A-18 covers the later Desktop run.
  [Reference service discovery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/clients/antigravity/service.rs)
  [Antigravity PreToolUse](https://antigravity.google/docs/hooks?tab=ide#pretooluse)

- **A-18. A workspace hook denied a command in authenticated Antigravity
  Desktop.** The installed 2.32.1 Desktop service opened a new conversation
  in a disposable workspace whose path included spaces and Chinese
  characters. The first run reached the hook twice but took its default
  `allow` branch. The tool calls carried an extra JSON string encoding of
  the command and omitted `Cwd` inside `toolCall.args`. The fixture was
  adjusted to recognize the exact command after decoding and to use the
  single matching workspace when the call omitted its directory.

  A fresh conversation then produced two matching hook events. The
  service reported `deny`, an error step, and no process exit code or
  shell output for the first command. The control reported `auto_approve`,
  exit code 0, and the exact marker as a line in the captured terminal
  output. Neither command required a Desktop approval click. The probe
  targeted only the two fixed commands in its own workspace. The
  workspace hook was removed and both test conversations were deleted.
  [Reference Desktop service](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/mod.rs#L101)
  [Antigravity PreToolUse](https://antigravity.google/docs/hooks?tab=ide#pretooluse)

- **A-19. Authenticated Antigravity Desktop launched a native hook from
  a Unicode path.** On 2026-11-15, Desktop 2.32.1 opened one disposable
  conversation through its authenticated loopback service. The workspace
  `PreToolUse` entry used a UTF-16LE encoded PowerShell command to
  invoke a 308,224-byte Rust MSVC Windows-subsystem executable. Both the
  executable's directory and its filename contained spaces and Chinese
  characters. Synthetic stdin first verified denial, automatic approval,
  and default handling for a mismatched conversation.

  In the Desktop run, two distinct processes verified their own
  executable path and matched the stdin event to the expected workspace,
  conversation and tool. Desktop read the hook's `deny` for the first
  fixed command. The step ended in error with no shell output or process
  exit code. Desktop read `auto_approve` for the control. The command
  finished with exit code 0 and its expected output line. Both outcomes
  were verified through `GetCascadeTrajectory`.

  `DeleteCascadeTrajectory` removed the conversation. Readback confirmed
  removal of the trajectory and its data directory. The workspace hook and
  the fixture directory were removed. The user-level hook configuration
  matched its pre-test state.
  [Reference Desktop service](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/mod.rs#L101)
  [Antigravity PreToolUse](https://antigravity.google/docs/hooks?tab=ide#pretooluse)

- **A-20. Desktop MCP discovery and relay startup use different
  scopes.** On 2026-11-15 the installed Antigravity Desktop 2.32.1
  language server's embedded MCP guide named
  `~/.gemini/config/mcp_config.json` and enabled plugins' MCP files as
  configuration sources. The running daily profile's
  `GetMcpServerStates` included the entry in the personal file. Its
  `spec.configPath` field was empty, so the path was tested with a fresh
  profile as well.

  The installed Desktop executable launched its own language server
  under disposable Electron and home directories. The same stdio
  relay was registered under distinct names in the temporary personal
  file and in the scratch workspace's `.agents/mcp_config.json`. The
  personal-file entry reached `MCP_SERVER_STATUS_READY`. Its trace
  recorded `initialize`, `notifications/initialized` and `tools/list`.
  `GetCustomizationStates` found the workspace entry when queried with
  the fixture workspace URI. The workspace relay never started. This
  reproduces the missing handshake in G-13 at the registration
  boundary. The workspace file was discoverable as a customization,
  while the personal file supplied the live MCP server.

  A temporary local conversation was created and the direct
  `LaunchMcpApp` and `CallMcpAppTool` requests both returned
  `failed_precondition` because MCP Apps was disabled in the isolated
  instance. No `tools/call` reached the fixture in this run. This result
  covers the direct MCP Apps route. A-21 covers a normal agent tool
  invocation under the same Desktop version. The isolated process tree
  and both probe profiles were removed. The daily profile stayed running,
  and its MCP file had the same SHA-256 before and after.
  [Google Antigravity Codelab](https://codelabs.developers.google.com/getting-started-google-antigravity#5)
  [Antigravity MCP configuration](https://antigravity.google/docs/mcp/)
  [Reference shared relay](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L26)

- **A-21. A signed-in Desktop turn called a temporary MCP tool once.**
  The owner authorized inserting one read-only server under `mcpServers`
  in the active `~/.gemini/config/mcp_config.json` for Antigravity Desktop
  2.32.1. The probe preserved all original bytes and kept a rollback copy
  in the same directory. The file change and a named shallow refresh in
  the first attempt did not register the probe. That attempt restored the
  file before any conversation was created. In the repeat, the new entry
  was ready after `RefreshMcpServers` with the probe's `serverName` and
  `shallow: false`.

  The relay recorded one `initialize`, one `notifications/initialized`,
  one `tools/list`, and one `tools/call`. The call used empty arguments
  and its MCP conversation metadata matched the disposable Desktop
  conversation. The tool returned a fixed string. The same string
  appeared in the Desktop trajectory, and the turn returned to idle.
  No one-time approval had to be sent.

  Desktop readback confirmed deletion of the conversation and its
  data directory. The original MCP config matched byte for byte and by
  SHA-256. The named entry and schema cache were absent. One relay
  process remained after unregistering the server. The probe stopped the
  process after matching its command line and creation time, then
  confirmed no fixture process remained. The rollback copy and all
  temporary files were removed. Desktop stayed running throughout.
  [Reference Antigravity registration and caller metadata](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/apps/mcp/README.md#L27)
  [Antigravity MCP configuration](https://antigravity.google/docs/mcp/)

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
  [Harness session controller](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/session#ctx-sessioncontroller-%E2%80%94-sessioncontroller)
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
- **D-18. The installed Desktop shell started a Host with working controls.**
  Harness 0.2.2-rc.3 launched as a GUI app with a disposable profile and
  data directory. A probe plugin created a session and held it before
  model dispatch. It removed one queued message and checked that the
  other remained, then interrupted the running turn. The Host observed
  the abort signal and its compressed session log recorded a user abort.
  Two sessions called the bundled stdio MCP tool. The plugin matched both
  callers, the relay received only `name` and `arguments`, and all notes
  were removed. The probe used no model credentials. This covers the
  Host launched by the installed desktop shell. Renderer controls and
  the normal profile were outside this repeat.
  [Harness session controller](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/session#ctx-sessioncontroller-sessioncontroller)

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
- **G-5. Hook path and stdio preflight.** An optimized, 166,400-byte
  Rust Windows-subsystem helper was placed under a name with spaces and
  Chinese characters. Two separate `codex exec` runs used fresh
  `CODEX_HOME` directories and a synthetic loopback model. The only hook
  was a fixture `SessionStart` command, trusted for the invocation with
  the CLI's one-off flag. Both standalone codex-cli 0.162.1 and the
  executable bundled in Desktop 26.1111.2228.0 started the helper through
  a UTF-16LE PowerShell `commandWindows` entry, passed it the stdin event,
  and delivered its stdout context marker to the model fixture. The
  other command field was an exit-only negative control. Claude Code
  2.1.325 also started it through a user `SessionStart` hook with
  `command` and `args` in an isolated profile, and the loopback request
  received its stdout context marker. The Claude run without the hook
  produced neither a launch marker nor the context marker.

  In its MCP role the same helper answered `initialize` and `tools/list`
  for standalone codex-cli 0.162.1. The bundled 0.169.0-alpha.11.1 engine
  returned Windows error 5 before a handshake in the restricted shell,
  including with an ASCII-only helper path and a byte-identical engine
  copy outside the Store directory. The exact Codex release source
  creates private named pipes before it starts a relay. A pipe probe
  with that descriptor also got error 5 in the restricted shell (G-7).
  Running the original bundled executable outside the restricted shell
  with a fresh fixture completed `initialize` and `tools/list` using the
  helper at the Unicode path. The Desktop window and the final Rooom
  executable have separate acceptance criteria.
  [Reference Windows hook launch](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/hooks/windows.rs#L23)
  [Codex Windows stdio source](https://github.com/openai/codex/blob/rust-v0.169.0-alpha.11.1/codex-rs/rmcp-client/src/local_stdio_io/windows.rs#L95)
  [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- **G-6. Windows focus and capture APIs.** With a foreground window
  available, `SetWindowPos` with `SWP_SHOWWINDOW | SWP_NOACTIVATE`
  made a disposable top-level Win32 window visible and preserved the
  foreground handle. DWM composition was enabled. Setting and reading
  back its display affinity produced `0 → 0x11 → 0`, where `0x11` is
  `WDA_EXCLUDEFROMCAPTURE`. The screen-copy probe could not see its
  unprotected control window's expected pixel, so it gave no evidence
  about capture exclusion. G-8 and G-9 cover the later WebView2 tests.
  [Reference HUD focus](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/windows.rs#L41)
  [Windows display affinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)
- **G-7. Named-pipe creation and connection within one Windows session.** A
  disposable Win32 pipe with `FILE_FLAG_FIRST_PIPE_INSTANCE` and
  `PIPE_REJECT_REMOTE_CLIENTS` rejected a second first instance with the
  same name (Win32 error 5). A distinct-name control was created. A
  client connected to the default-descriptor control. In the restricted
  shell, user-only, user-and-SYSTEM, and owner-rights-and-SYSTEM
  descriptors returned access denied at client connection. The same
  user-and-SYSTEM test outside the restricted shell connected and
  returned the expected server session ID. `WTSEnumerateSessionsW`
  succeeded there and found no second session for the same account.
  Cross-session collision and routing remain unverified.
  [Reference Windows pipe name and server](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/os/windows/pipe.rs#L27)
  [Windows named-pipe access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- **G-8. A windowed WebView2 executable ran in both window and stdio
  modes.** A disposable Win32 program linked the WebView2 loader and
  used an isolated WebView2 157.0.4341.46 profile. Showing the panel
  with `SWP_NOACTIVATE` left the control window in the foreground.
  An explicit focus action made the panel the foreground window,
  called `ICoreWebView2Controller::MoveFocus`, and verified both
  `document.hasFocus()` and the textarea's identity from the page.
  This exercised programmatic activation. Manual click and Chinese
  IME composition were outside this run.

  The same 218,112-byte Windows-subsystem executable took a binary
  stdio path before loading WebView2. All 30 launches echoed the
  exact input bytes, exited with code 0 and left stderr empty.
  The median from parent spawn to exit was 22.0 ms and the p95
  was 30.2 ms. These were repeated process starts. The test host's
  Defender real-time protection was off, and there was no Rooom
  Tauri executable to measure.
  [WebView2 MoveFocus](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller#movefocus)
- **G-9. Windows Graphics Capture excluded the WebView2 panel.**
  FFmpeg 9.0's `gfxcapture` sampled a 2×2 monitor region where two
  synthetic windows overlapped. It saw the background before the
  panel was shown, the white WebView2 field with affinity off, the
  background with `WDA_EXCLUDEFROMCAPTURE`, and the same field again
  after restoring `WDA_NONE`. The probe compared colors and saved no
  images. A separate Win32 color fixture passed the same sequence
  through both `BitBlt` and FFmpeg. The restricted shell denied the
  FFmpeg monitor capture; the full sequence passed on the Windows
  desktop. The owner's meeting and recording apps still need a check.
  [FFmpeg gfxcapture](https://ffmpeg.org/ffmpeg-filters.html#gfxcapture)

- **G-10. Physical Chinese IME input reached the WebView2 field.**
  The first probe showed its panel without taking focus and timed out
  without a composition receipt. A retry used a visible native button
  on the panel to request focus. It again confirmed that initial show
  left the original window in the foreground. After the owner's click,
  it verified the panel foreground and WebView2 textarea focus. The
  owner used a Chinese IME, and the probe observed composition start,
  a Han input event and completion of the composition with Han text.
  It exited 0 and closed the window on success. The probe recorded
  event labels only. G-14 covers physical Chinese input on Tauri.
  [Reference text focus](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/text_focus.rs#L37)

- **G-11. One windowed executable served a batch of isolated client
  launches.** On 2026-11-15, a 160,256-byte Rust MSVC fixture under a
  directory and filename containing spaces and Chinese characters ran
  as both a command hook and a stdio MCP server.

  - Standalone Codex 0.162.1, the Codex 0.169.0-alpha.11.1 engine bundled
    with Desktop 26.1111.2228.0, and Claude Code 2.1.325 from the Desktop
    distribution each ran twice with a fresh profile and a loopback
    model. The hook used the Windows PowerShell override in Codex and
    explicit arguments in Claude. In each configured run, the helper
    received one `SessionStart` event, the model received the exact hook
    context, and the MCP client negotiated `initialize`, sent
    `notifications/initialized` and completed `tools/list`. The model
    returned its fixed reply. Each unconfigured control returned the same
    reply with no hook context or helper activity. The Claude fixture
    disabled deferred tool search and also checked the exact tool name in
    the model request.
  - The installed DeepSeek Harness 0.2.2-rc.3 Desktop Host, launched in
    Electron Node mode, loaded a test plugin from another path containing
    spaces and Chinese characters. Its bundled MCP client started the same
    windowed executable, completed initialization and discovery, and
    returned the expected `tools/call` result through the registered tool.
    The relay recorded EOF at shutdown. In the missing-executable control
    the Host became ready and loaded the plugin, the launch failed, and
    no relay events were recorded.

  This batch used disposable profiles with fixture-only settings and
  synthetic inputs. The engine runs cover hook launch and stdio exchange.
  The installed Desktop windows and the Rooom Tauri artifact have
  separate acceptance checks.
  [Reference hook launch forms](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/agents/src/hooks/windows.rs#L21)
  [Reference Windows MCP relay](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/cli/windows.rs#L8)
  [Reference Harness plugin loading](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/deepseek_plugin/agent-hud.mjs#L1)

- **G-12. The unsigned-in Antigravity CLI produced no hook receipt.**
  The A-18 workspace hook used an encoded PowerShell command
  that passed a script path with spaces and Chinese characters to Node.
  A fresh, unsigned-in Antigravity CLI profile was tried with and
  without a workspace `PreInvocation` hook. Both print-mode runs reported
  a sign-in requirement and recorded no hook receipt. A direct control
  gave synthetic stdin to the executable at the Unicode path and
  received its expected stdout. A-19 covers the authenticated Desktop
  follow-up.
  [Reference Antigravity Desktop delivery](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/delivery/antigravity/mod.rs#L101)
  [Antigravity PreInvocation](https://antigravity.google/docs/hooks?tab=ide#preinvocation)

- **G-13. One Tauri executable ran the client and desktop probes.**
  On 2026-11-15 a disposable release build used Tauri 2.12.5,
  tauri-build 2.7.4, x64 MSVC and WebView2 157.0.4341.46. The
  7,749,120-byte executable had PE subsystem 2 and the same SHA-256
  throughout the runs:
  `ff058ade1b4a50e3966b9686e7bd0644450d1cea25299406c565a48fb4e46f19`.
  The hook and MCP flags entered their stdio handlers before starting
  Tauri. The window test used the executable's Tauri path.

  - **Client launches.** The G-11 configured and unconfigured cases
    passed with standalone Codex 0.162.1, Desktop's bundled Codex
    0.169.0-alpha.11.1, and Claude Code 2.1.325 using loopback models.
    Each configured run delivered the exact hook context to the model and
    observed MCP initialization and tool discovery. The Claude run also
    found the tool in the model request. The Harness 0.2.2-rc.3 Desktop
    Host returned the exact MCP tool receipt and closed the relay. Its
    missing-executable control started the Host without relay events.
    Antigravity Desktop 2.32.1 launched the same executable twice as a
    workspace hook in one disposable conversation. The deny step had
    no output or exit code, and the automatic-approval control produced
    its expected output with exit code 0. The workspace MCP entry
    produced no relay receipt. The conversation and its app data were
    removed, and the user-level configuration matched its pre-test state.
  - **Focus.** With the panel hidden and its focusability off, the probe
    sampled the foreground immediately before `show`. The same window
    stayed foreground after show. Explicit activation enabled
    focusability and made the panel foreground. The WebView reported
    `document.hasFocus()` and the textarea as its active element. The
    first run sampled the foreground before launching the capture
    controls and gave an inconclusive show comparison. The repeat
    moved that sample next to show and used the same executable.
    The physical Chinese IME follow-up is G-14.
  - **Capture.** FFmpeg 9.0 compared only a 2×2 region of overlapping
    synthetic windows. Both `gfxcapture` (Windows Graphics Capture)
    and `gdigrab` (GDI) saw the background, the visible panel, the
    background under protection, then the restored panel. Each phase
    matched all 12 sampled pixels for each path. The visible panel's
    display affinity read `0 → 0x11 → 0`. The run wrote no frame files.
    The owner waived separate application trials for milestone 1 on
    2026-11-15. See [the experiment scope](PLAN.md#experiments-before-the-interface-promises-anything).
  - **Repeated startup.** Fifteen launches per stdio role produced only
    the expected JSON replies, empty stderr and exit code 0. Spawn-to-exit
    medians were 32.7 ms for the hook and 33.3 ms for MCP. These runs
    measure repeated launches and leave cold startup with antivirus open.

  The workspace hook and MCP files, probe, WebView2 profile and temporary
  dependency cache were removed.
  [Reference Windows MCP relay](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/cli/windows.rs#L8)
  [Reference text focus](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/text_focus.rs#L37)
  [Reference WebView2 capture](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/capture/windows.rs#L36)
  [Tauri content protection](https://docs.rs/tauri/2.12.5/tauri/webview/struct.WebviewWindow.html#method.set_content_protected)
  [FFmpeg capture filters](https://ffmpeg.org/ffmpeg-filters.html#gfxcapture)

- **G-14. Physical Chinese IME input reached a Tauri textarea after one
  click.** On 2026-11-15 a disposable Tauri 2.12.5 build with WebView2
  157.0.4341.46 started hidden and non-focusable. Showing it preserved
  the existing foreground window. One trusted pointer event on the
  textarea requested `set_focusable(true)` and `set_focus()`. Windows
  then reported the panel as foreground. WebView2 reported document
  focus and the textarea as its active element.

  The owner typed with a Chinese IME and confirmed completion. With
  both focus checks still true, the probe recorded 7 trusted
  `compositionstart` events, 27 trusted composition updates, and
  19 trusted input events with Han characters. Five new composition
  sequences reported an update with zero Han characters followed by
  an input receipt still reporting Han characters. Paste and drop
  were disabled in the field. The probe stored only event types,
  booleans and counts.

  There was no trusted `compositionend` receipt. The initial automatic
  checker required that receipt and exited 1 without recording
  automatic completion. A separate check of the captured foreground,
  pointer, focus, composition and input receipts verified the physical
  input described above. The harness then signalled the probe to
  close. The probe exited 0. The executable, profile, logs and
  dependency cache were removed, and no probe or fixture WebView2
  process remained.
  [Reference text focus](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/hud/text_focus.rs#L37)
  [Tauri window focus API](https://docs.rs/tauri/2.12.5/tauri/webview/struct.WebviewWindow.html#method.set_focusable)
  [UI Events composition events](https://www.w3.org/TR/uievents/#events-compositionevents)

- **G-15. A disposable Tauri release build completed both stdio roles.**
  Cargo's optimized release profile built a 7,767,040-byte x64 Windows GUI
  executable with Tauri 2.12.5. Its SHA-256 was
  `e51f551834d8fb5e031b91c4de8041a1b915d938e1163cdb67454a13bf30a3a7`.
  Both flags branched before Tauri window creation. From a path with
  spaces and Han characters, a harness ran one priming launch and ten
  repeat launches per role. Requesting the process through receipt of its
  first stdout JSON reply took a median of 29.86 ms for the hook
  (28.20–32.55 ms) and 29.98 ms for MCP initialize (28.90–32.51 ms).
  All 22 processes returned the expected hook context or full MCP
  initialize/list/call receipts, empty stderr and exit code 0.

  Defender was stopped with demand start configured. The service ACL
  allowed start and did not grant `SERVICE_STOP` to Administrators.
  A reversible enablement path was unavailable, so no service or
  preference was changed. Before and after the batch, the service
  remained stopped and antivirus, real-time and on-access protection
  all read disabled. Antivirus-included cold startup remains unverified.
  For that test, require active real-time and on-access protection and
  correlate the exact measured image with a Defender scan event during
  its launch. A scan when staging an image is separate timing evidence.
  [Reference startup dispatch](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/lib.rs)
  [Windows service access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights)
  [Defender Performance Analyzer](https://learn.microsoft.com/en-us/defender-endpoint/tune-performance-defender-antivirus)

- **G-16. First-instance collision and routing passed across processes
  in one Windows session.** Each of two simultaneous Win32 servers used a
  distinct name, an account-and-SYSTEM DACL and
  `PIPE_REJECT_REMOTE_CLIENTS`. With room for a second instance, a
  duplicate create with `FILE_FLAG_FIRST_PIPE_INSTANCE` returned error 5
  for each name. Both second-instance controls succeeded without the
  flag. The client verified each server PID and Windows session ID.
  The two servers returned the expected client PID and Windows session
  ID and echoed each challenge to the correct client.

  The initial one-instance fixture returned error 231 because it had
  reached capacity. A retry could read stale readiness receipts. The
  successful run used two-instance capacity and fresh receipt paths and
  pipe names. All three processes were in the same Windows session.
  WMI (local and loopback) and a PowerShell loopback session with the
  existing interactive token also returned the same account SID and
  Windows session ID as the caller. Windows listed only one interactive
  login. Same-account, cross-session collision and routing remain open.
  The G-15 and G-16 probe files and build cache were removed, and no
  probe process remained.
  [Reference Windows pipe name and server](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/os/windows/pipe.rs#L27)
  [CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)

- **G-17. Defender scanned a Tauri release executable during first launch
  in an isolated Windows guest.** Windows Server 2025 Standard Evaluation,
  build 26100, ran offline in QEMU software emulation. Defender reported
  normal mode and active antivirus, real-time protection and on-access
  protection before and after the measured batch. Platform 4.18.25080.5,
  engine 1.1.25080.5 and signatures 1.437.1.0 were unchanged across it.

  The x64 Windows GUI-subsystem Tauri 2.12.5 release build was 7,768,064
  bytes. Its SHA-256 was
  `3d6b61a951a326a5df9f905a2cd01b03495a71c2e9aaee0a6c0a4b1a9dd68090`.
  Six filenames held the same executable code with distinct non-code PE
  trailers. The harness inserted their read-only guest media after WPR
  confirmed start with Defender's installed `Scans.Light` profile. Each
  file was launched twice. All 12 runs verified the hook response or the
  MCP initialize/list/call responses, an exact trace, empty stderr and
  exit code 0. Timing covered process launch through the first stdout
  reply. Each result below has three samples.

  | Role | First-launch median (range) | Same-file repeat median (range) | First-launch Defender scan median |
  | --- | --- | --- | --- |
  | Hook | 4,133.14 ms (3,485.34–4,482.34) | 483.15 ms (385.85–1,425.34) | 3,083.907 ms |
  | MCP | 3,946.70 ms (2,454.60–4,828.72) | 361.76 ms (315.43–509.93) | 3,455.931 ms |

  `Get-MpPerformanceReport -Raw` returned 152 file scan events. One
  `RealTimeScan` with reason `OnOpen` and skip status `Not skipped`
  matched each fresh image. Raw start and end timestamps placed all six
  events within their first-launch intervals. The six repeat intervals
  had zero matching scans. A first recorder attempt expired without a
  finalized ETL. The successful rerun explicitly started and stopped WPR
  and used fresh images. These timings apply to the software-emulated
  guest and its virtual optical storage.
  [Reference startup dispatch](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/host/src/lib.rs)
  [Windows Server Evaluation](https://www.microsoft.com/en-us/evalcenter/download-windows-server-2025)
  [Defender Performance Analyzer](https://learn.microsoft.com/en-us/defender-endpoint/performance-analyzer-reference)

- **G-18. A same-account pipe name collided across Windows sessions, and
  session-qualified names routed both ways.** In the G-17 guest, an
  interactive console process in Session 1 and a Task Scheduler S4U
  process in Session 0 reported different process IDs and an identical
  Windows account SID. All pipe servers restricted access to that account
  and SYSTEM and enabled `PIPE_REJECT_REMOTE_CLIENTS`.

  With the interactive server holding a name with no session suffix,
  the batch process got Win32 error 5 when requesting a second
  `FILE_FLAG_FIRST_PIPE_INSTANCE`. A second instance without the flag
  opened successfully under the same name. The batch process then
  connected to the interactive server and received the expected reply.

  Adding each actual Windows session ID to the name let both servers
  hold their first instances simultaneously. Each process still got
  error 5 when requesting a first instance under the other process's
  qualified name. The two clients connected in both directions and
  received the correct per-server marker and nonce. The Win32 pipe APIs
  returned the expected process and Windows session IDs at both ends.

  The PowerShell batch-worker tries timed out before a ready receipt.
  A precompiled worker reached the cross-session exchange. The final
  retry used shared reads for the coordination files after an earlier
  run hit a transient file sharing error. The S4U task was removed at
  the end of the run. The verified pair was one interactive console
  and one noninteractive Windows session in the same guest.
  The G-17 and G-18 virtual machine, media, probes, build cache and
  transient receipts were removed after verification.
  [Reference Windows pipe name and server](https://github.com/jazzenchen/agent-hud-open/blob/d14bc43fe117f2f5d4e9143d5ebcf92c62225a66/src/crates/platform/src/os/windows/pipe.rs#L27)
  [Task Scheduler security contexts](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks)
  [Windows guidance on session-specific pipe names](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services)

## How the reference does it

What the next experiments start from. Paths are in the reference repository.

| Question | The reference's answer | Where |
| --- | --- | --- |
| Windows: when does the windowed app handle `--mcp`? | It handles the flag before Tauri starts and runs its packaged MCP relay as a separate process. | `src/crates/host/src/lib.rs`, `cli.rs`, `cli/windows.rs` |
| Windows: when does the HUD take focus? | The Tauri window is shown with focus disabled. A text-editing request enables focus and the Windows host transfers it, remembering the previous foreground window. | `src/crates/host/src/hud/generic.rs`, `text_focus.rs`, `windows.rs` |
| Windows: what does the reference's capture code test? | It renders a dedicated WebView2 picture surface with `CapturePreview`. This path does not set display affinity or test recording exclusions. | `src/crates/host/src/capture.rs`, `capture/windows.rs` |
| Windows: what goes into the pipe name? | A channel and a hash of the user SID and canonical data folder. The server requires the first instance, uses a user-and-SYSTEM DACL, and rejects remote clients. The name has no Windows session component. | `src/crates/platform/src/os/windows/pipe.rs`, `acl.rs`, `src/crates/platform/src/instance/windows.rs` |
| Windows: how do hook executable paths reach the client? | Claude receives a command and an argument array. Codex receives a `commandWindows` override containing a PowerShell script encoded as UTF-16LE. | `src/crates/agents/src/hooks/windows.rs` |
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

1. **Claude, requests.** Done in the Desktop Code tab for a project-level
   entry (C-6). User-scope hooks, a session-only rule, and question and plan
   replies were exercised in the isolated engine (C-21, C-22). The Code
   tab loaded user settings and returned the hook's answer (C-24).
   A workspace hook approved a live Code-tab plan (C-25). Its first
   session-only rule trial prompted again on the second write. The
   scoped `Edit` rule worked in the Manual-mode Desktop repeat (C-27).
2. **Claude.** Words were observed with a development module (C-9, C-10).
   The MCP relay identified two Desktop Code-tab sessions (C-14).
   A new session was seeded and imported into Desktop (C-15). The owner
   refused a call in the plugin's own Desktop decision dialog (C-19).
   The first Desktop withdrawal trial delivered both prompts (C-20).
   In the held-turn repeat only KEEP reached the model (C-28).
   User-scope plugin loading worked in two isolated CLI sessions (C-23).
   An installed user-scope plugin loaded in a fresh Desktop Code-tab
   session (C-25).
3. **Codex.** Words (X-5), requests (X-8), opening a new thread (X-14),
   withdrawing a queued message (X-15) and interrupting (X-16) were
   observed. The bundled Desktop app-server identified its MCP caller
   (X-17). A wake-only link delivered the first queued message to each
   of two new threads with Desktop already running (X-18).
   Session-only command approval worked via app-server stdio (X-19).
   The Desktop owner pipe honored the same decision for a fixed command
   (X-20). Desktop acknowledged declines for a command and a file change
   (X-22). The Desktop-owned engine completed the relay handshake and
   provided caller identity in two chats (X-21). Both tested inline cards
   showed allow-once and decline (X-23). The installed renderer maps
   the additional dropdown action to different protocol decisions for
   commands and file changes (X-24). A file-change session grant
   covered the same path in a later Desktop turn (X-25).
4. **Antigravity.** State, words and requests were observed (A-7, A-8, A-10).
   The MCP relay identified two Desktop conversations (A-13). The
   Windows Desktop service created a conversation, withdrew a pending
   message and interrupted an invocation (A-14 to A-16).
   The scoped workspace `PreToolUse` hook passed an authenticated Desktop
   trial for denial and automatic approval of the fixed control (A-18).
   The native executable at the Unicode path passed the same pair
   in one authenticated Desktop conversation (A-19).
5. **DeepSeek Harness.** The owner removed the original probe block (D-4
   to D-8). The isolated Desktop Host verified creation, queue withdrawal,
   interrupt and a synthetic denial (D-10 to D-13). The preflight
   verified rejection and dispatch using the native PowerShell tool
   with a stand-in answerer (D-14). In the Desktop window, the owner saw
   the native approval card during the race and after the delayed
   handoff. The denied command did not dispatch and the control ran
   (D-15). The installed Desktop Host and its actual stdio MCP client
   identified two callers through a probe plugin (D-16). The owner
   confirmed the normal Desktop launch after removal (D-17). The installed
   Desktop shell repeated creation, withdrawal, interrupt and MCP caller
   identification with its own isolated Host (D-18).
