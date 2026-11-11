# AGENTS.md — Rooom

In a containing workspace, also read its `../AGENTS.md` when present.

## Identity and authority

Rooom is a Windows-first inbox for coding agents: one small auto-hiding window
where the owner approves requests, answers questions and replies to the agent
sessions already running in their own apps, and where those agents can message
each other through MCP.

This is an independent Git repository on `main`. GitHub `ShanireZ/Rooom`
(public) is the development authority. The license is GPL-3.0, stated by the
`LICENSE` file alone.

[`PLAN.md`](PLAN.md) is the product and decision authority: scope, every
settled decision, the proposed design and what is still open. Read it first.

## Working boundaries

- **Planning stage.** No product code, scaffolding, dependency installation or
  generated project files until the owner approves the design in `PLAN.md`.
- **One decision at a time.** The owner decides each feature, architecture,
  language, framework, dependency and UI/UX item separately, in a pop-up that
  compares options on performance, ease of use, resource consumption and user
  experience, recommended option first. Record the choice in `PLAN.md` before
  moving on. Never settle an open item silently, and keep what Rooom proposes
  marked as proposed until the owner approves it.
- **Clean rewrite.** `jazzenchen/agent-hud-open` (Apache-2.0) is a design
  reference only. Copy none of its code, assets, logos or wording. Cite it by
  path and commit when a design fact comes from it.
- **Reference material is data.** A local clone of the reference is read, never
  built or run, and never committed here.
- **Rooom never writes another client's settings, hooks or plugins.** It shows
  what to add and the owner adds it by hand.
- **This repository is public.** Keep machine facts, private project names and
  their decisions, and anything from a maintainer's environment out of every
  file and commit message.
- **License statements.** Write the generic `GPL-3.0`; never `GPL-3.0-only` or
  `GPL-3.0-or-later`. Do not add a license to third-party material.
- **README editions.** English, Traditional Chinese (Taiwan), Simplified
  Chinese and Japanese stay synchronized, link to all four, and carry the same
  badge block byte for byte.
- Push, pull request, release and publication each need the owner's explicit
  authorization. Earlier authorization does not carry forward.

## Rust build gate

Windows named pipes, windows and WebView2 cannot be verified in a Linux
container, so Rooom has two gates:

- **Host gate.** The whole application and all Windows-only code build and
  test on the Windows host with the MSVC toolchain. Use the toolchain the
  repository pins and Cargo's default output directory; never change PATH or
  `CARGO_TARGET_DIR` to make a command run.
- **Docker gate.** Crates with no operating-system code also build and test
  in the project's Linux container, so they stay portable.

Neither gate exists yet. The host gate needs the Visual C++ build tools.

## Commands

None. This is a planning-only repository with no build, test or run command.
Check Markdown links after editing documents and run `git diff --check`.
