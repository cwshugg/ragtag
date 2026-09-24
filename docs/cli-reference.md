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

`--no-color`, help, and version follow clap's global placement rules.
`--config` must precede the outer command when it selects startup
configuration, because configuration and aliases are loaded before the single
terminal clap parse. The raw scan recognizes only leading `--config PATH`,
`--config=PATH`, and `--no-color`; it skips `--no-color` and continues
scanning. It stops at the command, a literal `--`, or any other option,
including `-h`, `--help`, and `--version`. In split form, `--config --` does
not select `--` as a path and does not replace an earlier valid leading
selector. A post-command selector for the same loaded file is accepted as
redundant; a different original selector is rejected. Alias-introduced
selectors remain terminal syntax and never reload configuration.

## Subcommand Prefix Matching

ragtag enables unambiguous prefix matching for *every* subcommand and
sub-subcommand via clap's `infer_subcommands`. Subject to exact top-level alias
precedence described below, you may type a leading prefix of a subcommand name
when it resolves to exactly one command at that level:

```bash
ragtag su                  # → ragtag summary
ragtag q task              # → ragtag query task
ragtag t li             # → ragtag task list
ragtag t cr --title "X" # → ragtag task create
ragtag task sum            # → ragtag task summary
ragtag task comp <ID>      # → ragtag task complete
ragtag task pr 0 <ID>      # → ragtag task prioritize
ragtag task ab <ID>        # → ragtag task abandon
```

Ambiguous prefixes (e.g., `ragtag task c`, which could be `complete` or
`create`) are rejected with a list of candidate subcommands. Add one more
character to disambiguate.

At the top level, an exact alias name is resolved before real-command prefix
inference. For example, an alias named `t` handles `ragtag t` instead of
inferring the real `task` command. Use `ragtag task ...` when a configured
alias may claim a shorter spelling.

## Aliases

You can define command aliases in your config file under the `aliases` key. Running `ragtag <alias>` expands the alias's `arguments` and executes the result as if you had typed the full command.

```yaml
# .ragtag.yaml
aliases:
  - names: ["tsum", "ts"]
    arguments: "task summary"
  - name: "all-tasks"
    arguments: "tsum --all"
```

```bash
ragtag tsum                # → ragtag task summary
ragtag ts                  # → ragtag task summary
ragtag tsum --path src     # → ragtag task summary --path src   (trailing args appended)
ragtag tsu                 # → ragtag task summary  (prefix inference; when unambiguous)
ragtag all-tasks           # → ragtag task summary --all
```

Notes:

* **One or multiple names.** Each definition specifies exactly one of `name`
  or a nonempty ordered `names` list. All names invoke the same definition.
* **Split first, then interpolate each token.** The trusted `arguments`
  template is shell-split during config loading. `$NAME` and `${NAME}` inside
  each stored token use the current environment when invoked. With
  `OWNER='Alice Smith'`, `arguments: "query task --filter owner=$OWNER"`
  keeps `owner=Alice Smith` in one token. Environment-provided spaces, quotes,
  backslashes, and option-like text never create new arguments or syntax.
  `$$` emits a literal dollar, undefined names become empty, and malformed
  braced forms remain one literal unit.
* **Trailing args are appended** after the alias's own arguments.
* **Prefix inference includes aliases** — an ambiguous prefix across commands and aliases errors just like any other ambiguous prefix.
  Prefix matches through multiple peer names of one definition count as one
  candidate, not an ambiguity. A uniquely inferred synonym uses the
  definition's first configured name as its canonical spelling.
  An exact alias name wins before real-command prefix inference, so an alias
  named with a real command's prefix shadows that abbreviated spelling.
* **Composition uses exact names.** If token zero of an expansion exactly names
  another alias, that definition expands too. Recursive prefixes do not
  compose. Inner arguments come first, followed by each outer remainder and
  then the original invocation suffix. Each composed definition evaluates its
  own template at the actual invocation step.
