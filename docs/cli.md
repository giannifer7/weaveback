# CLI reference

## `azadi` — combined tool

Runs macro expansion and chunk extraction in one pass.

```bash
azadi [OPTIONS] <INPUTS>...      # explicit files
azadi [OPTIONS] --dir <DIR>      # auto-discover driver files
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--input-dir <PATH>` | `.` | Base directory prepended to every input path |
| `--special <CHAR>` | `%` | Macro invocation character |
| `--include <PATHS>` | `.` | Include search paths for `%include`/`%import` (colon-separated on Unix) |
| `--gen <PATH>` | `gen` | Base directory for generated output files |
| `--open-delim <STR>` | `<[` | Chunk-open delimiter |
| `--close-delim <STR>` | `]>` | Chunk-close delimiter |
| `--chunk-end <STR>` | `@` | End-of-chunk marker |
| `--comment-markers <STR>` | `#,//` | Comment prefixes recognised before chunk delimiters (comma-separated) |
| `--formatter <EXT=CMD>` | | Run a formatter after writing each file, e.g. `rs=rustfmt`; repeatable |
| `--dump-expanded` | off | Print macro-expanded text to stderr before noweb processing |
| `--dir <DIR>` | | Auto-discover driver files; mutually exclusive with positional inputs |
| `--ext <EXT>` | `md` | File extension to scan in `--dir` mode; repeatable |
| `--depfile <PATH>` | | Write a Makefile depfile listing every source file read |
| `--stamp <PATH>` | | Touch this file on success (build-system stamp) |
| `--db <PATH>` | `azadi.db` | Path to the source-map database |
| `--allow-env` | off | Enable `%env(NAME)` (disabled by default to protect secrets) |

### Directory mode

`--dir` scans a directory tree recursively for files matching `--ext`,
determines which are *drivers* (top-level files) vs *fragments* (included by
another file), and processes each driver. No changes needed when new files are
added.

```bash
azadi --dir src --include . --gen src
azadi --dir src --ext adoc --include . --gen src
azadi --dir src --ext md --ext adoc --include . --gen src
```

`--dump-expanded` is the first thing to check when a chunk can't be found or
expands unexpectedly — it prints the macro-expanded intermediate text that
azadi-noweb receives.

### Build-system integration

`--depfile` and `--stamp` together let a single build rule cover an entire
directory of literate sources:

```meson
custom_target('gen',
  output  : ['gen.stamp'],
  depfile : 'gen.d',
  command : [azadi,
             '--dir',     meson.current_source_dir() / 'src',
             '--ext',     'adoc',
             '--include', meson.current_source_dir(),
             '--stamp',   '@OUTPUT0@',
             '--depfile', '@DEPFILE@'],
)
```

> **Ninja note:** list only the stamp in `output`, never the `.d` file.
> Ninja consumes the depfile internally; declaring it as an output makes ninja
> think it is always missing and reruns the target on every build.

---

## `azadi-macros` — macro expander only

```bash
azadi-macros [OPTIONS] <INPUTS>...
azadi-macros [OPTIONS] --dir <DIR>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--output <PATH>` | `-` | Output file (`-` for stdout) |
| `--special <CHAR>` | `%` | Macro invocation character |
| `--include <PATHS>` | `.` | Include search paths |
| `--pathsep <STR>` | `:` / `;` | Path separator (platform default) |
| `--input-dir <PATH>` | `.` | Base directory prepended to each input path |
| `--allow-env` | off | Enable `%env(NAME)` |
| `--dir <DIR>` | | Auto-discover driver files |
| `--ext <EXT>` | `md` | File extension to scan in `--dir` mode; repeatable |

---

## `azadi-noweb` — chunk extractor only

```bash
azadi-noweb [OPTIONS] <FILES>...
```

| Flag | Default | Description |
|------|---------|-------------|
| `--gen <PATH>` | `gen` | Base directory for generated output files |
| `--output <PATH>` | stdout | Output for `--chunks` extraction |
| `--chunks <NAMES>` | | Comma-separated chunk names to extract to stdout |
| `--open-delim <STR>` | `<[` | Chunk-open delimiter |
| `--close-delim <STR>` | `]>` | Chunk-close delimiter |
| `--chunk-end <STR>` | `@` | End-of-chunk marker |
| `--comment-markers <STR>` | `#,//` | Comment prefixes (comma-separated) |
| `--formatter <EXT=CMD>` | | Run a formatter after writing each file |
