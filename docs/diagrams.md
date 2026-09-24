# Source-Only D2 Diagrams

Ragtag can project configured task tags into deterministic
[D2](https://d2lang.com/) source. It does not execute D2 or render images.

```bash
ragtag diagram task-tree --path notes
ragtag diagram task-buckets --path notes --all
ragtag diagram task-tree --filter 'owner=alice AND status=active'
ragtag diagram task-tree --direction right --output tasks.d2
```

`task-tree` emits flat task boxes and parent-child edges. `task-buckets` emits
recursive containment: a selected task with selected children is one container,
and leaves are boxes. Both commands use the existing configured `tasks.tag_name`
and the same resolved task defaults and status categories as task commands.
There is no `diagrams:` configuration section.

Diagram discovery silently skips files whose bytes are not valid UTF-8,
including images and extensionless binary files. They do not produce diagram
diagnostics, count as invalid task records, or prevent valid text files in the
same path from producing output.

## Selection

By default, the categories in `tasks.exclude_status_categories` are hidden.
`--all` includes them. `--filter` uses the task boolean-expression syntax and
is strict: every field must be one of `id`, `pid`, `title`, `description`,
`owner`, `status`, `priority`, `worktime_spent`, `worktime_estimate`,
`time_created`, `time_last_updated`, or `worktime_units`. An exact `status`
field in the parsed expression disables default exclusions. A value or another
field that merely contains the text `status` does not.

This is intentionally stricter than `task list` and `task summary`, whose
historical compatibility behavior treats unknown fields as empty text and uses
the raw expression's `status` substring for the exclusion override.

Selection happens only after Ragtag validates the complete task graph. After
default status exclusions and any explicit filter are applied, every visible
task regains the excluded ancestor chain needed to explain its position. Such
ancestors are styled as context; unrelated siblings and descendants remain
omitted. With `--all`, those ancestors are ordinary selected tasks rather than
context.

Missing explicit IDs are recoverable: Ragtag warns, assigns a deterministic
source-occurrence key, and shows the task as a non-referenceable standalone
node. A parent on such a task is ignored with a warning. An unknown parent also
warns once and leaves the child at the root. Repeated recognized attributes
warn and retain the parser's first-match behavior. Empty titles warn and render
as `(untitled)`. Duplicate explicit IDs, malformed task records,
self-parenting, multiple parents, and cycles are fatal before D2 serialization
or output.

## Output and Safety

Omitting `--output`, or passing `--output -`, writes the complete in-memory D2
document through stdout followed by one newline. Validation and serialization
failures emit no D2 bytes. An operating-system write or flush failure can occur
after an arbitrary prefix reached the stream; Ragtag exits unsuccessfully and
reports that output may be partial.

On Linux, a file output is written to an exclusive mode-`0600` temporary file
in the destination directory, flushed and synchronized, revalidated, and
atomically renamed. Failures before rename clean up the temporary file and do
not replace the destination. Rename is the commit point. A directory-sync
failure after rename exits unsuccessfully with a distinct
`replacement committed, but directory durability could not be confirmed`
error; the destination may already contain the new bytes. Symlink and
non-regular destinations are rejected.

Reviewed secure file output is Linux-only. Windows and macOS builds support
stdout and receive compile-only CI checks; file output fails before mutation.

Untrusted task text never becomes D2 syntax. Ragtag converts controls, line
separators, ANSI/OSC controls, and bidirectional formatting characters to
visible text, creates the sole semantic label newline itself, uses injective
keys, and quotes backslashes, quotes, and D2 substitutions. CI validates output
with D2 `v0.9.0` and a checksum-pinned helper using that version's parser,
compiler, and semantic graph model. The helper asserts exact decoded labels,
node identities, containment, edge direction, classes, and generated styles.

Task labels contain the title and a trusted status/priority/owner line. Status
selects a fixed fill/stroke palette; priorities `0`, `1`, `2`, and `3+` select
stroke widths `4`, `3`, `2`, and `1`. Context adds fixed opacity and dash
properties. Task data cannot define D2 properties, classes, links, imports, or
expressions.

## Resource Limits

Diagram generation fails closed at these production bounds:

| Resource | Maximum |
| --- | ---: |
| Discovered paths | 100,000 |
| One normalized path | 32 KiB |
| Aggregate path bytes | 64 MiB |
| Aggregate retained source | 256 MiB |
| Tag occurrences | 1,000,000 |
| Diagnostics at an ordinary gate | 4,096 |
| Provider/integrity diagnostics at graph validation | 2,048 each |
| Parameters attached to one diagnostic | 64 |
| One diagnostic parameter | 16 KiB |
| Aggregate parameter count per diagnostic gate | 4,096 |
| Aggregate parameter bytes per diagnostic gate | 2 MiB |
| Rendered diagnostics at one gate | 8 MiB |
| Graph nodes or edges | 100,000 each |
| Properties per node | 32 |
| Aggregate graph properties | 1,600,000 |
| One graph text value | 64 KiB |
| Aggregate graph property/key bytes | 64 MiB |
| Document elements or edges | 100,000 each |
| One sanitized field | 64 KiB |
| One assembled label | 128 KiB |
| Aggregate document text | 64 MiB |
| Document nesting | 1,024 |
| Serialized D2 | 256 MiB |

Discovery stops before retaining path 100,001 or exceeding its byte budget.
Diagnostics are structured and bounded internally, rendered as terminal-safe
human text, and written to stderr. Count, parameter, and rendered-byte
overflow each produce one stable `DIA-LIMIT-*` sentinel rather than silently
dropping evidence. Machine-readable diagnostic serialization is not part of
this MVP.
