---
audience: [operator, developer]
doc_type: guide
status: current
last_verified: 2026-03-25
---

# Console Mode

**Control the verbosity of Moss's terminal output at runtime.**

---

## Overview

Moss writes structured events to the terminal while it runs. The console mode determines which events are displayed. Change the mode at runtime via the API or persist it in `moss.toml` so it survives restarts.

---

## Available Modes

| Mode | Output | Typical use |
|------|--------|-------------|
| `silent` | No console output | Windows service, systemd unit with no TTY |
| `minimal` | Startup and critical events only | Daemon default — headless stones |
| `informative` | Major lifecycle events (services starting, storage changes) | Interactive sessions, local development |
| `verbose` | Full debug output | Troubleshooting a specific issue |

The default is `minimal`.

---

## Get the Current Mode

```
GET /api/v1/console/mode
```

```json
{ "mode": "minimal" }
```

---

## Set the Mode

```
POST /api/v1/console/mode
```

### Request Body

```json
{
  "mode": "verbose",
  "persist": false,
  "timeout_minutes": 30
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | string | *(required)* | One of `silent`, `minimal`, `informative`, `verbose` |
| `persist` | bool | `false` | Save the mode to `moss.toml` so it survives restarts |
| `timeout_minutes` | integer | `30` | Auto-revert to the previous mode after this many minutes. Set to `0` for no timeout. |

### Response

```json
{
  "mode": "verbose",
  "previous_mode": "minimal",
  "timeout_minutes": 30,
  "persisted": false
}
```

After 30 minutes, Moss reverts to `minimal` automatically. This is useful for temporary debugging without accidentally leaving a stone in verbose mode.

---

## When to Change the Mode

- **Debugging a problem**: switch to `verbose`, reproduce the issue, then let the timeout revert it.
- **Headless deployment**: set `silent` with `persist: true` so the stone never writes to the terminal.
- **Live demo or monitoring**: set `informative` to see lifecycle events without debug noise.
