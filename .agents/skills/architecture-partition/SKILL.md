---
name: architecture-partition
description: Build a verified mental model of a repository and partition it into largely independent logical systems before detailed architecture review.
---

# Architecture Partition

Use this as the first phase of a read-only architecture review. The goal is a useful conceptual partition of the running system, not a proposed file move and not a copy of the existing directory tree.

## Procedure

1. Inspect the repository structure, entry points, build configuration, runtime startup, major flows, tests, documentation, and platform-specific code.
2. Trace the important paths through the implementation. Identify domain concepts, state holders, threads/tasks, external interfaces, persistence, configuration, and side effects.
3. Compare documentation with code. Record contradictions instead of choosing one silently.
4. Group code into the smallest useful set of systems that have coherent responsibilities and can be reasoned about with minimal knowledge of the others.
5. Identify boundaries that are real in behavior or ownership even when the code does not currently express them cleanly.

## Required output

Start with a short repository summary containing verified facts. Then provide a table with one row per proposed system and these columns:

- System name
- Purpose and responsibilities
- State and invariants owned
- Inputs, outputs, and externally visible effects
- Interfaces to other systems
- Direct dependencies and dependants
- Representative paths and symbols
- Reason this grouping is useful

Follow the table with:

- A compact dependency diagram or dependency list.
- Cross-cutting concerns that must not be assigned to one system incorrectly.
- Shared state, cycles, dependency leaks, and highly entangled areas.
- Glue/orchestration code versus domain logic.
- Systems that are provisional or too entangled for isolated review.
- A recommended review order based on dependencies and risk.
- Explicit assumptions and missing evidence.

Do not recommend code changes in this phase. Do not treat current directories, modules, or docs as the architecture without verification. The partition is a mental model for later review; it is not a refactoring proposal.

End by asking the user to approve or correct the partition. Do not continue into a detailed system review until they do.
