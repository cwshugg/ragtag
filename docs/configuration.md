# Configuration Reference

ragtag is configured via a YAML file. All settings have sensible defaults, so a config file is entirely optional.

## Config File Discovery

ragtag searches for a config file by walking up the directory tree from the current working directory. At each level it checks the following names in order, using the first that exists:

1. `.ragtag.yaml`
2. `.ragtag.yml`
3. `ragtag.yaml`
4. `ragtag.yml`

Both the `.yaml` and `.yml` extensions are searched. A dotfile takes precedence over a non-dotfile, and within the same base name `.yaml` takes precedence over `.yml`.

The search **stops** when it reaches a directory containing a `.git` folder or the filesystem root. If no config file is found, built-in defaults are used.

### Override With `--config`

You can skip auto-discovery and specify an explicit config file path:

```bash
ragtag --config /path/to/.ragtag.yaml summary
```

If the specified file does not exist, ragtag exits with an error.

### Override With `RAGTAG_CONFIG`

Alternatively, set the `RAGTAG_CONFIG` environment variable to specify a config file path without passing `--config` every time:

```bash
export RAGTAG_CONFIG=~/.config/ragtag/.ragtag.yaml
ragtag summary
```

The `--config` CLI flag takes precedence over `RAGTAG_CONFIG`. If neither is set, walk-up discovery is used.

## Complete Schema

Below is a fully-specified config file showing all options and their default values:

```yaml
# Regex patterns for file paths to ignore.
# Matched against relative file paths during directory scanning.
# Maximum 256 patterns, each up to 1024 characters.
ignore_patterns: []

# Whether to respect .gitignore files when scanning directories.
respect_gitignore: true

# Whether to skip hidden files and directories (those starting with '.').
skip_hidden: true

# Maximum directory depth for recursive scanning.
# null (or omitted) means unlimited depth.
max_depth: null

# Maximum file size in bytes to scan. Files larger than this are skipped.
# Default: 10485760 (10 MB).
max_file_size: 10485760

# Output settings.
output:
  # Color mode: "auto", "always", or "never".
  # "auto" enables color when stdout is a terminal.
  color: "auto"

# User-defined command aliases (empty by default — there are no built-in aliases).
# Each entry has a `name` (invoked as `ragtag <name>`) and an `arguments` string
# that is split with shell-like quoting and executed as if typed directly.
aliases: []

# File creation settings.
files:
  # Directory used by `ragtag file touch` when --path is omitted.
  default_directory: "."

  # UTC chrono strftime pattern used for the generated filename.
  filename_format: "%Y-%m-%d_%H-%M-%S.md"

# Task extension configuration.
tasks:
  # The tag name used for tasks.
  # Change this if you prefer @todo or another name.
  tag_name: "task"

  # Default owner for new tasks.
  default_owner: "me"

  # Default time units for new tasks.
  # Allowed values: "hours", "days", "weeks".
  default_worktime_units: "hours"

  # Default status for new tasks.
  # Must be a recognized status keyword.
  default_status: "new"

  # Status categories to exclude from `task list` and `task summary` by default.
  # Use --all to include these categories.
  exclude_status_categories:
    - "done"
    - "abandoned"

  # Status keyword groups.
  # Each group maps to a color in terminal output.
  status_keywords:
    done:
      - "done"
      - "finished"
      - "complete"
      - "completed"
    active:
      - "active"
      - "underway"
      - "working"
      - "wip"
    blocked:
      - "blocked"
    abandoned:
      - "abandoned"
      - "deleted"
      - "removed"
      - "dead"
    inactive:
      - "inactive"
      - "pending"
      - "new"
```

## Options Reference

### Core Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `ignore_patterns` | list of strings | `[]` | Regex patterns matched against file paths to exclude |
| `respect_gitignore` | boolean | `true` | Honor `.gitignore` files during scanning |
| `skip_hidden` | boolean | `true` | Skip hidden files/directories (names starting with `.`) |
| `max_depth` | integer or null | `null` | Maximum recursion depth (`null` = unlimited) |
| `max_file_size` | integer | `10485760` | Maximum file size in bytes to scan |

### File Creation Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `files.default_directory` | path | `"."` | Directory for generated file targets |
| `files.filename_format` | string | `"%Y-%m-%d_%H-%M-%S.md"` | UTC chrono strftime pattern for generated filenames |

### Output Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `output.color` | string | `"auto"` | Color mode: `"auto"`, `"always"`, or `"never"` |

