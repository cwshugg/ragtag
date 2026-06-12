# ragtag

A Rust CLI tool for parsing, querying, and managing `@tag(attr=value)` annotations embedded in plain text files.

ragtag scans your notes, documentation, and other plain text files for structured `@tag` syntax, then lets you summarize, query, and filter what it finds. It also includes a built-in **task manager** powered by `@task` tags, so you can track tasks right inside your notes.

## Features

* **Tag discovery** — recursively scans directories for `@tag` and `@tag(attr=value)` syntax
* **Summary** — shows a table of all unique tags and their counts
* **Query** — search for specific tags with attribute filtering (`=`, `!=`, `>`, `<`, `>=`, `<=`)
* **Task management** — create, list, update, and organize `@task` tags with status tracking, priorities, time estimates, and parent-child relationships
* **Colored output** — status and priority values are color-coded in the terminal
* **Configurable** — YAML config file with ignore patterns, output settings, and task extension options

## Installation

Build and install from source with [Cargo](https://doc.rust-lang.org/cargo/):

```bash
# Clone the repository
git clone <repo-url>
cd ragtag

# Build and install
cargo install --path .
```

Or build without installing:

```bash
cargo build --release
# Binary is at target/release/ragtag
```

## Quick Start

1. **Add tags to your notes.** Put `@tag` or `@tag(key=value)` anywhere in your plain text files:

    ```
    Meeting notes for 2026-06-10.
    @topic(name="architecture review")

    @task(
        id="a1b2c3d4e5f67890",
        title="Refactor parser module",
        status="active",
        worktime_estimate=4,
        workworktime_units="hours"
    )
    ```

2. **Summarize all tags** in the current directory:

    ```bash
    ragtag summary
    ```

3. **Query for specific tags:**

    ```bash
    ragtag query topic
    ragtag query task --filter status=active
    ```

4. **List all tasks:**

    ```bash
    ragtag task list
    ```

5. **Create a new task:**

    ```bash
    ragtag task create --title "Write docs" --worktime-estimate 2 --worktime-units hours
    ```

    This prints an `@task(...)` string to stdout for you to copy into a note file.

## Commands

### `ragtag summary`

Shows a count of each unique tag found across all scanned files.

```bash
ragtag summary
ragtag summary --path ./notes
```

### `ragtag query <TAG_NAME>`

Searches for tags matching a name and prints their locations.

```bash
ragtag query todo
ragtag query task --filter status=active --filter priority=0
ragtag query task --count
```

### `ragtag config get <KEY>`

Prints the value of a config field using dot-notation. Useful for scripts and editor plugins that need to read ragtag configuration without parsing YAML.

```bash
ragtag config get max_depth
ragtag config get tasks.tag_name
ragtag config get tasks.status_keywords.done
```

### `ragtag task <subcommand>`

Task management commands. See the [task management guide](docs/task-management.md) for full details.

| Subcommand | Description |
| --- | --- |
| `create` | Generate a new `@task(...)` string |
| `list` | List tasks found in files |
| `get` | Look up a task by ID or title |
| `summary` | Display a grouped summary of tasks |
| `get-attr` | Print a single task attribute value |
| `set-attr` | Update a single task attribute |

## Global Flags

| Flag | Description |
| --- | --- |
| `--config <PATH>` | Path to a config file (overrides auto-discovery) |
| `--no-color` | Disable colored output |
| `--version` | Print version information |

## Environment Variables

| Variable | Description |
| --- | --- |
| `RAGTAG_CONFIG` | Path to the ragtag config file. Alternative to `--config`. The CLI flag takes precedence over this variable. If neither is set, ragtag uses walk-up config discovery. |
| `RAGTAG_PATH` | Default search path for tags and tasks. Alternative to `--path`. The CLI flag takes precedence over this variable. If neither is set, defaults to `.` (current directory). |
| `RUST_LOG` | Controls log verbosity (e.g., `RUST_LOG=info` or `RUST_LOG=debug`). Uses the `env_logger` crate format. |
| `NO_COLOR` | When set, disables colored output. Overrides the `output.color` config setting but is itself overridden by the `--no-color` CLI flag. |

**Precedence:** CLI flag > environment variable > default value.

## Configuration

ragtag looks for `.ragtag.yaml` or `ragtag.yaml` in the current directory and walks up the directory tree until it finds one (stopping at a directory containing a `.git` folder or the filesystem root). See the [configuration reference](docs/configuration.md) for full details.

## Documentation

* [Tag Syntax Reference](docs/tag-syntax.md) — complete tag format specification
* [Task Management Guide](docs/task-management.md) — using `@task` tags for task tracking
* [Configuration Reference](docs/configuration.md) — YAML config file options
* [CLI Reference](docs/cli-reference.md) — full command-line reference

## License

MIT
