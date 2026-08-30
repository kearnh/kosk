# Architecture and Maintainability Review

You are reviewing this repository as a senior software architect and maintainer. The codebase has grown incrementally, often with AI assistance. The goal is to assess whether its architecture, boundaries, abstractions, and coding practices will remain understandable and easy to extend.

This is a **read-only review**. Do not edit files, reformat code, change configuration, add tests, rename symbols, or propose functionality changes as if they were required fixes. Recommendations may affect structure, ownership, dependencies, or maintainability, but must preserve observable behavior unless you explicitly identify a separate, optional behavior concern.

Do not assume the existing directory structure is the correct architecture. Do not assume existing documentation is accurate. Verify claims against the implementation, call sites, runtime flow, tests, and configuration.

## Review method

Work in explicit phases. Do not attempt to review the entire repository in one undifferentiated pass.

### Phase 0: Reconnaissance

Inspect enough of the repository to establish:

- How the program starts and what its important runtime flows are.
- The major domain concepts, state holders, threads/tasks, external interfaces, and persistence/configuration paths.
- Existing tests, examples, binaries, documentation, build scripts, and platform-specific code.
- The language/toolchain and the conventions the repository already follows.

Report facts separately from interpretations. Cite every material finding with exact file paths and symbols, and line numbers when practical.

### Phase 1: Divide the project into logical systems

Before making detailed recommendations, create a mental model of the project and divide it into the smallest useful set of **largely independent logical systems**. This is a conceptual partition, not a proposed file move and not necessarily a one-to-one match with modules or directories.

For each system, describe:

- Its purpose and responsibilities.
- The important state and invariants it owns.
- Its inputs, outputs, and externally visible effects.
- The public or semi-public interfaces through which other systems interact with it.
- Its direct dependencies and dependants.
- Which parts are implementation details versus architectural boundaries.
- Why it is a separate system and why it is grouped with nearby code.

Also identify:

- Cross-cutting concerns that should not be incorrectly assigned to one system.
- Shared state, cycles, and dependency paths that make systems less independent.
- Glue/orchestration code versus domain logic.
- Areas too entangled to review in isolation.
- A sensible review order based on dependencies and risk.

Represent the result as a table plus a compact dependency diagram or dependency list. Do not recommend changes yet. End Phase 1 and wait for my approval or corrections to the partition.

### Phase 2: Review one system at a time

After I approve the partition, review exactly one system per step unless I ask for a batch. Keep each review focused on that system, while recording issues that clearly belong to cross-system synthesis later.

For each system, examine:

1. **Responsibility and cohesion**
   - Does the system have a clear purpose?
   - Are unrelated responsibilities mixed together?
   - Are there responsibilities that belong elsewhere?

2. **Boundaries and dependencies**
   - Are dependencies flowing in a comprehensible direction?
   - Does the system depend on concrete low-level details unnecessarily?
   - Are there cycles, dependency leaks, or calls that bypass the intended layer?
   - Are interfaces defined at the right abstraction level?

3. **State and ownership**
   - Who owns each important piece of state?
   - Are lifetime, mutability, synchronization, and initialization rules clear?
   - Are global/singleton/session states justified, or do they create hidden coupling?
   - Are invalid states representable when they could be prevented?

4. **Data and control flow**
   - Are transformations and side effects located at sensible boundaries?
   - Is orchestration separated from policy and mechanism?
   - Are there needless conversions, copies, allocations, locks, callbacks, or repeated computations?
   - Are error, cancellation, retry, and shutdown paths coherent where applicable?

5. **Abstractions and APIs**
   - Are helpers and types earning their complexity?
   - Are there unnecessary wrappers, indirection, generic abstractions, or pass-through functions?
   - Are there missing abstractions that would make an existing boundary clearer?
   - Are names and visibility communicating the actual contract?
   - Do similar-looking operations genuinely share semantics, or should they remain distinct?

