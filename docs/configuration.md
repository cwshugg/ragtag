# Configuration Reference

ragtag is configured via a YAML file. All settings have sensible defaults, so a config file is entirely optional.

## Config File Discovery

ragtag searches for a config file by walking up the directory tree from the current working directory. At each level it checks for:

1. `.ragtag.yaml` (preferred)
2. `ragtag.yaml`

The search **stops** when it reaches a directory containing a `.git` folder or the filesystem root. If no config file is found, built-in defaults are used.

### Override With `--config`

You can skip auto-discovery and specify an explicit config file path:

```bash
ragtag --config /path/to/.ragtag.yaml summary
```

If the specified file does not exist, ragtag exits with an error.

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

# Whether to skip binary files.
skip_binary: true

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

# Task extension configuration.
tasks:
  # The tag name used for tasks.
  # Change this if you prefer @todo or another name.
  tag_name: "task"

  # Default owner for new tasks.
  default_owner: "me"

  # Default time units for new tasks.
  # Allowed values: "hours", "days", "weeks".
  default_time_units: "hours"

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
| `skip_binary` | boolean | `true` | Skip binary files |
| `max_depth` | integer or null | `null` | Maximum recursion depth (`null` = unlimited) |
| `max_file_size` | integer | `10485760` | Maximum file size in bytes to scan |

### Output Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `output.color` | string | `"auto"` | Color mode: `"auto"`, `"always"`, or `"never"` |

### Task Extension Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `tasks.tag_name` | string | `"task"` | Tag name used for tasks |
| `tasks.default_owner` | string | `"me"` | Default owner for new tasks |
| `tasks.default_time_units` | string | `"hours"` | Default time units (`hours`, `days`, or `weeks`) |
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
  default_time_units: "days"
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
skip_binary: true
respect_gitignore: true
ignore_patterns:
  - "vendor/"
  - "dist/"
  - "build/"
  - ".*\\.lock$"
```
