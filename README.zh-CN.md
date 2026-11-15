# Rooom

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

给你正在使用的编程 agent 一个小窗口：不必切换应用，就能批准、回答与回复。

## 状态

Windows 常驻骨架已可从源码构建。[PLAN.md](PLAN.md) 记录构建命令、验收结果与接下来的客户端适配器工作。

## 它会做什么

- 显示 Claude 桌面应用、Codex Desktop、Antigravity 与 DeepSeek Harness 里运行中的会话，以及哪一个正在等你。
- 在同一条时间线上批准请求、回答提问、发送回复，并显示每条消息的状态：排队中、已接收、已开始。
- 让 agent 之间通过 MCP 互传消息，每一条你都看得到。
- 不挡路：平时是屏幕边缘的一条细条，鼠标靠近或 agent 需要你时才滑出。先支持 Windows。

## 它不会做什么

- 运行或拥有对话。你的会话仍在各自的应用里。
- 修改其他应用的设置。Rooom 只显示要加入的内容，由你自己加入。
- 另存一份你的对话。
- 追踪额度或 token 用量。

## 预定技术栈

程序本体使用 Rust 与 Tauri 2，界面使用 React 19，agent 互传消息使用官方 Rust MCP SDK。

## 参考

设计参考了 [Agent HUD](https://github.com/jazzenchen/agent-hud-open)（Apache-2.0）。Rooom 从零编写，没有复制它的任何代码或素材。

## 许可

GPL-3.0。见 [LICENSE](LICENSE)。
