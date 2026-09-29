---
name: architecture-review
description: Coordinate a read-only, behavior-preserving architecture and maintainability review by partitioning a repository into logical systems, reviewing approved systems individually, and synthesizing findings into an incremental roadmap.
---

# Architecture Review Coordinator

Use this skill when a repository has grown incrementally and needs a high-level architecture, boundaries, abstraction, duplication, and maintainability review without changing functionality.

This skill coordinates the focused skills in this directory. They are procedural modules: read the relevant `SKILL.md` before carrying out that phase. Do not imply that a child skill was automatically executed unless you actually followed its instructions.

## Required workflow

1. Read `review-standards/SKILL.md`.
2. Run `architecture-partition/SKILL.md`.
3. Stop and ask the user to approve or correct the proposed logical-system partition.
4. For each approved system, run `system-review/SKILL.md` exactly once, one system per turn unless the user asks for a batch. Stop after each system and wait for the next instruction.
5. After all approved systems have been reviewed, run `architecture-synthesis/SKILL.md`.

Do not skip the approval pause. Do not begin detailed system reviews merely because the repository already has directories or documentation describing subsystems.

## Scope

The review is read-only. Do not edit, reformat, rename, add tests, change configuration, or provide implementation patches. Recommendations may change structure, ownership, dependencies, or abstractions, but must preserve observable behavior unless clearly marked as an optional behavior change.

Inspect implementation, call sites, runtime flow, tests, configuration, build scripts, and relevant documentation. Treat documentation as evidence to verify, not as authority.

## Output discipline

Always separate facts, interpretations, and recommendations. Cite material findings with exact paths and symbols, and line numbers when practical. Do not report stylistic preferences unless they affect comprehension, correctness, change safety, or maintenance cost.

At the end of each phase, follow that phase skill's output format and stop at its defined handoff point. Never silently convert a review into a refactor plan or code change.
