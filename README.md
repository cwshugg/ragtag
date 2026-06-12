# Ragtag

A Rust CLI tool for parsing, querying, and managing `@tag(attr=value)` annotations embedded in plain text files.

I created this to make tagging things in my notes quick and easy, while supporting a drop-in-anywhere structured syntax that is easy to understand.
Ragtag will:

* Scan your notes, documentation, or *any* plain text file, for structured `@tag` syntax
* Let you search for tags across all your files
* Summarize the tags found, *where* they were found, *how many* of each there are, etc.
* Track your tasks/to-dos by providing a rich interface for a custom `@task` tag.

This is all wrapped into an intuitive CLI, configurable by a YAML file.

If you use Vim, check out the [ragtag.vim](https://github.com/cwshugg/ragtag.vim) plugin I created for this.

## Installation

Build and install from source with [Cargo](https://doc.rust-lang.org/cargo/):

```bash
# Clone the repository
git clone https://github.com/cwshugg/ragtag.git
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

1. **Add tags to your notes.** Put `@tag` or `@tag(attribute_name=attribute_value)` anywhere in your plain text files:

    ```
    Meeting notes for 2026-06-10.
    @topic(name="architecture review")

    @task(
        id="a1b2c3d4e5f67890",
        title="Refactor parser module",
        status="active",
        worktime_estimate=4,
        worktime_units="hours"
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

    # Or, enter the fields one-by-one via stdin:
    ragtag task create
    ```

    This prints an `@task(...)` string to stdout for you to copy into a note file.
    Integrate this with other tools to generate the `@task(...)` string and drop it straight into your other notes.

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
| `create` | Generate a new `@task(...)` string (interactive when `--title` is omitted) |
| `list` | List tasks found in files |
| `get` | Look up a task by ID or title |
| `summary` | Display a grouped summary of tasks (default grouping: priority) |
| `get-attr` | Print a single task attribute value |
| `set-attr` | Update a single task attribute |
| `complete` | Mark a task as done |
| `activate` | Set a task's status to active |
| `deactivate` | Set a task's status to inactive |
| `block` | Set a task's status to blocked |
| `abandon` | Set a task's status to abandoned |
| `prioritize` | Set a task's priority (`prioritize <PRIORITY> <ID>`) |

> **Subcommand prefix matching:** ragtag accepts any unambiguous prefix of every subcommand. For example, `ragtag su` resolves to `ragtag summary`, and `ragtag t li` resolves to `ragtag task list`. See the [CLI Reference](docs/cli-reference.md#subcommand-prefix-matching) for details.

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

Ragtag looks for `.ragtag.yaml` or `ragtag.yaml` in the current directory and walks up the directory tree until it finds one (stopping at a directory containing a `.git` folder or the filesystem root).
See the [configuration reference](docs/configuration.md) for full details.

## Documentation

* [Tag Syntax Reference](docs/tag-syntax.md) — complete tag format specification
* [Task Management Guide](docs/task-management.md) — using `@task` tags for task tracking
* [Configuration Reference](docs/configuration.md) — YAML config file options
* [CLI Reference](docs/cli-reference.md) — full command-line reference

