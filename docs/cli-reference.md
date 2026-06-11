# CLI Reference

Complete command-line reference for ragtag.

## Synopsis

```
ragtag [OPTIONS] <COMMAND>
```

## Global Options

| Option | Description |
| --- | --- |
| `--config <PATH>` | Path to config file (overrides auto-discovery) |
| `--no-color` | Disable colored output |
| `--version` | Print version information |
| `--help`, `-h` | Print help information |

These options are global and can be placed before any subcommand.

## Commands

### `summary`

Show a summary of all tags found.

```
ragtag summary [OPTIONS]
```

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |

**Output:**

Prints a table with one row per unique tag name, showing the count of occurrences. For tags with registered extensions (e.g., `task`), an additional breakdown is appended.

```
Tag    Count
---    -----
note   12
task   8 (3 active, 2 done, 1 blocked, 2 inactive)
todo   5
```

### `query`

Search for specific tags.

```
ragtag query <TAG_NAME> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `TAG_NAME` | Yes | Tag name to search for (without `@`) |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--filter <EXPR>` | — | Filter by attribute (repeatable). Supported operators: `=`, `!=`, `>`, `<`, `>=`, `<=` |
| `--count` | — | Print only the count of matching tags |

**Output (default):**

Grep-style output with file path, line number, and the full tag:

```
notes/ideas.md:15: @todo(priority=1, owner="alice")
notes/bugs.md:42: @todo(priority=0, owner="bob")
```

**Output with `--count`:**

```
2
```

**Filter operators:**

| Operator | Example | Description |
| --- | --- | --- |
| `=` | `status=active` | Equal |
| `!=` | `status!=done` | Not equal |
| `>` | `priority>0` | Greater than (numeric) |
| `<` | `ttc_estimate<8` | Less than (numeric) |
| `>=` | `priority>=1` | Greater than or equal (numeric) |
| `<=` | `ttc_estimate<=4` | Less than or equal (numeric) |

Numeric comparisons parse both sides as `f64`. If parsing fails, the comparison returns false.

### `task`

Track and manage tasks embedded in plain text files. Requires a subcommand.

```
ragtag task <SUBCOMMAND>
```

#### `task create`

Create a new task and print the `@task(...)` string to stdout.

```
ragtag task create [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--title <STR>` | Task title (required unless `--interactive`) |
| `--description <STR>` | Task description |
| `--owner <STR>` | Task owner |
| `--status <STR>` | Task status |
| `--priority <NUM>` | Priority (`0` = highest) |
| `--ttc-estimate <NUM>` | Time-to-complete estimate |
| `--time-units <STR>` | Time units: `hours`, `days`, or `weeks` |
| `--pid <STR>` | Parent task ID |
| `-i`, `--interactive` | Launch interactive prompt for all fields |

**Output:**

Prints a multi-line `@task(...)` string to stdout:

```
@task(
    id="a1b2c3d4e5f67890",
    title="Write documentation",
    owner="me",
    status="new",
    ttc_estimate=4,
    time_units="hours"
)
```

The task ID is a randomly-generated 16-character hex string.

#### `task list`

List tasks found in files.

```
ragtag task list [OPTIONS]
```

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--filter <EXPR>` | — | Filter tasks by attribute (repeatable) |
| `--sort <FIELD>` | — | Sort by field name |
| `--reverse` | — | Reverse sort order |
| `--all`, `-a` | — | Show all tasks, including excluded status categories (done, abandoned) |

**Output:**

One task per line, showing the file path and selected attributes:

```
notes/project.md id="a1b2c3d4e5f67890" status="active" title="Design API" description="REST API design"
notes/bugs.md id="f0e1d2c3b4a59687" status="blocked" title="Fix parser bug"
```

#### `task get`

Look up a task by ID (exact or prefix) or title substring.

```
ragtag task get <SEARCH_STRING> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `SEARCH_STRING` | Yes | Task ID, ID prefix, or title substring to search for |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--all`, `-a` | — | Show all tasks, including excluded status categories (done, abandoned) |
| `--config <PATH>` | — | Path to config file (overrides auto-discovery) |
| `--no-color` | — | Disable colored output |

**Examples:**

```bash
# Look up a task by ID
ragtag task get a1b2c3d4e5f67890

# Look up by ID prefix
ragtag task get a1b2

# Search by title substring
ragtag task get "parser bug"
```

#### `task summary`

Display a table-like summary of tasks grouped by field.

```
ragtag task summary [OPTIONS]
```

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--group <FIELD>` | `status` | Group tasks by field: `status`, `owner`, or `priority` |
| `--sort <FIELD>` | — | Sort tasks within each group by any task field name |
| `--filter <EXPR>` | — | Filter tasks by attribute (e.g., `status=active`). Repeatable |
| `--all`, `-a` | — | Show all tasks, including excluded status categories (done, abandoned) |

**Output:**

Tasks are displayed in aligned tables, grouped by the specified field. Each group has a header (e.g., `=== Status: active ===`).

Columns: ID, Title, Owner, Status, Priority, Time Spent, TTC Est., TTC Act., Time Units.

Status values are color-coded and priority `0` is shown in red.

#### `task set-status`

Update a task's status.

```
ragtag task set-status --id <ID> [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--id <ID>` | Task ID (required) |
| `--status <VALUE>` | New status (prompted interactively if omitted) |
| `--path <PATH>` | Search path (default: `.`) |

**Behavior:**

* Finds the task by ID across all scanned files
* Validates the new status against recognized keywords
* Edits the source file in-place (atomic write via temp file)
* Prints the updated task details

#### `task set-time`

Update a task's `time_spent`.

```
ragtag task set-time --id <ID> [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--id <ID>` | Task ID (required) |
| `--time <VALUE>` | New `time_spent` value (prompted interactively if omitted) |
| `--path <PATH>` | Search path (default: `.`) |

#### `task set-owner`

Update a task's owner.

```
ragtag task set-owner --id <ID> [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--id <ID>` | Task ID (required) |
| `--owner <VALUE>` | New owner (prompted interactively if omitted) |
| `--path <PATH>` | Search path (default: `.`) |

#### `task set-parent`

Update a task's parent ID.

```
ragtag task set-parent --id <ID> [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--id <ID>` | Task ID (required) |
| `--pid <VALUE>` | New parent task ID (prompted interactively if omitted) |
| `--path <PATH>` | Search path (default: `.`) |

#### `task set-priority`

Update a task's priority.

```
ragtag task set-priority --id <ID> [OPTIONS]
```

**Options:**

| Option | Description |
| --- | --- |
| `--id <ID>` | Task ID (required) |
| `--priority <VALUE>` | New priority (non-negative integer, prompted interactively if omitted) |
| `--path <PATH>` | Search path (default: `.`) |

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Error (config not found, parse error, invalid filter, task not found, I/O error, etc.) |

All errors are printed to stderr with a descriptive message.

## Environment Variables

| Variable | Description |
| --- | --- |
| `RUST_LOG` | Controls log verbosity (e.g., `RUST_LOG=info` or `RUST_LOG=debug`). Uses the `env_logger` crate format. |
| `NO_COLOR` | When set, disables colored output. Overrides the `output.color` config setting but is itself overridden by the `--no-color` CLI flag. |

> **Note:** When `output.color` is set to `auto` (the default), colors are automatically disabled when stdout is not a terminal (e.g., when piping to another command or redirecting to a file).

## File Editing Safety

The `set-*` commands modify files using **atomic writes**: the updated content is written to a temporary file first, then moved into place. This prevents data loss from interrupted writes.

ragtag **refuses to edit symlinked files** — you must resolve the symlink or edit the target file directly.
