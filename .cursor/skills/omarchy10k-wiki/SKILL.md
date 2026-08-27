# Omarchy10k Wiki

Provides context from the project wiki when working on Omarchy10k. Read the relevant wiki page(s) before making changes to understand how components connect.

## Wiki Location

All wiki pages live in `docs/wiki/` relative to the project root (`omarchy10k/`).

## Page Map

| Page | File | Covers |
|------|------|--------|
| Index | `docs/wiki/INDEX.md` | Navigation, repo layout, quick start |
| Architecture | `docs/wiki/architecture.md` | System design, data flows, component graph, segment plugin architecture, dependency graph |
| Daemon | `docs/wiki/daemon.md` | `omarchy10kd` modules: server, config, theme, git, layout, render, segments |
| CLI | `docs/wiki/cli.md` | `omarchy10k` subcommands, prompt client, doctor diagnostics |
| Bash Adapter | `docs/wiki/bash-adapter.md` | Hook broker, daemon lifecycle, command timing, ble.sh mode |
| Quattro Plugin | `docs/wiki/quattro.md` | Manifest, QML components, daemon IPC, config UI, Process/Socket components |
| Configuration | `docs/wiki/config.md` | Every config key with type, default, valid values, implementation status |
| Protocol | `docs/wiki/protocol.md` | NDJSON over Unix socket, all commands and responses, sequence diagrams |
| Theme | `docs/wiki/theme.md` | Theme bridge template, hook, palette loading, color roles, known issues |
| Glossary | `docs/wiki/glossary.md` | Terms, environment variables, file paths |

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

3. **After implementation:** Update the wiki to reflect your changes. See the project rule `.cursor/rules/wiki-maintenance.mdc` for the update protocol.

## Quick Lookup Patterns

**"What config keys exist for X?"** → Read `docs/wiki/config.md`, find the `[section]` header.

**"How does the daemon handle command Y?"** → Read `docs/wiki/protocol.md` for the command reference, then `docs/wiki/daemon.md` for the server module.

**"What environment variables affect Z?"** → Read `docs/wiki/glossary.md`, Environment Variables table.

**"Where does file X live on disk?"** → Read `docs/wiki/glossary.md`, File Paths table.

**"What's the data flow for prompt rendering?"** → Read `docs/wiki/architecture.md`, Data Flow: Prompt Render section.

**"What Quickshell components does the plugin use?"** → Read `docs/wiki/quattro.md`, Quickshell Imports section.
