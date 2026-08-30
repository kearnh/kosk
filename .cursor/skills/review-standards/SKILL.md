---
name: review-standards
description: Shared standards for evidence-backed, read-only architecture and maintainability reviews that preserve existing behavior.
---

# Review Standards

Apply these standards to every phase of the architecture review.

## Scope

The review is read-only. Do not edit files, reformat code, rename symbols, add tests, change configuration, or produce patches. Recommendations may affect structure, ownership, dependencies, or abstractions, but preserve observable behavior unless clearly marked as optional behavior change.

Do not silently expand into performance tuning, security auditing, feature design, dependency upgrades, or implementation. Mention those only when directly relevant to an architectural conclusion and label them outside scope.

## Evidence

- Verify documentation against implementation, call sites, runtime flow, tests, and configuration.
- Separate facts, interpretations, and recommendations.
- Cite exact file paths and symbols, with line numbers when practical.
- Do not claim a helper is unnecessary without checking all callers and its semantic role.
- Do not claim code is duplicated without checking whether paths have different invariants or valid reasons to diverge.
- If evidence is insufficient, state what must be inspected instead of guessing.

## Design judgment

- Prefer simple, explicit boundaries and clear ownership over clever abstractions.
- Preserve domain distinctions and existing behavior.
- Treat visibility or public-API changes as significant and justify them.
- Consider runtime behavior, not only static file layout.
- Account for platform boundaries, concurrency, persistence, configuration, errors, cancellation, and shutdown where they exist.
- Do not recommend a rewrite because another architecture is fashionable.
- Do not combine code solely because it looks similar or split code solely because a file is large.
- Report style only when it affects comprehension, correctness, change safety, or maintenance cost.

## Finding quality

Every material finding should explain:

- What is wrong or uncertain.
- Where the evidence is.
- Why it matters for maintenance.
- The smallest improvement that addresses it.
- Why behavior is preserved, or why it is an optional behavior change.
- Confidence and priority.
