# Tag Syntax Reference

This document describes the complete syntax for ragtag tags.

## Basic Format

Tags always begin with `@` followed by a name:

```
@tag
@todo
@bookmark
```

Tags can also have attributes inside parentheses:

```
@tag(attr1, attr2)
@tag(key=value)
@tag(key1=value1, key2="value two")
```

## Tag Discovery Rules

ragtag only recognizes `@` as a tag when it appears at the **start of the input** or is **preceded by whitespace** (space, tab, newline, or carriage return). This avoids matching email addresses and other non-tag uses of `@`.

| Input | Recognized? |
| --- | --- |
| `@tag` (start of file) | ✅ Yes |
| `some text @tag` | ✅ Yes |
| `\t@tag` (after tab) | ✅ Yes |
| `email@address.com` | ❌ No |
| `abc@tag` | ❌ No |

## Tag Names

Tag names follow these rules:

* **First character:** must be an ASCII letter (`a-z`, `A-Z`), underscore (`_`), or hyphen (`-`)
* **Subsequent characters:** ASCII letters, digits (`0-9`), underscores, or hyphens
* **Cannot start with a digit**
* **Maximum length:** 256 characters
* Parsing stops at any character that is not alphanumeric, `_`, or `-`

### Valid Tag Names

```
@tag
@TAG
@tag123
@tag_1
@tag-1
@_tag
@-tag
@tag-----example
@My-Long_Tag_Name
```

### Invalid Tag Names

```
@1tag           — starts with a digit (ignored entirely)
@tag::bad       — parsed as @tag (stops at the colon)
@tag?           — parsed as @tag (stops at the question mark)
```

## Attributes

Attributes are specified inside parentheses immediately after the tag name. They are comma-separated and support both **named** and **positional** forms.

### Positional Attributes

Values without a `key=` prefix are positional:

```
@tag(hello, world, 42)
```

### Named Attributes

Values with a `key=value` prefix are named:

```
@tag(color=blue, count=3)
```

Attribute names follow slightly different rules than tag names:

* **First character:** ASCII letter or underscore (hyphens are **not** allowed as the first character)
* **Subsequent characters:** ASCII letters, digits, underscores, or hyphens

### Mixed Attributes

Positional and named attributes can be mixed freely:

```
@tag(positional_value, key=named_value)
```

### Whitespace and Multi-line

Whitespace (including newlines) is freely allowed between attributes:

```
@tag(   key1=value1,key2=value2)

@tag(
    key1=value1,
    key2=value2,
    key3="a longer value"
)
```

### Trailing Commas

A trailing comma after the last attribute is allowed and ignored:

```
@tag(a, b, c,)
```

### Attribute Limits

A single tag may have at most **256 attributes**. Tags exceeding this limit are not parsed.

## Value Types

Attribute values are parsed in the following order of precedence:

### 1. Quoted Strings

Values enclosed in double (`"`) or single (`'`) quotes are always treated as strings, even if they contain numeric content:

```
@tag(name="hello world", alt='single quoted')
```

**Escaping:** Use a backslash (`\`) before any character to include it literally. There are no special escape sequences — `\n` inserts a literal `n`, not a newline.

```
@tag(value="she said \"hi\"")     — embedded double quotes
@tag(value="path\\to\\file")      — embedded backslashes
@tag(value='it\'s fine')          — embedded single quote
```

An unterminated quoted string (no closing quote before end-of-file) causes the tag parse to fail.

### 2. Prefixed Integers

Bare values starting with `0x`, `0o`, or `0b` are parsed as hexadecimal, octal, or binary integers respectively:

| Prefix | Base | Example | Decimal Value |
| --- | --- | --- | --- |
| `0x` or `0X` | 16 (hex) | `0xff` | 255 |
| `0o` or `0O` | 8 (octal) | `0o77` | 63 |
| `0b` or `0B` | 2 (binary) | `0b1010` | 10 |

If the digits after the prefix are not valid for that base, the value falls through to string.

### 3. Floating-Point Numbers

Bare values containing a decimal point (`.`) that parse as valid `f64` are stored as floats:

```
@tag(time=4.5, rate=-1.25)
```

### 4. Decimal Integers

Bare values that parse as valid `i64` (without a decimal point) are stored as integers:

```
@tag(count=42, offset=-7)
```

### 5. String Fallback

If none of the above conversions succeed, the value is kept as a string:

```
@tag(status=active, color=blue)
```

### Bare Value Boundaries

Bare (unquoted) values are terminated by any of these characters:

* Whitespace (space, tab, newline)
* Comma (`,`)
* Closing parenthesis (`)`)
* Single quote (`'`)
* Double quote (`"`)
* Equals sign (`=`)

Bare values have a maximum length of **4096 characters**.

## Duplicate Attributes

If multiple named attributes share the same key, **the first one wins** when accessed by name:

```
@tag(key=a, key=b)    — looking up "key" returns "a"
```

## Complete Examples

### Simple Tags

```
@todo
@note
@bookmark
```

### Tags With Attributes

```
@todo(priority=0, owner="alice")
@bookmark(url="https://example.com", title="Example")
@metric(0xff, 0b1010, 3.14)
```

### Multi-line Task Tag

```
@task(
    id="a1b2c3d4e5f67890",
    title="Implement feature X",
    description="Add support for feature X",
    owner="alice",
    status="active",
    priority=1,
    worktime_estimate=8,
    time_created="2026-06-12T09:00:00Z",
    time_last_updated="2026-06-12T10:00:00Z",
    workworktime_units="hours"
)
```

### Edge Cases

```
@tag()                  — valid, no attributes
@tag(   )               — valid, whitespace-only parens
@tag(a, b,)             — valid, trailing comma
@tag(val="has ) inside") — valid, paren inside quotes
```
