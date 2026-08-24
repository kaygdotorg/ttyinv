# ttyinv engineering rules

## Prefer one source of truth

- Keep business policy, limits, currencies, identifiers, and shared labels in a single named configuration or domain module. Runtime code, CLI copy, metadata, and tests must import that source instead of repeating literals.
- When one conceptual change requires edits in more than one runtime file, stop and extract the shared concept before shipping. Favor small typed modules and pure helpers with focused tests over copy-pasted values or branches.
- Tests should verify the behavior and internal consistency of shared configuration without redefining the production constant. Operational documentation should point to the authoritative configuration instead of duplicating values that can drift.
- Keep library interfaces thin: domain validation and transformation belong in reusable library code, while CLI commands compose those modules.

## Read this codebase through the graph

- Index before reasoning: `code-review-graph build` on a cold checkout, `code-review-graph update` after edits. The graph covers the Python CLI in `src/ttyinv`.
- Answer structural questions with the graph rather than file-by-file reading: `search`, `query`, `impact` (blast radius before changing an export), `architecture`, `communities`/`community <id>`, `flows`/`flow <id>`, `dead-code`, `large-functions`. Open only the files it names.
- Route shell output through `rtk` to keep it small: `rtk ls`, `rtk read`, `rtk grep`, `rtk git`, `rtk diff`, `rtk test`, `rtk ruff`, `rtk format`, `rtk err`. `rtk find` accepts simple predicates only (`-name`, `-type`); compound predicates go to plain `find`.

## Orchestrate; delegate the legwork

- The lead agent plans, schedules, and integrates. Hand each self-contained slice to the most specific role: `scout` for read-only investigation, `reviewer` for change quality, `security-reviewer` for secret and supply-chain exposure, `librarian` for external API facts, `sonic` for mechanical edits.
- Fan out independent slices in one batch, state the shared contract up front, and have siblings skip formatters, linters, and the full test suite; the lead runs those once at the end.
- Reach for `skill://harness-web-search` when a fact about font licence terms, packaging, or dependency behavior may have moved since training, and when a load-bearing claim needs a second independent searcher.
