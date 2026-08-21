# Ragtag

A CLI tool for parsing, querying, and managing `@tag(attr=value)` annotations embedded in plain text files.

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

6. **Create a new tagged file:**

    ```bash
    ragtag file touch --tag project --tag '@task(status=new)'
    ragtag file touch --path notes/idea.md --tag idea --edit
    ```

    `file touch` always creates a new file and fails if its target already
    exists. Without `--path`, the default is a UTC-named file such as
    `2026-08-21_12-33-52.md` under the configured file directory. On success,
    it prints the new file's full absolute path.

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

### `ragtag file touch [--path <FILE>] [--tag <TAG>]... [--edit]`

Creates exactly one new plain text file and prints its full absolute resolved
path to stdout, followed by a newline. Creation is exclusive: an existing file,
directory, symlink, or dangling symlink is rejected and never modified.

```bash
ragtag file touch
ragtag file touch --path notes/today.md
ragtag file touch --path ../shared/idea.md --tag idea --tag '@project(name=ragtag)'
ragtag file touch --path /absolute/path/note.md --edit
```

An explicit relative `--path` is resolved from the current working directory;
absolute paths and paths containing parent components are accepted. Missing
parent directories are created recursively. Without `--path`,
`files.default_directory` is resolved relative to the selected config file's
directory (or the startup working directory when no config exists), and
`files.filename_format` generates the filename in UTC. A collision fails
without overwriting, suffixing, or retrying.

Repeat `--tag` once per complete parser-valid tag. The leading `@` is optional.
After normalization, exact duplicates are removed while preserving first-seen
order. Tags are written from byte zero, one per line with no blank separator;
with no tags, the new file is empty.

`--edit` safely parses `EDITOR` as an executable and arguments without a shell,
then appends the new file path as the final argument and waits for the editor.
`EDITOR` is ignored when `--edit` is absent. Invalid editor configuration is
rejected before creation; if launching fails or the editor exits
unsuccessfully after creation, the file is retained and the command reports
failure without printing the path. With `--edit`, the path is printed only
after the editor exits successfully.

See the [CLI reference](docs/cli-reference.md#file-touch) and
[configuration reference](docs/configuration.md#file-creation) for details.

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
| `time` | Set, add, or subtract `worktime_spent`; the value is `N`, `+N`, or `-N` |
| `complete` | Mark a task as done |
| `activate` | Set a task's status to active |
| `deactivate` | Set a task's status to inactive |
| `block` | Set a task's status to blocked |
| `abandon` | Set a task's status to abandoned |
| `prioritize` | Set a task's priority (`prioritize <PRIORITY> <ID>`) |

Use `task time` for absolute or relative time tracking:

```bash
ragtag task time 4 a1b2c3d4e5f67890       # set worktime_spent to 4
ragtag task time +1.5 a1b2c3d4e5f67890    # add 1.5
ragtag task time -2 a1b2c3d4e5f67890      # subtract 2, clamped to 0
```

The numeric magnitude must be finite and non-negative. A missing current
`worktime_spent` starts at `0`. `--path <PATH>` selects the search location;
`--no-edit` prints the updated `@task(...)` without writing its file. Every
successful operation updates `time_last_updated`.

> **Subcommand prefix matching:** ragtag accepts unambiguous prefixes. For example, `ragtag su` resolves to `ragtag summary`, and `ragtag task li` resolves to `ragtag task list`. An exact configured alias takes precedence over top-level real-command prefix inference. See the [CLI Reference](docs/cli-reference.md#subcommand-prefix-matching) for details.

### Aliases

Define command aliases in your config file to create shorthands. Running `ragtag <alias>` expands the alias's `arguments` and runs it as if typed directly.

```yaml
# .ragtag.yaml
aliases:
  - name: "ts"
    arguments: "task summary"
  - names: ["active", "a"]
    arguments: "query task --filter status=active"
  - name: "active-count"
    arguments: "active --count"
  - names: ["quicknote", "qn"]
    arguments: "file touch --tag \"quicknote\" --edit"
```

```bash
ragtag ts                # → ragtag task summary
ragtag ts --path src     # → ragtag task summary --path src   (trailing args appended)
ragtag active-count      # → ragtag query task --filter status=active --count
```

Each definition uses either `name` or an ordered, nonempty `names` list.
Aliases participate in outer-command prefix inference and may compose when the
first configured argument exactly names another alias. Alias names are absent
from top-level help and may not collide with real commands. Configuration is
loaded once from the original command line, and a leading `--` disables alias
recognition. See the [configuration reference](docs/configuration.md#aliases)
for composition order, limits, boundary behavior, and error details.

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

Ragtag looks for `.ragtag.yaml`, `.ragtag.yml`, `ragtag.yaml`, or `ragtag.yml` (searched in that order of precedence) in the current directory and walks up the directory tree until it finds one (stopping at a directory containing a `.git` folder or the filesystem root).
Repository-local config is trusted input: review it before running ragtag in an
untrusted checkout because it controls aliases, discovery behavior, and
file-writing options. For automation, use a reviewed config via a leading
`--config <PATH>` or `RAGTAG_CONFIG`. Config files must be regular files no
larger than 1 MiB.
See the [configuration reference](docs/configuration.md) for full details.

## Documentation

* [Tag Syntax Reference](docs/tag-syntax.md) — complete tag format specification
* [Task Management Guide](docs/task-management.md) — using `@task` tags for task tracking
* [Configuration Reference](docs/configuration.md) — YAML config file options
* [CLI Reference](docs/cli-reference.md) — full command-line reference
