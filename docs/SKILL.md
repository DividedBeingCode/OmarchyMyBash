---
name: omarchy10k-wiki
description: Provides context from the Omarchy10k project wiki when modifying daemon, CLI, Bash adapter, Quattro plugin, config, protocol, or theme code. Use when working on any Omarchy10k component to understand architecture, dependencies, and integration points.
---

# Omarchy10k Wiki

Provides context from the project wiki when working on Omarchy10k. Read the relevant wiki page(s) before making changes to understand how components connect.

## Wiki Location

All wiki pages live in `omarchy10k/docs/wiki/` relative to the workspace root.

## Page Map

| Page | File | Covers |
|------|------|--------|
| Index | `omarchy10k/docs/wiki/INDEX.md` | Navigation, repo layout, quick start |
| Architecture | `omarchy10k/docs/wiki/architecture.md` | System design, data flows, component graph, segment plugin architecture, dependency graph |
| Daemon | `omarchy10k/docs/wiki/daemon.md` | `omarchy10kd` modules: server, config, theme, git, layout, render, segments |
| CLI | `omarchy10k/docs/wiki/cli.md` | `omarchy10k` subcommands, prompt client, doctor diagnostics |
| Bash Adapter | `omarchy10k/docs/wiki/bash-adapter.md` | Hook broker, daemon lifecycle, command timing, ble.sh mode |
| Quattro Plugin | `omarchy10k/docs/wiki/quattro.md` | Manifest, QML components, daemon IPC, config UI, Process/Socket components |
| Configuration | `omarchy10k/docs/wiki/config.md` | Every config key with type, default, valid values, implementation status |
| Protocol | `omarchy10k/docs/wiki/protocol.md` | NDJSON over Unix socket, all commands and responses, sequence diagrams |
| Theme | `omarchy10k/docs/wiki/theme.md` | Theme bridge template, hook, palette loading, color roles, known issues |
| Glossary | `omarchy10k/docs/wiki/glossary.md` | Terms, environment variables, file paths |

## When to Read Wiki Pages

| Working on... | Read these pages |
|---------------|-----------------|
| Daemon Rust code | Architecture, Daemon, Protocol |
| CLI Rust code | CLI, Protocol |
| Bash adapter | Bash Adapter, Protocol, Glossary (env vars) |
| Quattro plugin QML/JS | Quattro Plugin, Protocol, Configuration |
| Config changes | Configuration, Daemon (config section) |
| Theme system | Theme Integration, Daemon (theme section) |
| Adding a new segment | Daemon (segments section), Architecture (segment plugin architecture) |
| Protocol changes | Protocol, Architecture (data flow) |
| Debugging | Glossary (file paths, env vars), CLI (doctor section) |
| Any structural change | Architecture first, then specific page |

## How to Use

1. **Before changing code:** Read the wiki page(s) for the component you're modifying. The page tells you what connects to what, what's implemented vs stubbed, and what the design constraints are.

2. **During implementation:** Cross-reference the protocol page if you're changing IPC. Check the config page if you're adding/changing config keys. Check the glossary for environment variable dependencies.

3. **After implementation:** Update the wiki to reflect your changes. See the workspace rule `.cursor/rules/wiki-maintenance.mdc` for the update protocol.

## Quick Lookup Patterns

**"What config keys exist for X?"** → Read `omarchy10k/docs/wiki/config.md`, find the `[section]` header.

**"How does the daemon handle command Y?"** → Read `omarchy10k/docs/wiki/protocol.md` for the command reference, then `omarchy10k/docs/wiki/daemon.md` for the server module.

**"What environment variables affect Z?"** → Read `omarchy10k/docs/wiki/glossary.md`, Environment Variables table.

**"Where does file X live on disk?"** → Read `omarchy10k/docs/wiki/glossary.md`, File Paths table.

**"What's the data flow for prompt rendering?"** → Read `omarchy10k/docs/wiki/architecture.md`, Data Flow: Prompt Render section.

**"What Quickshell components does the plugin use?"** → Read `omarchy10k/docs/wiki/quattro.md`, Quickshell Imports section.
