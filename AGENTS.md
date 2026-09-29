 # Version control — mandatory

- Use Jujutsu (`jj`) exclusively.
- `jj` is installed globally with WinGet; invoke it directly as `jj`.
- NEVER invoke `git`, including read-only commands such as `git status`, `git diff`, and `git log`.
- The presence of `.git` does not permit raw Git commands.
- `jj git ...` is allowed because the invoked program is `jj`.
- Use `jj status`, `jj diff`, and `jj log` for inspection.
- Do not create bookmarks or branches unless explicitly requested.
- After completing a task that edits files, create a commit with `jj commit -m "<message>"`. Do not ask for confirmation.
- Never include unrelated user changes in the commit.
- If an operation cannot be performed with `jj`, stop and ask rather than falling back to `git`.
- When writing something intended for human consumption, (comment, commit message, reply to prompt) use as few words as possible. Pick every word meticulously to reduce the volume to a strict minimum. Be down to the point. Less is more. 

# General

- For code comments: do not refer in comments to anything that only makes sense in the context of the AI chat, a future reader must be able to make sense of comments without this context - this includes any references to how code used to be, the only context a reader has is how the code is when reading it.

- User-facing names (config keys, mapping actions, `when` flags, overlay labels, comments in `config.toml` / `mappings.toml` / `MAPPINGS.md`) must be fully descriptive. Name the subject (`suggestionJustAccepted`, not `justAccepted`). Do not put internal identifiers in those texts. Do not treat shipped mappings as facts in comments.

- Avoid superlatives and praise. Stop telling me I am absolutely right. Give me the cold hard truth.

- Avoid magic numbers and strings by extracting recurring or meaningful values into descriptive constants (const) or enums. Keep self-explanatory, one-off values inline to avoid clutter. If a value comes from a spec (e.g. HTTP 200 OK), use a constant regardless.

- Reduce code indentation. Avoid Arrow Anti-Pattern. Leverage early return and continue.

- Let the reader of the code breathe. Add empty lines between logical blocks of code.

- Treat symbol visibility changes as a breaking design shift. Keep all symbols private unless external access is strictly required by the design. Prompt the user for explicit approval before changing any access modifier.

- Program to levels of abstraction. Lower-level mechanics must be encapsulated in a dedicated abstraction layer. Expose clean, high-level APIs to the rest of the application so calling code works with domain concepts, not raw implementation details.

- Don't touch blocks of code unrelated to the feature you implement. e.g. Don't add comments to a block of code if you did not create it or modify it. As much as possible try to minimize the number of changed lines when implementing a feature.

- Strictly adhere to the layered boundary hierarchy: each layer may only communicate with its immediate neighbor directly below it. Never "punch holes" through layers (e.g., controllers or UI components must never directly call database queries, raw hardware drivers, or low-level network clients; always route through the intermediate service/abstraction layer).

- If the prompt indicates that a bug is being fixed, don't write the fix right away. First write the test. Observe it failing. Then write the fix. And observe the test passing.

# Validation

After editing Rust files:

1. Run `cargo fmt`.
2. Run `cargo clippy`.
3. Address warnings caused by the changes.
4. Do not modify unrelated code solely to address existing warnings.

## Editing
- Prefer the `edit` tool (hashline patch) for every file modification.
- Never rewrite a whole file when a targeted edit suffices.
- On a stale-tag/hash error or parse failure: stop, re-read, retry.
