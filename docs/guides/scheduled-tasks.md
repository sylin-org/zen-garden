# Scheduled Tasks in Offering Manifests

## Overview

Offering manifests can define scheduled tasks that run periodically inside the offering's container. This enables:
- Automatic maintenance operations (e.g., database optimization, cache cleanup)
- Periodic updates (e.g., blocklist refresh, index rebuild)
- Health monitoring tasks (e.g., integrity checks)
- Custom automation tasks

## Manifest Format

Tasks are defined as a YAML map in the manifest's `tasks` block:

```yaml
tasks:
  update-gravity:
    description: "Update Pi-hole blocklist database (gravity)"
    schedule: "0 3 * * 0"
    command: ["pihole", "-g"]
    category: maintenance
    timeout_secs: 900
```

### Task Definition Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `description` | string | Yes | - | Human-readable task description |
| `schedule` | string | Yes | - | Cron expression (5 or 6 fields) |
| `command` | array | Yes | - | Command to execute inside container |
| `category` | enum | No | `maintenance` | Task category (see below) |
| `enabled` | bool | No | `true` | Whether task is active |
| `timeout_secs` | number | No | `300` | Max execution time in seconds |

### Task Categories

- `maintenance` - Updates, cleanup, optimization tasks
- `backup` - Backup operations
- `health` - Health/integrity checks
- `custom` - Other tasks

### Cron Schedule Format

Standard 5-field cron format:

```
┌───────────── minute (0-59)
│ ┌───────────── hour (0-23)
│ │ ┌───────────── day of month (1-31)
│ │ │ ┌───────────── month (1-12)
│ │ │ │ ┌───────────── day of week (0-6, 0=Sunday)
│ │ │ │ │
* * * * *
```

Examples:
- `0 3 * * *` - Daily at 3:00 AM
- `0 3 * * 0` - Weekly on Sunday at 3:00 AM
- `0 */6 * * *` - Every 6 hours
- `30 2 1 * *` - Monthly on 1st at 2:30 AM

## Task Lifecycle

### Registration

Tasks are automatically registered when an offering is installed:

1. During installation, Moss reads the manifest's `tasks` block
2. For each task, a `ScheduledTask` entry is created
3. Tasks are stored in `{config_dir}/tasks/registry.json`
4. The scheduler calculates the next run time from the cron expression

### Execution

The task scheduler runs in the background:

1. Every 60 seconds, it checks for due tasks
2. For each due task where the container is running:
   - Executes the command via `docker exec`
   - Records the result (exit code, output, duration)
   - Calculates the next run time
3. Results are persisted to the task registry

### Cleanup

Tasks are automatically removed when an offering is deleted:

1. On `DELETE /api/v1/services/:name` or destroy
2. All tasks for that `offering_id` are unregistered
3. Registry is updated

### Backfill

At startup, Moss backfills missing tasks:

1. For each installed offering
2. Check if manifest defines tasks
3. Register any tasks not already in the registry
4. This ensures tasks survive restarts and updates

## Code Structure

### Types (garden-common)

```rust
// Task category enum
pub enum TaskCategory {
    Maintenance,
    Backup,
    Health,
    Custom,
}

// Task definition from manifest
pub struct TaskDefinition {
    pub description: String,
    pub schedule: String,
    pub command: Vec<String>,
    pub category: TaskCategory,
    pub enabled: bool,
    pub timeout_secs: u64,
}

// Registered task instance
pub struct ScheduledTask {
    pub task_id: String,      // "{offering_id}:{task_name}"
    pub offering_id: String,
    pub offering_name: String,
    pub task_name: String,
    pub definition: TaskDefinition,
    pub registered_at: String,
    pub last_run: Option<String>,
    pub last_result: Option<TaskResult>,
    pub next_run: Option<String>,
}

// Execution result
pub struct TaskResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub error: Option<String>,
}
```

### Infrastructure (moss)

- `infra/task_store.rs` - Persistence layer (TaskStore, TaskRegistry)
- `tasks/task_scheduler.rs` - Background scheduler and execution

## Examples

### Pi-hole Blocklist Update

```yaml
# pihole.snippet.yaml
container_name: pihole
image: pihole/pihole:2024.07.0
ports:
  default: [53, 53]
  admin: [8053, 80]
# ... other config ...
tasks:
  update-gravity:
    description: "Update Pi-hole blocklist database (gravity)"
    schedule: "0 3 * * 0"  # Weekly on Sunday at 3 AM
    command: ["pihole", "-g"]
    category: maintenance
    timeout_secs: 900  # 15 minutes
```

### Database Maintenance

```yaml
# postgres.snippet.yaml
tasks:
  vacuum-analyze:
    description: "Run VACUUM ANALYZE on all databases"
    schedule: "0 2 * * *"  # Daily at 2 AM
    command: ["psql", "-U", "postgres", "-c", "VACUUM ANALYZE;"]
    category: maintenance
    timeout_secs: 1800  # 30 minutes

  reindex:
    description: "Reindex system catalogs"
    schedule: "0 3 * * 0"  # Weekly on Sunday at 3 AM
    command: ["reindexdb", "-U", "postgres", "--system"]
    category: maintenance
    timeout_secs: 3600  # 1 hour
```

### Multiple Tasks

```yaml
# elasticsearch.snippet.yaml
tasks:
  flush-caches:
    description: "Flush translog and file system caches"
    schedule: "0 4 * * *"  # Daily at 4 AM
    command: ["curl", "-X", "POST", "localhost:9200/_flush"]
    category: maintenance
    timeout_secs: 300

  force-merge:
    description: "Force merge indices for optimization"
    schedule: "0 5 * * 0"  # Weekly on Sunday at 5 AM
    command: ["curl", "-X", "POST", "localhost:9200/_forcemerge?max_num_segments=1"]
    category: maintenance
    timeout_secs: 7200  # 2 hours
```

## Best Practices

1. **Set reasonable timeouts**: Consider worst-case execution time
2. **Stagger schedules**: Don't run all tasks at the same time
3. **Use descriptive names**: Task names become part of the task ID
4. **Log output wisely**: Output is truncated at 10KB
5. **Handle failures gracefully**: Tasks should be idempotent
6. **Test commands locally first**: Verify commands work in the container

## Monitoring

Task execution is logged with tracing:

```
INFO  Executing scheduled task task_id="offer-123:update-gravity" container="pihole"
INFO  Task completed successfully task_id="offer-123:update-gravity" duration_ms=45230
```

Failed tasks are logged as warnings:

```
WARN  Task completed with non-zero exit code task_id="offer-123:update-gravity" exit_code=1
ERROR Task execution failed task_id="offer-123:update-gravity" error="timeout after 900s"
```
