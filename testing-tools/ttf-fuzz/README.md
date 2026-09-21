## Build

Install AFL first:

```text
cargo install afl
```

and then build via `cargo-afl`:

```text
cargo afl build
```

## Run

Before running, we have to collect some test data.
Using raw fonts is too wasteful, so we are using the `strip-tables.py` script
to remove unneeded tables.

Here is an example to test `cmap`/`Face::glyph_index`:

```text
strip-tables.py glyph-index in /usr/share/fonts
cargo afl fuzz -i in -o out target/debug/fuzz-glyph-index
```
