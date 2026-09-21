# Contributing to ttf-parser

`ttf-parser` is a `#![no_std]`, `#![forbid(unsafe_code)]`, zero-allocation parser for
TrueType, OpenType and AAT fonts. It sits underneath text rendering and it parses untrusted
input, so contributions are held to that standard.

## Scope

**The crate is in maintenance mode: bug fixes only.**

In scope, and welcome:

- Incorrect parsing, wrong values, glyphs that fail to outline
- Panics, aborts and unbounded work on malformed input
- Fixes to existing table support

Out of scope, and will be closed:

- New table support and new public API
- Performance work
- Refactors that change the API surface

If you need broader coverage, [fontations](https://github.com/googlefonts/fontations) is
actively developed and is what we recommend for new projects.

A bug report does not need to be phrased as a fix — if a font renders wrong, say so and we
will work out whether the cause is in scope.

## Repository layout

The repository root is **not** a Cargo workspace. It holds four independent crates:

| Path | What it is |
|---|---|
| `.` | the `ttf-parser` library |
| `c-api/` | C bindings, producing `libttfparser` and a cbindgen-generated `ttfparser.h` |
| `benches/` | benchmarks against stb_truetype, FreeType and others |
| `testing-tools/ttf-fuzz/` | legacy afl fuzz targets, unmaintained (see below) |

`testing-tools/font-view/` is a Qt/C++ visual debugging tool. It is not built by CI.

Because these are separate crates, commands have to be run from the right directory.
`cargo test` at the root does not touch `c-api/`.

## MSRV and edition

MSRV is **1.88.0** and the crate is on **edition 2024**. The MSRV appears in three places
that must agree: `rust-version` in `Cargo.toml`, the CI matrix in
`.github/workflows/main.yml`, and the badge in `README.md`.

## Building and testing

### The library

The feature matrix matters more than usual: `std` and `no-std-float` are mutually exclusive
floating-point backends, one of which is mandatory, and `variable-fonts` roughly doubles
binary size. CI builds every one of these:

```sh
cargo build --no-default-features --features=no-std-float
cargo build --no-default-features --features=std
cargo build --no-default-features --features=alloc,no-std-float,variable-fonts,gvar-alloc
cargo build --no-default-features --features=variable-fonts,no-std-float
cargo build --all-features
```

A change that compiles under `--all-features` but not under
`--no-default-features --features=no-std-float` is the most common CI failure. If you touch
float arithmetic or anything behind a `#[cfg(feature = ...)]`, build the no-std leg locally.

### Tests, in both profiles

```sh
cargo test
cargo test --release
```

**Run both.** `debug_assert!` is compiled out in release, so the two profiles can disagree
about whether malformed input aborts or is rejected cleanly. A test that passed in debug and
failed in release went unnoticed on `main` for months because CI ran only debug.

### The C API

```sh
cd c-api
cargo build --no-default-features
cargo build --no-default-features --features=variable-fonts
cargo test
```

`cargo test` here is not optional: the root is not a workspace, so a root `cargo test` never
reaches this crate. Its test module went unrun for its entire existence.

The C smoke test is compiled with AddressSanitizer and `-Werror` against the freshly built
dynamic library:

```sh
cargo build
gcc test.c -o test -L./target/debug/ -lttfparser -Werror -fsanitize=address
env LD_LIBRARY_PATH=./target/debug/ ./test
```

On macOS, use `clang` and `DYLD_LIBRARY_PATH`.

`c-api/ttfparser.h` is cbindgen-generated and then hand-edited. If you change an exported
signature, update the header to match; it is not regenerated automatically.

### Benchmarks

```sh
cd benches
cargo bench dummy   # `cargo build` does not actually build them
```

## Linting and formatting

Both are enforced in CI and both must pass:

```sh
poly lint --no-workspace .
poly fmt --check .
poly fmt --fix .          # to apply
```

`--no-workspace` matters: without it, `poly lint` shells out to `cargo clippy` and friends.
This crate has never carried a clippy configuration, so that produces an unbounded set of
findings unrelated to your change.

Configuration lives in `poly.toml`. Two exclusions are deliberate and should not be removed
without discussion:

- **`tests/tables/**`** — hand-built binary fixtures where one source line is one logical
  record and the trailing comment names it. A formatter puts each literal on its own line,
  which detaches the comment from what it describes. This is why those modules are declared
  `#[rustfmt::skip] mod x;` in `tests/tables/main.rs`.
- **`c-api/ttfparser.h`** — generated then hand-edited; formatting fights regeneration.

## Comments are part of the source

This is the one rule worth stating explicitly.

This crate's comments encode OpenType, TrueType and AAT specification detail that **cannot be
re-derived from the code**: why a `loca` table has `numGlyphs + 1` offsets, which binary
search invariant a function relies on, why `deltas_are_zero` overrides `deltas_are_words`,
the operand layout of a CFF charstring. Deleting them destroys knowledge that took someone a
spec reading to acquire.

Do not run comment-stripping tools over this repository. `poly`'s `uncomment` engine is
pinned off in `poly.toml`, with `per-file-ignores` on the `comment` and `doc-comment`
diagnostic codes as a backstop that survives a later config change. Note that a standalone
`uncomment` binary exists outside `poly` and **no `poly.toml` can govern it** — that one is
guarded by review only.

Mark a comment that carries non-inferrable rationale with `~keep`.

## Writing tests

Tests live in `tests/tables/`, one module per table, registered in `tests/tables/main.rs`.
Build fixtures inline with the `convert(&[Unit...])` helper rather than committing binary
font files; `tests/tables/cff1.rs` shows the pattern for something as involved as a full
CFF font.

Two expectations:

- **Assert exact values**, not truthiness. `assert_eq!(result, 42)`, not `assert!(result)`.
- **Verify the test fails without the fix.** Revert your change, watch the test go red, then
  restore it. A test that passes against broken code is not testing anything. For security
  fixes, check both profiles — an argument-stack underflow shows as a `debug_assert` in debug
  and as an out-of-bounds index in release, and only the release message reveals the real
  failure.

## Security-relevant changes

Most open work on this crate is hardening against malformed input. If you are adding a limit:

- Give it a **named constant with a comment justifying the value**, ideally citing a measured
  corpus or a comparable limit in harfbuzz or fontations. "Chosen arbitrarily" invites someone
  to raise it later without evidence.
- **Bound the right thing.** A depth cap does not bound total work: a graph with fan-out `b`
  and depth `d` performs `b^d` visits while never exceeding depth `d`. Several bugs here were
  exactly this mistake.
- **Do not leak state between calls.** A budget must be a fresh local per public entry point.
  A counter that persists makes the API start failing spuriously after enough calls.
- **Charge the budget before any early return**, or an attacker gets the cheap paths free.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
type(scope): subject

Body explaining why, if the subject is not self-evident.
```

- **Types**: `fix`, `feat`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`.
  Given the crate is in maintenance mode, nearly everything is `fix`, `docs`, `test` or `chore`.
- **Scope** is the table or module the change belongs to, matching how the changelog groups
  entries: `fix(cmap):`, `fix(CFF2):`, `fix(gvar):`. Omit it for changes that span the crate.
- **Subject** is imperative mood, no trailing period, first line under 72 characters.
- The body explains *why*, not what — the diff already says what. Reference an issue by number
  when one exists.
- A breaking change gets a `!` after the type (`feat(cff)!:`) and a note in the changelog under
  `### Changed`.

History before 0.25.1 predates this convention and is not being rewritten, so `git log` mixes
both styles. New commits use the convention.

## Pull requests

- Add a `## [Unreleased]` entry to `CHANGELOG.md`. Contributed changes are credited in the
  form `Thanks to [name](https://github.com/name).`
- Call out breaking changes explicitly under `### Changed`, including behavioural ones that
  produce no compile error.
- One logical change per PR. Unrelated fixes riding along make review slower and bisection
  worse.

## Fuzzing

`testing-tools/ttf-fuzz/` is **not maintained** and does not compile — it has called
`Face::parse` expecting an `Option` since that function started returning a `Result` in
0.8.0. Do not use it as a starting point.

The crate is fuzzed by OSS-Fuzz, whose targets are currently vendored in the `google/oss-fuzz`
repository rather than here. See the open issue about OSS-Fuzz project ownership before
investing effort in fuzzing infrastructure.
