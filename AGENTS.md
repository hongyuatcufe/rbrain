# Repository Rules

- Do not run mutating `cargo fmt` commands, including `cargo fmt` and `cargo fmt --all`, unless the user explicitly asks for formatting. Formatting creates large diffs that obscure functional changes and debugging signal.
- Check-only formatting commands such as `cargo fmt --all -- --check` are allowed when useful, because they do not modify files.
