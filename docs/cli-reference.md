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
| `<` | `worktime_estimate<8` | Less than (numeric) |
| `>=` | `priority>=1` | Greater than or equal (numeric) |
| `<=` | `worktime_estimate<=4` | Less than or equal (numeric) |

Numeric comparisons parse both sides as `f64`. If parsing fails, the comparison returns false.

### `config`

Inspect ragtag configuration. Requires a subcommand.

```
ragtag config <SUBCOMMAND>
```

#### `config get`

Print the value of a config field using dot-notation. Useful for external tools and editor plugins that need to read ragtag configuration programmatically.

```
ragtag config get <KEY>
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `KEY` | Yes | Config key in dot-notation (e.g., `max_depth`, `tasks.tag_name`, `tasks.status_keywords.done`) |

**Output:**

Prints the resolved value to stdout. Strings are printed without quotes, numbers and booleans as-is, sequences in bracket notation, and mappings in brace notation.

**Examples:**

```bash
ragtag config get max_depth             # null (when unset)
ragtag config get max_file_size         # 10485760
ragtag config get respect_gitignore     # true
ragtag config get output.color          # auto
ragtag config get tasks.tag_name        # task
ragtag config get tasks.default_owner   # me
ragtag config get ignore_patterns       # ["*.git", "node_modules"]
ragtag config get tasks.status_keywords.done  # ["done", "finished", "complete", "completed"]
ragtag config get nonexistent_field     # error: unknown config key "nonexistent_field"
```

Extension configs (like `tasks`) are resolved with defaults applied, so all fields are available even if not explicitly set in the YAML file.

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
| `--worktime-estimate <NUM>` | Time-to-complete estimate |
| `--worktime-spent <NUM>` | Worktime already spent (default: `0`) |
| `--worktime-units <STR>` | Time units: `hours`, `days`, or `weeks` |
| `--pid <STR>` | Parent task ID |
| `--format <FORMAT>` | Output format: `multiline` (default) or `oneline` |
| `-i`, `--interactive` | Launch interactive prompt for all fields |

**Output:**

Prints an `@task(...)` string to stdout. With `--format multiline` (default):

```
@task(
    id="a1b2c3d4e5f67890",
    title="Write documentation",
    owner="me",
    status="new",
    worktime_spent=0,
    worktime_estimate=4,
    worktime_units="hours"
)
```

With `--format oneline`:

```
@task(id="a1b2c3d4e5f67890", title="Write documentation", owner="me", status="new", worktime_spent=0, worktime_estimate=4, worktime_units="hours")
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
| `--filter <EXPR>` | — | Filter tasks by attribute expression (e.g., `"status=active AND priority<=2"`). Supports `AND`, `OR`, and parentheses |
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
| `--group <FIELD>` | `priority` | Group tasks by field: `status`, `owner`, or `priority` |
| `--sort <FIELD>` | — | Sort tasks within each group by any task field name |
| `--filter <EXPR>` | — | Filter tasks by attribute expression (e.g., `"status=active AND priority<=2"`). Supports `AND`, `OR`, and parentheses |
| `--format <FORMAT>` | `table` | Output format: `table` (aligned columns) or `list` (multi-line per task) |
| `--all`, `-a` | — | Show all tasks, including excluded status categories (done, abandoned) |

**Output:**

Tasks are displayed in aligned tables, grouped by the specified field. Each group has a header (e.g., `Status: active`).

With `--format table` (default), columns are: Path, Title, Owner, Status, Priority, Time, ID.

With `--format list`, each task is shown as three lines: file path, truncated title, and a detail line with ID, owner, priority, status, and time. Tasks are separated by blank lines.

Status values are color-coded and priority `0` is shown in red.

#### `task get-attr`

Get the value of a single task attribute.

```bash
ragtag task get-attr <ID> <ATTR> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |
| `ATTR` | Yes | Attribute name: `title`, `description`, `owner`, `status`, `priority`, `worktime_spent`, `worktime_estimate`, `time_created`, `time_last_updated`, `worktime_units`, `pid`, `id` |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |

**Output:**

Prints the raw attribute value with no label or formatting. For `Option` fields that are `None`, prints nothing (empty output).

**Examples:**

```bash
ragtag task get-attr a1b2c3d4e5f67890 status      # active
ragtag task get-attr a1b2c3d4e5f67890 priority    # 1
ragtag task get-attr a1b2c3d4e5f67890 title       # Design API
ragtag task get-attr a1b2c3 status                # prefix match
```

#### `task set-attr`

Set the value of a single task attribute.

```bash
ragtag task set-attr <ID> <ATTR> <VALUE> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |
| `ATTR` | Yes | Attribute name (same as `get-attr`, except `id` which is immutable) |
| `VALUE` | Yes | New value for the attribute |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Finds the task by ID across all scanned files
* Validates the new value against attribute-specific rules (e.g., status must be a recognized keyword, priority must be a non-negative integer)
* Edits the source file in-place (atomic write via temp file)
* Prints a confirmation message: `Updated <ATTR> to "<VALUE>" for task <ID>`

**With `--no-edit`:**

* Does not modify the file
* Prints the complete reconstructed `@task(...)` string with the attribute changed
* Useful for editor plugin integration (e.g., Vim plugin injects the string into the buffer)

**Examples:**

