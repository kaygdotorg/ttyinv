# ttyinv engineering rules

## Speak only in Simplified Technical English

- Use ASD-STE100 as the writing standard.
- Keep each instruction to 20 words or fewer.
- Write one instruction in each sentence.
- Use active voice and name the agent that performs each action.
- Use present tense for facts and imperative mood for instructions.
- Use one approved word for each meaning.
- Use no synonym for an approved meaning.
- Keep technical names, file paths, commands, and code symbols exactly as they are.
- Use no idioms, metaphors, or humour.
- Limit each noun cluster to three words.
- Apply this rule to code comments, commit messages, documentation, and replies to the user.

## Keep one source of truth

- Keep business policy, limits, currencies, identifiers, and shared labels in one named configuration or domain module.
- Make runtime code, CLI copy, metadata, and tests import that source instead of repeating literals.
- Stop when one conceptual change requires edits in more than one runtime file.
- Extract the shared concept before shipping.
- Favor small typed modules and pure helpers with focused tests instead of copied values or branches.
- Make tests verify shared configuration behavior and internal consistency without redefining production constants.
- Make operational documentation point to authoritative configuration instead of duplicating values that can drift.
- Keep library interfaces thin.
- Put domain validation and transformation in reusable library code.
- Make CLI commands compose those modules.

## Use the code graph

- Run `code-review-graph build` on a cold checkout before reasoning.
- Run `code-review-graph update` after edits.
- The graph covers the Python CLI in `src/ttyinv`.
- Answer structural questions with the graph instead of file-by-file reading.
- Use `search`, `query`, `impact`, `architecture`, `communities`/`community <id>`, `flows`/`flow <id>`, `dead-code`, and `large-functions`.
- Run `impact` before you change an export.
- Open only the files that the graph names.
- Use `rtk` to keep shell output short.
- Use these `rtk` commands: `rtk ls`, `rtk read`, `rtk grep`, `rtk git`, `rtk diff`, `rtk test`, `rtk ruff`, `rtk format`, and `rtk err`.
- Use `rtk find` only with simple predicates (`-name`, `-type`).
- Use plain `find` for compound predicates.

## Plan and delegate tasks

- The lead agent plans, schedules, and integrates.
- Give each self-contained slice to the most specific role.
- Use `scout` for read-only investigation, `reviewer` for change quality, and `security-reviewer` for secret and supply-chain exposure.
- Use `librarian` for external API facts and `sonic` for mechanical edits.
- Run independent slices in one batch.
- State the shared contract before work starts.
- Tell siblings to skip formatters, linters, and the full test suite.
- Run those checks once at the end.
- Use `skill://harness-web-search` when facts about font licence terms, packaging, or dependency behavior may have changed since training.
- Use it when an important claim needs a second independent searcher.
