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

### Trust Boundary and Safeguards

Configuration is trusted input. When ragtag runs inside a repository, it may
automatically load that repository's config before parsing the requested
command. Review config files in untrusted repositories before running ragtag.
In particular:

* Aliases can select any ragtag command and flags. They cannot invoke a shell
  or replace a real command name, but an unfamiliar alias can run file-writing
  operations such as `file touch`, choose absolute output paths, request
  `--edit` using your `EDITOR`, or alter query behavior.
* Discovery settings such as `ignore_patterns` can omit files from query and
  summary output.
* For scripts and CI, place `--config /trusted/path.yaml` before the command
  name, or set `RAGTAG_CONFIG` to a reviewed file. Avoid relying on
  auto-discovery in repositories you do not trust.
* Inspect unfamiliar alias names before invoking them, especially names that
  resemble project-specific commands.

Config files must be regular files and are limited to 1 MiB. Larger files and
special or unbounded sources such as `/dev/zero` are rejected before parsing.

### Override With `--config`

You can skip auto-discovery and specify an explicit config file path:

```bash
ragtag --config /path/to/.ragtag.yaml summary
```

The flag that selects startup configuration must appear before the command
name. A different `--config` parsed after the command is rejected because
aliases and extensions have already been initialized. If the specified file
does not exist, ragtag exits with an error.

The raw startup scan recognizes only these leading root options:

* `--config PATH`
* `--config=PATH`
* `--no-color`, which is skipped while scanning continues

Scanning stops at the outer command token, a literal `--`, or any other option,
including `-h`, `--help`, and `--version`.

Within that prefix, the last valid split `--config PATH` or
`--config=PATH` selector wins. For split syntax, a literal `--` in the value
position terminates the scan and is not used as a path. If an earlier valid
selector was already seen, `--config --` leaves that earlier selection intact.

The single terminal clap parse may later encounter another global config
spelling. A post-command selector that resolves to the same file already
loaded is accepted as redundant and does not reload it. A different original
selector is rejected. A selector introduced by an alias is terminal syntax
only: clap may accept it, but it never changes or reloads startup
configuration.

### Override With `RAGTAG_CONFIG`

Alternatively, set the `RAGTAG_CONFIG` environment variable to specify a config file path without passing `--config` every time:

```bash
export RAGTAG_CONFIG=~/.config/ragtag/.ragtag.yaml
ragtag summary
```

The `--config` CLI flag takes precedence over `RAGTAG_CONFIG`. If neither is set, walk-up discovery is used.

## Environment Interpolation

After parsing the YAML document, ragtag interpolates every YAML string value
before typed configuration is deserialized or consumed. Mapping keys, YAML
tags, scalar types, sequences, mappings, and document structure are never
changed. This applies uniformly to core settings, paths, status values, colors,
lists, alias metadata, and known or unknown extension configuration.

Two reference forms are supported, using names that match
`[A-Za-z_][A-Za-z0-9_]*`:

```yaml
files:
  default_directory: "${RAGTAG_NOTES_DIR}"
tasks:
  default_owner: "$USER"
ignore_patterns:
  - "^${BUILD_DIRECTORY}/"
```

The interpolation rules are:

* `$NAME` and `${NAME}` read `NAME` from the process environment.
* An undefined variable and a defined-but-empty variable both produce an empty
  string.
* Unbraced names consume the longest valid name. For example, `$HOME_DIR`
  reads `HOME_DIR`, not `HOME` followed by `_DIR`.
* `$$` produces one literal `$` and prevents the second dollar from beginning
  a reference. For example, `$$HOME` becomes the literal `$HOME`.
* Expansion is one pass. If `FIRST` contains `$SECOND`, expanding `$FIRST`
  produces the literal text `$SECOND`; it is not expanded again.
* Invalid or incomplete forms are preserved as one literal unit, including
  `$`, `${`, `${}`, `${9NAME}`, `${BAD-NAME}`, `${MISSING`, `$9`, and `$-`.
  Dollars inside a malformed braced form are never scanned separately. For
  example, `${BAD-$GOOD}` remains unchanged even when `GOOD` is defined.
* Environment text is inserted only into an existing YAML string value. A
  value such as `[one, two]` remains a string and cannot inject a sequence or
  alter document structure.

