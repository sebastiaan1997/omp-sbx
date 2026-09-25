# ADR-0005: Preserve Interactive Configure Menus in Rust

- Status: Accepted
- Date: 2026-09-21

## Context

The original shell configure command provided an interactive terminal workflow for selecting model tiers, thinking levels, and optional agent roles. The initial Rust port exposed only selector flags and silently applied defaults when values were omitted. That changed the user-facing behavior and made the Rust command unsuitable as a drop-in replacement.

## Decision

The Rust `configure` command preserves the interactive workflow when tier selectors are omitted and stdin is a terminal:

- Show model choices with provider, model name, and selector.
- Provide default fast, standard, and deep selections.
- Offer `inherit`, `off`, and model-supported thinking levels.
- Offer optional role assignments for plan, vision, designer, commit, tiny, task, and advisor.
- Preserve skipped optional roles instead of creating assignments.
- Keep explicit `--fast`, `--standard`, and `--deep` selectors for scripts and CI.
- Reject missing selectors in non-interactive execution rather than guessing.

The Rust implementation uses the discovered model catalog for validation. Unsupported thinking levels and unknown selectors remain errors.

## Consequences

- Interactive users retain the shell command's primary configuration path.
- Automation remains deterministic through explicit flags.
- The menu implementation is testable through the pure catalog, selector, and role-merging functions.
- The current Rust menu uses numbered terminal prompts; future terminal rendering improvements must preserve the same choices and selector semantics.