```bash
# Update status
ragtag task set-attr a1b2c3d4e5f67890 status done

# Update priority
ragtag task set-attr a1b2c3d4e5f67890 priority 0

# Update owner
ragtag task set-attr a1b2c3d4e5f67890 owner alice

# Update time spent
ragtag task set-attr a1b2c3d4e5f67890 worktime_spent 6.5

# Update parent ID
ragtag task set-attr a1b2c3d4e5f67890 pid f0e1d2c3b4a59687

# Get updated tag string without modifying the file
ragtag task set-attr a1b2c3d4e5f67890 status done --no-edit
```

#### `task complete`

Mark a task as complete by setting its status to the first configured done keyword (default: `"done"`).

```bash
ragtag task complete <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Finds the task by ID across all scanned files
* Sets `status` to the first keyword in `config.status_keywords.done` (default: `"done"`)
* Automatically sets `time_last_updated` to the current UTC time (ISO 8601); adds the field if it doesn't already exist
* Edits the source file in-place (atomic write via temp file)
* Prints a confirmation message: `Completed task <ID> (status → "done")`

**With `--no-edit`:**

* Does not modify the file
* Prints the complete reconstructed `@task(...)` string with the status and timestamp updated
* Useful for editor plugin integration (e.g., Vim plugin injects the string into the buffer)

**Examples:**

```bash
# Mark task as complete (modifies file in-place)
ragtag task complete a1b2c3d4e5f67890

# Mark task using an ID prefix
ragtag task complete a1b2c3

# Print the updated tag string without modifying the file
ragtag task complete a1b2c3d4e5f67890 --no-edit
```

---

#### `task activate`

Set a task's status to the first configured active keyword (default: `"active"`).

```bash
ragtag task activate <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Sets `status` to the first keyword in `config.status_keywords.active` (default: `"active"`)
* Automatically updates `time_last_updated` to the current UTC time
* Prints a confirmation: `Activated task <ID> (status → "active")`

**Examples:**

```bash
ragtag task activate a1b2c3d4e5f67890
ragtag task activate a1b2c3d4e5f67890 --no-edit
```

---

#### `task deactivate`

Set a task's status to the first configured inactive keyword (default: `"inactive"`).

```bash
ragtag task deactivate <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Sets `status` to the first keyword in `config.status_keywords.inactive` (default: `"inactive"`)
* Automatically updates `time_last_updated` to the current UTC time
* Prints a confirmation: `Deactivated task <ID> (status → "inactive")`

**Examples:**

```bash
ragtag task deactivate a1b2c3d4e5f67890
ragtag task deactivate a1b2c3d4e5f67890 --no-edit
```

---

#### `task block`

Set a task's status to the first configured blocked keyword (default: `"blocked"`).

```bash
ragtag task block <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Sets `status` to the first keyword in `config.status_keywords.blocked` (default: `"blocked"`)
* Automatically updates `time_last_updated` to the current UTC time
* Prints a confirmation: `Blocked task <ID> (status → "blocked")`

**Examples:**

```bash
ragtag task block a1b2c3d4e5f67890
ragtag task block a1b2c3d4e5f67890 --no-edit
```

---

#### `task abandon`

Set a task's status to the first configured abandoned keyword (default: `"abandoned"`).

```bash
ragtag task abandon <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Sets `status` to the first keyword in `config.status_keywords.abandoned` (default: `"abandoned"`)
* Automatically updates `time_last_updated` to the current UTC time
* Prints a confirmation: `Abandoned task <ID> (status → "abandoned")`

**Examples:**

```bash
ragtag task abandon a1b2c3d4e5f67890
ragtag task abandon a1b2c3d4e5f67890 --no-edit
```

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Error (config not found, parse error, invalid filter, task not found, I/O error, etc.) |

All errors are printed to stderr with a descriptive message.

## Environment Variables

| Variable | Description |
| --- | --- |
| `RAGTAG_CONFIG` | Path to the ragtag config file. Alternative to the `--config` CLI flag. The CLI flag takes precedence. If neither is set, ragtag uses walk-up config discovery (see [Configuration Reference](configuration.md)). |
| `RAGTAG_PATH` | Default search path for tags and tasks. Alternative to the `--path` CLI flag used by `summary`, `query`, and all `task` subcommands. The CLI flag takes precedence. If neither is set, defaults to `.` (current directory). |
| `RUST_LOG` | Controls log verbosity (e.g., `RUST_LOG=info` or `RUST_LOG=debug`). Uses the `env_logger` crate format. |
| `NO_COLOR` | When set, disables colored output. Overrides the `output.color` config setting but is itself overridden by the `--no-color` CLI flag. |

**Precedence order:** CLI flag > environment variable > default value.

For example, to always search a specific directory for tasks without passing `--path` every time:

```bash
export RAGTAG_PATH=~/notes
ragtag task list            # searches ~/notes
ragtag task list --path .   # overrides to current directory
```

Similarly, to use a specific config file without passing `--config`:

```bash
export RAGTAG_CONFIG=~/.config/ragtag/.ragtag.yaml
ragtag summary              # uses the config at the exported path
```

## File Editing Safety

The `set-attr` command modifies files using **atomic writes**: the updated content is written to a temporary file first, then moved into place. This prevents data loss from interrupted writes.

ragtag **refuses to edit symlinked files** — you must resolve the symlink or edit the target file directly.
