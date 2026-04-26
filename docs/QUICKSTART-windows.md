# Quickstart — Windows

Get a workgraph smoke task running on Windows in under a minute.
This guide assumes you already have `wg.exe` on your PATH.
If not, see [port-handoff.md](port-handoff.md) for install instructions.

## Prerequisites

- **Windows 10/11** (ARM64 or x86_64)
- **`wg.exe`** installed and on PATH — verify with:
  ```
  wg --version
  ```
- **Claude Code CLI** (`claude`) installed and on PATH
- **Git** installed (workgraph uses git worktrees for agent isolation)

## 1. Authenticate with Claude

```
claude login
```

Follow the OAuth flow in your browser. This gives workgraph the
credential it needs to spawn Claude-powered agents.

## 2. Initialise a workgraph

Navigate to your project directory (any git repo), then:

```
wg init
```

This creates a `.workgraph/` directory in your project root.

## 3. Add a smoke task

```
wg add "hello world" -d "echo hi and call wg done"
```

This creates a task called `hello-world` that an agent will pick up.

## 4. Start the daemon

```
wg service start
```

The daemon polls for ready tasks and spawns agents to work on them.
On first start, Windows may show a firewall prompt — allow it.

## 5. Watch it land

```
wg show hello-world
```

Re-run this until the status flips to `done`. The task log will show
the agent's output.

You can also watch all tasks:

```
wg list
```

Or monitor agents in real time:

```
wg agents
```

## 6. Stop the daemon

When you're finished:

```
wg service stop
```

## Troubleshooting

If the task stays `open` or an agent fails:

1. **Run diagnostics:**
   ```
   wg doctor
   ```
   This checks host tools, auth, and daemon state. Fix any errors it reports.

2. **Check the daemon log:**
   ```
   wg service log
   ```

3. **Retry a failed task:**
   ```
   wg retry hello-world
   ```

4. **For deeper issues**, see [port-handoff.md](port-handoff.md) for
   architecture details and known Windows-specific behaviours.