* **Aliases are config-only.** They do not appear in top-level help. Every name
  is checked at startup against built-ins (including `help`), extension
  commands, duplicates, and empty names.
* **Boundaries are preserved.** A `--` before the outer command prevents alias
  recognition. A `--` after an alias is retained for the terminal command.
  Help and version tokens after an alias are handled by the expanded command.
* **Configuration loads once.** Only leading `--config` tokens before the outer
  command select startup configuration. A different original `--config` after
  the command is rejected. An alias-defined `--config` is still validated by
  clap but does not reload configuration.
* **Expansion is bounded.** Configuration allows at most 256 alias definitions
  and 256 aggregate names. Composition allows at most 32 definitions and 4096
  expanded arguments. Cycles, unknown terminal targets, ambiguous outer
  prefixes, and exceeded limits produce explicit errors.
* **Exit status distinguishes error ownership.** Alias-engine errors such as
  ambiguity, cycles, unknown targets, and exceeded limits exit with status `1`.
  Without interpolation, syntax rejected by the expanded terminal command is
  reported by clap and exits with status `2`. Errors involving
  environment-derived alias data use a generic status-`1` diagnostic that
  never echoes the resolved value.
* **OS-native argv is preserved through alias processing.** Original tokens
  retain their platform-native values through scanning, composition, and
  terminal argv assembly. Configured alias tokens are YAML strings. The final
  clap value parser may still require Unicode for a particular argument;
  OS-path arguments such as `--config` retain native path values.

