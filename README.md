# InScope

**InScope** is a Rust CLI for managing audit scope in codebases.

It helps you:

- define what is in scope (`IN`)
- define explicit exclusions (`OUT`)
- keep file markers in sync with scope rules
- avoid scope drift during audits

## Why

Audit scope gets messy fast with exceptions and refactors. This tool helps protocols make audit, competition, and bug bounty scoping easier for security researchers, while also letting researchers mark files that are in scope.

**InScope** gives you a single source of truth (`audit-scope.yml`) and deterministic behavior:

- `OUT` always wins over `IN`
- recursive directory handling
- glob support (`contracts/**/*.sol`)
- dry-run mode before touching files

## Installation

From GitHub:

```bash
cargo install --git https://github.com/IcarusxB/InScope.git
```

From local source:

```bash
git clone https://github.com/IcarusxB/InScope.git
cd InScope
cargo install --path .
```

## Quick Start

```bash
inscope init
inscope in --add 'contracts/**/*.sol'
inscope out --add contracts/protocols/dex/Bad.sol
inscope list
inscope sync --dry-run
inscope sync
```

## Commands

### Scope Management

```bash
inscope in --add <PATH_OR_GLOB>
inscope in --remove <PATH_OR_GLOB>
inscope in --list

inscope out --add <PATH_OR_GLOB>
inscope out --remove <PATH_OR_GLOB>
inscope out --list
```

### Marker Operations

```bash
inscope annotate --apply [--dry-run]
inscope annotate --remove [--dry-run]
inscope sync [--dry-run]
```

### Info

```bash
inscope status
inscope list
inscope --help
```

## Behavior Rules

- `OUT` has precedence over `IN`.
- If a directory is `IN` and a file inside it is `OUT`, that file is out of scope.
- `sync` runs in two phases:
  - `IN pass`: apply markers to effective in-scope files
  - `OUT pass`: remove markers from out-of-scope files

## File Types and Markers

InScope chooses marker style by file type:

- `// @scope`: `rs, js, ts, jsx, tsx, sol, c, cpp, h, hpp, java, go, swift, kt`
- `# @scope`: `py, sh, bash, zsh, rb, yml, yaml, toml` (also shebang fallback)
- `-- @scope`: `sql, lua`
- `/* @scope */`: `css`
- `<!-- @scope -->`: `html, xml, md`

Files like `json` are skipped for annotation.
System/non-text files (for example `.DS_Store`) are skipped.

## Ignored Paths

Recursive traversal skips:

- `.git`
- `node_modules`
- `target`

## Scope Manifest

`inscope init` creates `audit-scope.yml`:

```yaml
version: 1
include: []
exclude: []
files: {}
```

## Example `audit-scope.yml`

```yaml
version: 1
include:
  - contracts/protocols/dex
  - "contracts/**/*.sol"
exclude:
  - contracts/protocols/dex/Bad.sol
files: {}
```

## Typical Workflow

1. Initialize scope file.
2. Add broad in-scope paths.
3. Add specific out-of-scope exceptions.
4. Run `sync --dry-run`.
5. Run `sync`.
6. Re-run `list` and `status` as scope evolves.

## Notes

- Output is boxed for readability and consistency in terminal logs.
