# Rooom

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

給你正在使用的程式設計 agent 一個小視窗：不必切換應用程式，就能核准、回答與回覆。

## 狀態

規劃中。目前沒有程式碼，也沒有可安裝的東西。到目前為止的每個決定、提案中的設計與尚未決定的事項都在 [PLAN.md](PLAN.md)。

## 它會做什麼

- 顯示 Claude 桌面應用程式、Codex Desktop、Antigravity 與 DeepSeek Harness 裡執行中的工作階段，以及哪一個正在等你。
- 在同一條時間軸上核准請求、回答提問、送出回覆，並顯示每則訊息的狀態：排隊中、已接收、已開始。
- 讓 agent 之間透過 MCP 互傳訊息，每一則你都看得到。
- 不擋路：平時是螢幕邊緣的一條細條，滑鼠靠近或 agent 需要你時才滑出。先支援 Windows。

## 它不會做什麼

- 執行或擁有對話。你的工作階段仍在各自的應用程式裡。
- 修改其他應用程式的設定。Rooom 只顯示要加入的內容，由你自己加入。
- 另存一份你的對話。
- 追蹤額度或 token 用量。

## 預定技術棧

程式本體使用 Rust 與 Tauri 2，介面使用 React 19，agent 互傳訊息使用官方 Rust MCP SDK。

## 參考

設計參考了 [Agent HUD](https://github.com/jazzenchen/agent-hud-open)（Apache-2.0）。Rooom 從零撰寫，沒有複製它的任何程式碼或素材。

## 授權

GPL-3.0。見 [LICENSE](LICENSE)。