### Aliases

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `aliases` | list of objects | `[]` | User-defined command aliases (see [Aliases](#aliases-1)) |
| `aliases[].name` | string | (required) | The alias name, invoked as `ragtag <name>` |
| `aliases[].arguments` | string | (required) | Command string the alias expands to (split with shell-like quoting) |

### Task Extension Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `tasks.tag_name` | string | `"task"` | Tag name used for tasks |
| `tasks.default_owner` | string | `"me"` | Default owner for new tasks |
| `tasks.default_worktime_units` | string | `"hours"` | Default time units (`hours`, `days`, or `weeks`) |
| `tasks.default_status` | string | `"new"` | Default status for new tasks |
| `tasks.exclude_status_categories` | list of strings | `["done", "abandoned"]` | Status categories to exclude from `task list` and `task summary` output by default |
| `tasks.status_keywords` | object | (see above) | Status keyword groups by category |

## Ignore Patterns

Ignore patterns are regular expressions matched against file paths discovered during scanning. They use Rust's `regex` crate syntax.

**Limits:**

* Maximum **256** patterns
* Each pattern may be at most **1024** characters

**Examples:**

```yaml
ignore_patterns:
  - ".*\\.pdf$"       # Skip PDF files
  - "target/"          # Skip Rust build directory
  - "node_modules/"    # Skip Node.js dependencies
  - ".*\\.min\\.js$"   # Skip minified JavaScript
```

## Aliases

Aliases let you define shorthand commands in your config file. Running
`ragtag <alias>` expands the alias's `arguments` and executes the result as if
you had typed the full command. There are **no default aliases** — the list is
empty unless you define one.

```yaml
aliases:
  # Example: run `ragtag active` to list active tasks.
  - name: "active"
    arguments: "query task --filter status=active"
  # Example: run `ragtag t` to get a display showing only the active or priority-0 tasks.
  - name: "t"
    arguments: "task summary --filter \"(status = active OR priority = 0) AND (status != done AND status != abandoned)\""
  # Example: `run ragtag tt` as a shorthand version of `ragtag task summary`
  - name: "tt"
    arguments: "task summary"
```

With the config above, `ragtag tt` behaves exactly like
`ragtag task summary`.

**Behavior:**

* **Shell-like argument splitting.** The `arguments` string is split using
  shell-word semantics (via the `shlex` crate), so quoting is respected:
  `arguments: 'task get "two words"'` yields `task`, `get`, `two words`.
* **Trailing arguments are appended.** Anything you type after the alias name is
  appended to the expansion. `ragtag my-alias --count` runs
  `ragtag task summary --count`.
* **Prefix inference includes aliases.** ragtag infers unambiguous subcommand
  prefixes, and aliases participate too: `ragtag my` resolves to `my-alias`. An
  ambiguous prefix that matches multiple commands and/or aliases is an error,
  just as with built-in commands.
* **No recursion.** An alias always expands to built-in or extension commands
  only — an alias never expands into another alias.

**Validation (checked at startup):**

* An alias `name` must not be empty.
* An alias `name` must not collide with a real command name — a built-in
  (`summary`, `query`, `config`, `file`) or an extension command (`task`). Collisions
  are a config error.
* Alias names must be unique.

## File Creation

The `files` section controls targets generated by `ragtag file touch` when
`--path` is omitted:

```yaml
files:
  default_directory: "notes"
  filename_format: "%Y-%m-%d_%H-%M-%S-%3f.md"
```

`files.default_directory` must not be empty. An absolute value is used
directly. A relative value, including `.` or one containing parent components,
is resolved from the **ragtag root**. The ragtag root is the directory
containing the explicitly selected or auto-discovered config file; when no
config exists, it is the working directory in which ragtag started.

`files.filename_format` uses
[chrono strftime syntax](https://docs.rs/chrono/latest/chrono/format/strftime/index.html)
and is evaluated in UTC. Common directives include `%Y` (year), `%m` (month),
`%d` (day), `%H` (hour), `%M` (minute), and `%S` (second). Fractional seconds
such as `%3f` can reduce collisions between rapid creations. The rendered value
must be exactly one normal filename component: it cannot be empty, `.`, `..`,
absolute, or contain a platform path separator. Put directories only in
`files.default_directory`.

The default format has one-second resolution. If the generated target already
exists, creation fails without overwriting, suffixing, incrementing, or
retrying. The same exclusive behavior applies to explicit targets.

Configuration and CLI paths are literal operating-system paths. Ragtag does
not expand `~` or environment variables. Explicit relative `--path` values are
different from `files.default_directory`: they are resolved from the startup
working directory, not the ragtag root. Explicit absolute and parent paths are
supported, and missing parent directories are created recursively.

Both resolved values can be inspected:

```bash
ragtag config get files.default_directory
ragtag config get files.filename_format
```

For command behavior, tags, and optional editor integration, see
[`file touch`](cli-reference.md#file-touch).

## Example Configs

### Minimal Config

```yaml
# .ragtag.yaml — just ignore some directories
ignore_patterns:
  - "target/"
  - "\\.git/"
```

### Task-Focused Config

```yaml
# .ragtag.yaml — customized for task tracking
tasks:
  tag_name: "todo"
  default_owner: "alice"
  default_worktime_units: "days"
  default_status: "pending"
  status_keywords:
    done: ["done", "shipped"]
    active: ["active", "wip", "in-progress"]
    blocked: ["blocked", "waiting"]
    abandoned: ["abandoned", "cancelled"]
    inactive: ["pending", "new", "backlog"]

output:
  color: "always"
```

### Large Codebase Config

```yaml
# .ragtag.yaml — tuned for scanning a large project
max_depth: 10
max_file_size: 5242880    # 5 MB
skip_hidden: true
respect_gitignore: true
ignore_patterns:
  - "vendor/"
  - "dist/"
  - "build/"
  - ".*\\.lock$"
```

### Config With Aliases

```yaml
# .ragtag.yaml — handy shorthands
aliases:
  - name: "todo"
    arguments: "query task --filter status=active"
  - name: "ts"
    arguments: "task summary"
```

Now `ragtag todo` runs `ragtag query task --filter status=active`, and
`ragtag ts --path src` runs `ragtag task summary --path src`.