See [Configuration Reference → Aliases](configuration.md#aliases) for full details.

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
ragtag query [TAG_NAME] [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `TAG_NAME` | No | Tag name to search for (without `@`); omit to query every tag |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--filter <EXPR>` | — | Boolean filter expression (repeatable, AND-combined). See [Filter Expressions](#filter-expressions) |
| `--count` | — | Print only the count of matching tags |
| `--limit <INTEGER>` | — | Return at most this many final results; `0` returns none |
| `--randomize [SEED]` | — | Randomize before `--limit`; an optional `u64` seed makes ordering reproducible |

**Output (default):**

Registered extensions format their own results even when `TAG_NAME` is
omitted. Task results therefore use normalized human-readable task output in
both scoped and unscoped queries. Tags without an extension retain grep-style
source formatting:

```
notes/ideas.md:15: @todo(priority=1, owner="alice")
notes/bugs.md:42: @todo(priority=0, owner="bob")
```

`query` does not have a `--raw` mode. For safely framed, normalized
machine-oriented task records, use `ragtag task list --format jsonl`; its
`type` value is always canonical and its source span identifies the exact
occurrence. Legacy `--format raw` remains available for compatibility.

**Output with `--count`:**

```
2
```

Filters use the shared boolean [Filter Expressions](#filter-expressions) syntax,
so `--filter '(priority = 0 OR owner = alice) AND status != done'` works, with
optional whitespace around operators.

Result ordering options apply after the complete matching collection has been
collected and filtered but before every output mode, including
extension-specific formatting and `--count`. In other words, `--limit` limits
output after collection; it does not stop file discovery or parsing early.
Bare `--randomize` obtains a fresh nondeterministic seed from the operating
system. `--randomize SEED` and `--randomize=SEED` accept values from `0` through
`18446744073709551615`. The same seed and identical final pre-shuffle result
list produce the same order. The implementation pins `fastrand 2.4.1` and its
WyRand shuffle sequence; seeded output is stable across Ragtag `1.x`, while a
future major version may deliberately change the algorithm.

The space-separated seed is optional, so a positional query immediately after
`--randomize` is ambiguous and is parsed as a seed. Put the query first:

```bash
ragtag query idea --randomize
ragtag query idea --randomize 42 --limit 5
```

Alternatively, use `--` before a query that follows an unseeded flag:

```bash
ragtag query --randomize -- idea
```

`--randomize --limit 5` shuffles the complete matching collection and then
keeps up to five entries. Flag order does not change processing order.
`--limit` accepts non-negative integers only.

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

Prints the inspected value to stdout. Strings are printed without quotes,
numbers and booleans as-is, sequences in bracket notation, and mappings in
brace notation.

String fields produced by generic environment interpolation are displayed as
`<environment-derived>` rather than exposing their resolved contents. This
also applies to undefined and defined-empty references. Alias `arguments`
remain their canonical unexpanded templates.

**Examples:**

```bash
ragtag config get max_depth             # null (when unset)
ragtag config get max_file_size         # 10485760
ragtag config get respect_gitignore     # true
ragtag config get output.color          # auto
ragtag config get tasks.tag_name        # task
ragtag config get tasks.default_owner   # me
ragtag config get ignore_patterns       # ["*.git", "node_modules"]
ragtag config get aliases               # [{names: ["active", "a"], arguments: query task --filter 'status=active'}, {name: active-count, arguments: active --count}]
ragtag config get tasks.status_keywords.done  # ["done", "finished", "complete", "completed"]
ragtag config get nonexistent_field     # error: unknown config key "nonexistent_field"
```

Extension configs (like `tasks`) are resolved with defaults applied, so all fields are available even if not explicitly set in the YAML file.

`config get aliases` uses ragtag's human-readable flow-style rendering, not
YAML. Its field selection is canonical: a definition with one name is printed
with `name`, while a definition with multiple peer names is printed with
ordered `names`. Each `arguments` value remains one shell-quoted string; it is
never emitted as a token array.

### `file touch`

Create exactly one new plain text file.

```text
ragtag file touch [--path <FILE>] [--tag <TAG>]... [--edit]
```

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <FILE>` | UTC-generated target | Target file |
| `--tag <TAG>` | — | Complete tag to place at the beginning of the file; repeat once per tag |
| `--edit` | disabled | Open the newly created file using `EDITOR` |

On success, ragtag prints the new file's full absolute resolved path to stdout
with one trailing newline. The display path is normalized without filesystem
access, removing redundant `.` components without requiring the target to
still exist after an editor runs. It is not canonicalized; parent components
may remain where lexical removal could change symlink-sensitive semantics.

The operation is exclusive creation, not POSIX `touch`: it fails if the target
already exists. Existing regular files, directories, symlinks, and dangling
symlinks are all rejected and left unchanged. Concurrent attempts for the same
target have only one winner. Ragtag never overwrites, appends a suffix, retries,
or treats an existing target as success.

**Target resolution:**

* An explicit absolute `--path` is used directly.
* An explicit relative `--path`, including a path such as `../note.md`, is
    resolved from the working directory in which ragtag started.
* Missing parent directories are created recursively after all tags and any
    requested editor configuration have been validated.
* Paths are lexical and are not restricted to the ragtag root. Tildes are not
    expanded. Environment references in an explicit command-line `--path` are
    not expanded by ragtag.
* Without `--path`, `files.filename_format` is evaluated using the current UTC
    time and appended to `files.default_directory`. A relative configured
    directory is based on the ragtag root: the selected config file's directory,
    or the startup working directory if no config exists. An absolute configured
    directory is used directly.

The default format, `%Y-%m-%d_%H-%M-%S.md`, produces names such as
`2026-08-21_12-33-52.md`. Because it has one-second resolution, two creations
in the same second can collide. A collision is an error; configure fractional
seconds such as `%3f` to reduce that risk. See
[File Creation Configuration](configuration.md#file-creation).

**Tag header:**

Each `--tag` occurrence must contain one complete tag accepted by ragtag's
normal tag parser. Surrounding whitespace is trimmed, and `@` is prepended when
omitted. Trailing prose, multiple tags in one option, and malformed syntax are
rejected before any directories or files are created.

Exact normalized duplicates are removed in first-seen order: `todo` and
`@todo` are duplicates, while `@todo(owner=a)` and `@todo(owner=b)` are not.
The normalized source spelling, including valid quoting, spacing, and attribute
order, is preserved. Tags start at byte zero, one per line, with one line-feed
after every tag and no extra blank line:

```text
@project
@task(status=active)
```

With no tags, ragtag creates a zero-byte file.

**Editor behavior:**

`EDITOR` is consulted only when `--edit` is present. Its value must be set,
nonblank, and valid shell-word syntax. Ragtag parses the executable and initial
arguments with shell-like quoting, but invokes the executable directly without
a shell. The created target is appended as the final argument. The editor
inherits stdin, stdout, and stderr, and ragtag waits for it to finish.

Invalid `EDITOR` configuration fails before parent or file creation. A launch
failure, signal termination, or nonzero editor exit makes the command fail
after creation; the created file is deliberately retained because the editor
may already have changed it. These failures do not print the created path. A
zero editor exit is successful, and only then does ragtag print the path.
Without `--edit`, ragtag does not read or validate `EDITOR` and prints the path
immediately after writing the file.

**Examples:**

```bash
ragtag file touch
ragtag file touch --path notes/today.md
ragtag file touch --path ../shared/idea.md --tag idea --tag '@project(name=ragtag)'
ragtag file touch --path /absolute/path/note.md --edit
```

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
| `--title <STR>` | Task title; omit to trigger interactive mode |
| `--description <STR>` | Task description |
| `--owner <STR>` | Task owner |
| `--status <STR>` | Task status |
| `--type <STR>` | Task type (`item`, `project`, or a custom non-empty string) |
| `--priority <NUM>` | Priority (`0` = highest) |
| `--worktime-estimate <NUM>` | Time-to-complete estimate |
| `--worktime-spent <NUM>` | Worktime already spent (default: `0`) |
| `--worktime-units <STR>` | Time units: `hours`, `days`, or `weeks` |
| `--pid <STR>` | Parent task ID |
| `--format <FORMAT>` | Output format: `multiline` (default) or `oneline` |

**Output:**

Prints an `@task(...)` string to stdout. With `--format multiline` (default):

```
@task(
    id="a1b2c3d4e5f67890",
    title="Write documentation",
    owner="me",
    status="new",
    type="item",
    worktime_spent=0,
    worktime_estimate=4,
    time_created="2026-06-12T16:00:00Z",
    time_last_updated="2026-06-12T16:00:00Z",
    worktime_units="hours"
)
```

With `--format oneline`:

```
@task(id="a1b2c3d4e5f67890", title="Write documentation", owner="me", status="new", type="item", worktime_spent=0, worktime_estimate=4, time_created="2026-06-12T16:00:00Z", time_last_updated="2026-06-12T16:00:00Z", worktime_units="hours")
```

The task ID is a randomly-generated 16-character hex string. `type` defaults to `item` and is always emitted. Built-in `item` and `project` input is case-insensitive (with surrounding whitespace ignored) and emits canonical lowercase. Missing, empty, whitespace-only, malformed non-string values resolve silently to `item`. Every other non-empty string is a custom type and is preserved verbatim, including casing and surrounding whitespace; for example, `ProjectX` is custom. `worktime_spent` defaults to `0` and is always emitted. The `time_created` and `time_last_updated` fields are auto-populated with the current UTC timestamp (ISO 8601) at creation; they are never user-supplied and cannot be passed as flags.

Every non-tabular output that renders a complete task includes its effective type. Mutation commands using `--no-edit` also add or normalize the `type` attribute in the printed complete tag. Summary tables include Type only when the displayed rows in that individual table have different effective rendered type strings.

**Interactive mode:**

If `--title` is omitted (or supplied as an empty string `--title ""`), `task create` enters interactive mode. ragtag prompts for each field on stdin using a `rustyline`-backed line editor (arrow keys, line editing, and history work as expected). Each prompt:

* Shows the field name in a gray-blue color and the hint in dark gray
* Shows the effective default in brackets when one exists, e.g., `Owner (leave blank to skip; default: me):`
* Validates input on the spot — invalid entries print a red error and re-prompt the same field
* Leaves the field unset (or uses the config default) when blank

`time_created` and `time_last_updated` are **never** prompted — they are filled in automatically.

#### `task list`

List tasks found in files.

```
ragtag task list [OPTIONS]
```

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--filter <EXPR>` | — | Boolean filter expression, e.g. `"status=active AND priority<=2"` (repeatable, AND-combined). See [Filter Expressions](#filter-expressions) |
| `--sort <FIELD>` | — | Sort by field name |
| `--reverse` | — | Reverse sort order |
| `--all`, `-a` | — | Show all tasks, including projects and excluded status categories (done, abandoned) |
| `--format <FORMAT>` | `default` | Output format: `default`, legacy `raw`, or stable machine-readable `jsonl` |

Only the built-in `project` type is excluded by default; custom types, including `ProjectX`, remain visible. An exact parsed `type` field predicate disables that exclusion, while other fields or values that merely contain the text `type` do not. Type values use the normal case-sensitive filter comparison against canonical lowercase built-ins or the verbatim custom value. The existing status-filter override remains independent; `--all` disables both defaults.

**Output:**

One task per line, including its normalized type:

```text
notes/project.md: a1b2c3d4e5f67890 [project] [alice] [0/active] Design API
notes/bugs.md: f0e1d2c3b4a59687 [item] [bob] [1/blocked] Fix parser bug
```

For integrations, `--format jsonl` emits exactly one JSON object per physical
output line. Strings use standard JSON escaping, so embedded newlines and
record-like text cannot create extra records. The record structure is:

```json
{"id":"a1b2","pid":null,"title":"Design API","description":null,"owner":"alice","status":"active","type":"project","priority":0,"worktime_spent":null,"worktime_estimate":4.0,"time_created":null,"time_last_updated":null,"worktime_units":"hours","source":{"tag_name":"task","file":"notes/project.md","line":3,"column":5,"byte_start":42,"byte_end":180}}
```

Each listed task occurrence produces a separate record, even when IDs or
titles are duplicated. `type` contains the canonical built-in or verbatim custom value.
Optional scalar fields are JSON `null` when absent. `source.tag_name` is the
configured task tag name; `line` and `column` are 1-based; `byte_start` is the
0-based UTF-8 byte offset of `@`, and `byte_end` is the exclusive 0-based byte
offset immediately after that exact tag occurrence. `column` is a UTF-8 byte
column, matching the parser's location model. Offsets apply to the contents of
the scanned file named by `source.file`.

Consumers should validate all required fields and types and bind a record to
the unchanged scanned snapshot by `source.file`,
`source.tag_name`, and the exact `[byte_start, byte_end)` span. Unexpected
fields must be rejected. The legacy `--format raw` key/value blocks are
unchanged and are not safely framed when values contain newlines.

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
| `--group <FIELD>` | `priority` | Group tasks by field: `status`, `owner`, `priority`, or `type` |
| `--sort <FIELD>` | — | Sort tasks within each group by any task field name |
| `--filter <EXPR>` | — | Boolean filter expression, e.g. `"status=active AND priority<=2"` (repeatable, AND-combined). See [Filter Expressions](#filter-expressions) |
| `--format <FORMAT>` | `table` | Output format: `table` (aligned columns) or `list` (multi-line per task) |
| `--all`, `-a` | — | Show all tasks, including projects and excluded status categories (done, abandoned) |

**Output:**

Tasks are displayed in aligned tables, grouped by the specified field. Each group has a header (e.g., `Status: active`).

With `--format table` (default), columns are Path, Title, Owner, Status, Priority, Time, and ID. A Type column is inserted after Title only when that individual post-filter, post-grouping table contains different effective rendered type strings. Empty, single-row, and homogeneous tables omit it. Grouped tables share one width layout computed from every displayed row after selection, so common columns align across groups; groups that include Type share its global width. Widths, padding, and title truncation use terminal display cells. Truncation preserves complete Unicode grapheme clusters and counts the ellipsis within the configured cell limit.

With `--format list`, each task is shown as three lines: file path, truncated title, and a detail line with ID, type, owner, priority, status, and time. Tasks are separated by blank lines.

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
| `ATTR` | Yes | Attribute name: `title`, `description`, `owner`, `status`, `type`, `priority`, `worktime_spent`, `worktime_estimate`, `time_created`, `time_last_updated`, `worktime_units`, `pid`, `id` |

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

# Change task type (empty values normalize to item; other values may be custom)
ragtag task set-attr a1b2c3d4e5f67890 type project

# Update time spent
ragtag task set-attr a1b2c3d4e5f67890 worktime_spent 6.5

# Update parent ID
ragtag task set-attr a1b2c3d4e5f67890 pid f0e1d2c3b4a59687

# Get updated tag string without modifying the file
ragtag task set-attr a1b2c3d4e5f67890 status done --no-edit
```

For relative additions and subtractions, use [`task time`](#task-time).

#### `task time`

Set or adjust a task's `worktime_spent`.

```bash
ragtag task time <N|+N|-N> <ID> [OPTIONS]
```

The first argument controls how time is updated:

* `N` sets an absolute value.
* `+N` adds to the current value.
* `-N` subtracts from the current value and clamps the result to `0`.

`N` must be a finite, non-negative number. If `worktime_spent` is absent, its
current value is treated as `0`. A successful update also sets
`time_last_updated` to the current UTC timestamp.

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `N`, `+N`, or `-N` | Yes | Absolute value or relative adjustment |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Do not modify the file; print the updated `@task(...)` string instead |

Without `--no-edit`, ragtag atomically updates the source file and prints
exactly:

```text
Updated task <ID> (worktime_spent → <VALUE>)
```

With `--no-edit`, the file is unchanged and the complete updated tag,
including `time_last_updated`, is printed.

**Examples:**

```bash
ragtag task time 4 a1b2c3d4e5f67890
ragtag task time +1.5 a1b2c3d4e5f67890 --path ./notes
ragtag task time -2 a1b2c3d4e5f67890 --no-edit
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

#### `task prioritize`

Set a task's priority to a specific non-negative integer value in one command.

```bash
ragtag task prioritize <PRIORITY> <ID> [OPTIONS]
```

**Arguments:**

| Argument | Required | Description |
| --- | --- | --- |
| `PRIORITY` | Yes | New priority value — non-negative integer; `0` is highest/most urgent |
| `ID` | Yes | Task ID or ID prefix |

**Options:**

| Option | Default | Description |
| --- | --- | --- |
| `--path <PATH>` | `.` | Search path (file or directory) |
| `--no-edit` | — | Don't modify the file; print the updated `@task(...)` string to stdout instead |

**Behavior:**

* Finds the task by ID across all scanned files
* Validates the priority argument is a non-negative integer (`u32`); returns a clear error if not
* Sets `priority` to the supplied value
* Automatically sets `time_last_updated` to the current UTC time (ISO 8601); adds the field if it doesn't already exist
* Edits the source file in-place (atomic write via temp file)
* Prints a confirmation message: `Prioritized task <ID> (priority → <PRIORITY>)`

**With `--no-edit`:**

* Does not modify the file
* Prints the complete reconstructed `@task(...)` string with the priority and timestamp updated
* Useful for editor plugin integration (e.g., Vim plugin injects the string into the buffer)

**Examples:**

```bash
# Set priority to 0 (highest urgency) for a task
ragtag task prioritize 0 a1b2c3d4e5f67890

# Set priority using an ID prefix
ragtag task prioritize 2 a1b2c3

# Print the updated tag string without modifying the file
ragtag task prioritize 1 a1b2c3d4e5f67890 --no-edit
```

---

## Filter Expressions

The `--filter <EXPR>` option accepts the same boolean filter syntax everywhere
it appears (`ragtag query`, `ragtag task list`, and `ragtag task summary`).

**Conditions.** The building block is a single `field <op> value` condition
using one of these comparison operators:

| Operator | Example | Description |
| --- | --- | --- |
| `=` | `status=active` | Equal |
| `!=` | `status!=done` | Not equal |
| `>` | `priority>0` | Greater than |
| `<` | `worktime_estimate<8` | Less than |
| `>=` | `priority>=1` | Greater than or equal |
| `<=` | `worktime_estimate<=4` | Less than or equal |

`field` is any attribute name valid for the command's tags. Comparisons parse
both sides as `f64` when possible and compare numerically; otherwise they fall
back to a lexicographic string comparison.

**Boolean composition.** Conditions can be combined with `AND` and `OR`
(case-insensitive) and grouped with parentheses. `AND` binds tighter than `OR`;
parentheses override precedence:

```
(status = active OR priority = 0) AND status != done
```

**Whitespace and quoting.** Whitespace around a condition's operator is optional
(`status = active` and `status=active` are equivalent). Wrap a value that
contains spaces in single or double quotes (e.g. `owner='John Doe'`).

**Multiple `--filter` flags.** Passing `--filter` more than once combines the
expressions with `AND` — a tag must satisfy every flag.

---

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Application error (config loading or validation, alias-engine failure, invalid filter, task not found, I/O error, etc.) |
| `2` | Command-line grammar error reported by clap (unknown command or option, missing value, unexpected argument, etc.) |

All errors are printed to stderr with a descriptive message.

## Environment Variables

| Variable | Description |
| --- | --- |
| `RAGTAG_CONFIG` | Path to the ragtag config file. Alternative to the `--config` CLI flag. The CLI flag takes precedence. If neither is set, ragtag uses walk-up config discovery (see [Configuration Reference](configuration.md)). |
| `RAGTAG_PATH` | Default search path for tags and tasks. Alternative to the `--path` CLI flag used by `summary`, `query`, and all `task` subcommands. The CLI flag takes precedence. If neither is set, defaults to `.` (current directory). |
| `RUST_LOG` | Controls log verbosity (e.g., `RUST_LOG=info` or `RUST_LOG=debug`). Uses the `env_logger` crate format. |
| `NO_COLOR` | When set, disables colored output. Overrides the `output.color` config setting but is itself overridden by the `--no-color` CLI flag. |

**Precedence order:** CLI flag > environment variable > default value.

These direct fallbacks are separate from generic configuration interpolation.
After YAML parsing, ragtag expands environment references in configuration
string values as documented in
[Environment Interpolation](configuration.md#environment-interpolation).
Command-line arguments are not interpolated by ragtag.

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

The `set-attr` command (and all status-change commands: `complete`, `activate`, `deactivate`, `block`, `abandon`, and `prioritize`) modify files using **atomic writes**: the updated content is written to a temporary file first, then moved into place. This prevents data loss from interrupted writes.

ragtag **refuses to edit symlinked files** — you must resolve the symlink or edit the target file directly.

### Tag Regeneration & Formatting Preservation

When a task is edited in-place, ragtag does **not** perform a surgical text patch — it regenerates the entire `@task(...)` tag string from the parsed model and substitutes the result back into the source file. During regeneration, ragtag preserves:

* The original indentation of the tag (leading whitespace on each line)
* The original attribute order
* Multi-line vs. single-line formatting (multi-line tags stay multi-line; one-line tags stay one-line)

If a tag being modified does not already contain a `time_last_updated` attribute (for example, because it was created with an older version of ragtag), the field is **appended** to the tag automatically and set to the current UTC time. `time_created` is never changed after the initial creation.