6. **Duplication and consistency**
   - Identify meaningful duplicated logic, not merely similar syntax.
   - Distinguish accidental duplication from deliberate domain-specific differences.
   - Look for inconsistent error handling, lifecycle handling, validation, naming, or state transitions.

7. **Extensibility and testability**
   - What would be difficult to add, replace, or test?
   - Which likely future changes would force edits across too many files or layers?
   - Are tests located at useful behavioral boundaries?
   - Are platform, time, I/O, hardware, UI, and concurrency concerns isolated enough to test?

8. **Code quality**
   - Point out smaller, concrete issues such as needless helpers, awkward control flow, repeated code, misleading names, stale comments, dead paths, or overly broad modules.
   - Avoid style preferences that do not affect comprehension, correctness, change safety, or maintenance cost.

For every finding, use this format:

- **Finding:** one precise sentence.
- **Evidence:** exact files/symbols and the relevant flow or call sites.
- **Impact:** what becomes harder, riskier, slower, or more error-prone.
- **Recommendation:** the smallest structural improvement that addresses it.
- **Behavior:** explain why the recommendation preserves current behavior, or label it as an optional behavior change.
- **Confidence:** high, medium, or low, with the reason for uncertainty.
- **Priority:** critical, high, medium, or low, based on maintenance value and risk rather than personal preference.

Do not prescribe a rewrite merely because another architecture is fashionable. Prefer incremental, local improvements. Do not recommend combining code solely because it looks similar, and do not recommend splitting code solely because a file is large. Explain the boundary or invariant that justifies each recommendation.

At the end of each system review, include:

- The system's current strengths worth preserving.
- A short list of recommended improvements, ordered by value.
- A list of issues deliberately deferred to cross-system synthesis.
- The smallest safe sequence in which the improvements could be made, without implementing them.

Then stop and wait for my instruction before reviewing the next system.

### Phase 3: Cross-system synthesis

After all approved systems have been reviewed, produce a synthesis covering:

- The actual architecture as implemented, including its strongest and weakest boundaries.
- Cross-system coupling, cycles, shared state, duplicated policies, and inconsistent conventions.
- The highest-leverage maintainability problems.
- Which recommendations reinforce each other and which conflict.
- A prioritized, incremental roadmap grouped into small, behavior-preserving changes.
- Migration risks, sequencing constraints, and how to verify behavior after each change.
- Recommendations that should explicitly **not** be made because they add abstraction or churn without enough benefit.
- Missing evidence that prevents a confident conclusion.

Separate recommendations into:

1. **Must address:** likely to cause recurring defects or make normal extension disproportionately difficult.
2. **Should address:** worthwhile structural or maintainability improvements.
3. **Could address:** small cleanup with limited risk or payoff.
4. **Do not change:** code that may look imperfect but has a valid reason or insufficient benefit to disturb.

Do not provide implementation patches unless I separately request one. Do not silently expand the scope into performance tuning, security auditing, feature design, dependency upgrades, or behavior changes. Mention those only when they directly affect an architectural conclusion, and label them as outside scope.

## Review standards

- Evidence before conclusions.
- Facts, interpretations, and recommendations clearly separated.
- Preserve existing behavior and domain distinctions.
- Prefer simple boundaries and explicit ownership over clever abstractions.
- Treat public API or visibility changes as significant and justify them.
- Consider runtime behavior, not only static file layout.
- Account for platform boundaries, concurrency, persistence, configuration, error paths, and shutdown where they exist.
- Never claim a helper is unnecessary without checking all callers and its semantic role.
- Never claim code is duplicated without checking whether the duplicated paths have different invariants or future reasons to diverge.
- Never report a concern without explaining the maintenance consequence.
- If evidence is insufficient, say what would need to be inspected instead of guessing.

Begin with Phase 0 and Phase 1 only. Do not edit the repository. End by showing the proposed logical-system partition and asking me to approve or correct it before detailed reviews begin.