Alias `arguments` are the sole deferred value. Ragtag shell-splits their trusted
template during configuration loading without expanding references. Each
stored token is interpolated independently from the current process environment
when the alias is invoked. See [Aliases](#aliases) for token behavior.

To avoid disclosing environment-derived values, typed configuration and
runtime errors do not echo resolved content. `ragtag config get` prints
`<environment-derived>` for any string field produced by interpolation,
including undefined or defined-empty references. Alias inspection prints the
canonical unexpanded argument template.

Ragtag does not interpolate command-line arguments. The invoking shell is
responsible for expansion there. `RAGTAG_CONFIG` and `RAGTAG_PATH` retain their
existing roles as direct environment fallbacks; their values are not passed
through this config interpolation engine.

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
# Each entry has exactly one naming form: `name` (one string) or `names`
# (a nonempty ordered list of peer names), plus one shell-like `arguments`
# string.
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
| `aliases[].name` | string | conditional | Single alias name; required when `names` is absent and mutually exclusive with it |
| `aliases[].names` | nonempty list of strings | conditional | Ordered peer names; required when `name` is absent and mutually exclusive with it |
| `aliases[].arguments` | string | (required) | Shell-split template whose stored tokens are interpolated independently at invocation |

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
  # Both names invoke the same definition.
  - names: ["active", "a"]
    arguments: "query task --filter status=active"
  # This alias composes through the exact `active` name.
  - name: "active-count"
    arguments: "active --count"
  # Run `ragtag focus` to show only active or priority-0 tasks.
  - name: "focus"
    arguments: "task summary --filter \"(status = active OR priority = 0) AND (status != done AND status != abandoned)\""
```

Each definition must specify exactly one of `name` or `names`. `name` is a
string. `names` is a nonempty ordered sequence of strings whose entries are
peer invocation names. A one-element `names` sequence is accepted and is
serialized canonically as `name`; two or more names serialize as `names`.
Comma-delimited strings and token-array forms for `arguments` are not accepted.

`ragtag config get aliases` uses ragtag's human-readable flow-style rendering,
not YAML. Its field selection is canonical: a single-name definition uses
`name`, multiple names retain their configured order under `names`, and
`arguments` remains one canonical shell-quoted string rather than a token
sequence. Environment references remain unexpanded in this output.

The exported Rust `Alias.arguments` field is `Vec<String>`. YAML
deserialization performs the shell-like split, and serialization joins those
tokens into the canonical string form.

**Behavior:**

* **Shell-like argument splitting before interpolation.** The trusted
  `arguments` template is split using shell-word semantics (via the `shlex`
  crate) while config loads. Quotes and whitespace in the template define
  token boundaries. With `TASK_OWNER='Alice Smith'`,
  `arguments: "query task --filter owner=$TASK_OWNER"` produces one filter
  token, `owner=Alice Smith`. Quotes, backslashes, whitespace, or option-like
  text supplied by the environment remain opaque data in that same token and
  cannot create arguments or quoting structure.
* **Invocation-time environment.** Alias argument templates are not expanded
  while loading config. Each definition is expanded once when reached during
  an invocation, including recursively composed aliases. Every stored token is
  expanded independently and exactly once when its definition is reached.
  Environment changes after config loading therefore affect the next
  invocation. Dollar escaping, undefined variables, invalid forms, and
  single-pass behavior match
  [Environment Interpolation](#environment-interpolation).
* **Trailing arguments are appended.** Anything you type after the alias name is
  appended to the expansion. With the example above, `ragtag active --count`
  adds `--count` to the expanded `query task` command.
* **Prefix inference includes aliases.** ragtag infers unambiguous subcommand
  prefixes, and aliases participate too: `ragtag foc` resolves to `focus`. An
  ambiguous prefix that matches multiple commands and/or aliases is an error,
  just as with built-in commands. Prefix matches through peer names of one
  definition count as one candidate and use the first configured name as the
  canonical spelling.
* **Exact aliases precede real-prefix inference.** An exact alias name wins even
  when that spelling is also a prefix of a real command. For example, defining
  an alias named `t` would make `ragtag t` invoke that alias rather than infer
  `task`.
* **Exact composition.** If the first expansion token exactly matches any name
  of another alias definition, that token is recursively replaced. Prefixes do
  not compose. For `active-count` above, the final order is the inner
  `active` arguments, `--count`, and then any original suffix.
* **Terminal clap behavior.** Built-in and extension prefixes, command options,
  help, and version are interpreted only after expansion by the real command
  tree. Aliases are not listed in top-level help.
* **Separator boundary.** A literal `--` before a prospective alias prevents
  alias recognition. A separator after a resolved alias remains in the suffix,
  where the expanded terminal command interprets it.
* **One config load.** The last leading `--config PATH` or `--config=PATH`
  before the outer command selects startup configuration. Configuration is not
  reloaded after expansion. A different original `--config` after the command
  is rejected, while an alias-defined `--config` token remains terminal syntax
  only and cannot switch the loaded config.
* **OS-native argv boundary.** Original tokens are preserved without Unicode
  conversion through scanning, expansion, and assembly. Individual terminal
  clap arguments may still require Unicode; OS-path parsers such as
  `--config` retain native path values.

**Validation (checked at startup):**

* At most 256 definitions and 256 names in aggregate are allowed.
* Every definition must have one naming form, at least one nonempty name, and
  a shell-parseable `arguments` template that produces at least one token.
  Environment-derived values are never reparsed as shell-like text.
* Every name must be unique across all definitions and must not collide with a
  built-in (including `help`) or extension command.

Composition is limited to 32 active definitions and 4096 expanded arguments.
Definition identity, not the selected synonym, is used for cycle detection.
Ambiguous outer prefixes, cycles, exceeded limits, and definite unknown
terminal targets report deterministic errors without executing external
programs. These alias-engine and configuration errors exit with status `1`.
When no environment reference participates, terminal clap grammar errors exit
with status `2`. Failures involving an environment-derived alias token use a
generic status-`1` error so the resolved value cannot appear in diagnostics.

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

Ragtag does not expand `~`. Configuration path values such as
`files.default_directory` support the generic environment interpolation
described above. Command-line paths are left untouched by ragtag; the invoking
shell is responsible for any command-line expansion. Explicit relative
`--path` values are different from `files.default_directory`: they are resolved
from the startup working directory, not the ragtag root. Explicit absolute and
parent paths are supported, and missing parent directories are created
recursively.

Both configured values can be inspected. Environment-derived values display
the redaction marker described above:

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
    arguments: 'query task --filter "owner=$TASK_OWNER"'
  - name: "ts"
    arguments: "task summary"
```

With `TASK_OWNER='Alice Smith'`, `ragtag todo` runs
`ragtag query task --filter 'owner=Alice Smith'`, and
`ragtag ts --path src` runs `ragtag task summary --path src`.
