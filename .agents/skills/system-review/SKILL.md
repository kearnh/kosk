---
name: system-review
description: Review one approved logical system for cohesion, boundaries, state ownership, abstractions, duplication, testability, and maintainability while preserving behavior.
---

# Logical System Review

Use this after the user has approved a repository partition. Review exactly one approved system at a time. Read `review-standards/SKILL.md` first. If the approved partition or target system is missing, ask for it rather than guessing.

## Procedure

Inspect the target system in context. Read its implementation, all important callers and callees, relevant tests, configuration, runtime entry points, and documentation. Follow dependencies far enough to understand contracts and side effects, but do not turn this into an undifferentiated repository review.

Evaluate:

1. **Responsibility and cohesion** — mixed responsibilities, misplaced behavior, and unclear purpose.
2. **Boundaries and dependencies** — direction, cycles, dependency leaks, concrete-detail coupling, and abstraction level.
3. **State and ownership** — lifetime, mutability, synchronization, initialization, global state, and representable invalid states.
4. **Data and control flow** — policy versus mechanism, side effects, conversions, copies, allocations, locks, callbacks, errors, cancellation, retries, and shutdown where applicable.
5. **Abstractions and APIs** — unnecessary wrappers, pass-through helpers, indirection, generic abstractions, missing boundaries, naming, visibility, and actual contracts.
6. **Duplication and consistency** — meaningful duplicated logic and inconsistent validation, lifecycle, error, or state-transition rules. Distinguish deliberate domain differences.
7. **Extensibility and testability** — likely changes that spread across too many locations and boundaries that are difficult to test.
8. **Smaller code-quality issues** — needless helpers, awkward control flow, misleading names, stale comments, dead paths, and overly broad modules when they have a real maintenance consequence.

For every finding, use exactly this structure:

- **Finding:** one precise sentence.
- **Evidence:** exact files/symbols, call sites, and relevant flow.
- **Impact:** what becomes harder, riskier, slower, or more error-prone.
- **Recommendation:** the smallest structural improvement that addresses it.
- **Behavior:** why it preserves current behavior, or that it is an optional behavior change.
- **Confidence:** high, medium, or low, with the reason for uncertainty.
- **Priority:** critical, high, medium, or low, based on maintenance value and risk.

Do not prescribe a rewrite because another architecture is fashionable. Do not combine code solely because it looks similar, split code solely because a file is large, or call a helper unnecessary without checking all callers and its semantic role.

## Required ending

End with:

- Strengths worth preserving.
- Recommended improvements ordered by value.
- Issues deferred to cross-system synthesis.
- The smallest safe sequence for making improvements, without implementing them.
- Evidence gaps or questions that need user input.

Then stop and wait for the user before reviewing another system.
