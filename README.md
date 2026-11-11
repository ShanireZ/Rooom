# Rooom

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

One small window for the coding agents you already run: approve, answer and reply without switching apps.

## Status

Planning. There is no code yet and nothing to install. Every decision made so far, the proposed design and what is still open are in [PLAN.md](PLAN.md).

## What it will do

- Show the sessions running in the Claude desktop app, Codex Desktop, Antigravity and DeepSeek Harness, and which of them is waiting for you.
- Let you approve a request, answer a question and send a reply from one timeline, with the state of each message: queued, received, started.
- Let the agents message each other through MCP, with every message visible to you.
- Stay out of the way: a thin strip on a screen edge that slides out when you come near or when an agent needs you, on Windows first.

## What it will not do

- Run or own a conversation. Your sessions stay in their own apps.
- Change another app's settings. Rooom shows what to add and you add it.
- Keep a copy of your conversations.
- Track quotas or token usage.

## Planned stack

Rust and Tauri 2 for the program, React 19 for the interface, the official Rust MCP SDK for agent messages.

## Reference

The design is informed by [Agent HUD](https://github.com/jazzenchen/agent-hud-open) (Apache-2.0). Rooom is written from scratch and copies none of its code or assets.

## License

GPL-3.0. See [LICENSE](LICENSE).
