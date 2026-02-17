# Project Rules

## Behavioral Rules (Always Enforced)

- ALWAYS prefer editing an existing file to creating a new one
- NEVER proactively create documentation files (\*.md) or README files unless explicitly requested
- NEVER save working files, text/mds, or tests to the root folder
- ALWAYS read a file before editing it
- NEVER commit secrets, credentials, or .env files
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL terminal operations in ONE Bash message (note: if one parallel call fails, siblings may cascade-fail — re-run unfailed queries in next batch)
- **ALWAYS preserve data for reuse** in reports/emails (see Data Reusability below)
- When analyzing code or exploring the codebase, use LSP and project-specific tools first, not raw grep/bash. Check for existing project utilities before reaching for generic CLI tools.
- When modifying file paths or moving files, always fix ALL cross-references and imports across the entire codebase. Never create copies of files as a workaround — fix the actual path references instead.
- Use `gh` CLI for GitHub operations. When project-specific GitHub tooling exists, prefer it over raw CLI.
- ALWAYS store project-related memories in `.claude/memory/` folder — not in project root or other locations

## Critical Rules

### Conventions

- Follow DRY (Don't Repeat Yourself) and KISS (Keep It Simple, Stupid) principles
- Apply modern software design patterns (separation of concerns, composition over inheritance, dependency inversion)
- Every struct/class must have a single responsibility — if it does more than one thing, split it
- Keep files/modules under 500 lines; refactor when approaching this limit

## Context (Read When Relevant)

| Working On | Read                    |
| ---------- | ----------------------- |
| Rust code  | `.claude/rules/rust.md` |
