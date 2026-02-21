# Skills Polyfill Instructions

Use this file when your agent harness does not support native skill loading.

## Goal

Approximate skill behavior by manually discovering and loading only relevant skill instructions.

## Workflow

1. Scan for candidate skill files under `.agents/skills/**/SKILL.md`.
2. Read the first lines of each `SKILL.md` to identify purpose, triggers, and scope.
3. Pick only the skill files relevant to the current task.
4. Read the selected `SKILL.md` files fully and follow their instructions.
5. If a selected skill references extra files (for example `scripts/`, `references/`, or templates), open only the files needed for the active task.
6. Prefer existing scripts/templates from the skill folder over rewriting the same logic from scratch.
7. If multiple skills match, use the minimal set and execute them in a clear order.
8. If a skill is missing or unclear, continue with the best local fallback and note the gap briefly.

## Practical Notes

- Keep context small: skim all skills first, then deep-read only relevant ones.
- Resolve relative paths from the skill directory that contains the `SKILL.md`.
- Do not bulk-load all referenced files unless the task actually needs them.
