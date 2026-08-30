---
name: architecture-synthesis
description: Synthesize approved logical-system reviews into a prioritized, behavior-preserving architecture and maintainability roadmap.
---

# Architecture Synthesis

Use this only after the approved systems have been reviewed. Read `review-standards/SKILL.md` first. Treat the partition and system reports as evidence, not as unquestionable conclusions.

## Procedure

Reconcile findings across systems and verify high-impact claims against the repository where needed. Focus on interactions that isolated reviews cannot see: cross-system coupling, cycles, shared state, duplicated policies, inconsistent conventions, boundary violations, and changes that would spread across many systems.

Assess:

- The actual architecture as implemented, including strongest and weakest boundaries.
- Which systems are genuinely independent and which are only conceptually separate.
- The highest-leverage maintainability problems.
- Which recommendations reinforce one another and which conflict.
- Sequencing constraints, migration risks, and behavior-verification needs.
- Recommendations that should not be made because they add abstraction or churn without enough benefit.
- Missing evidence that prevents a confident conclusion.

## Required output

Provide:

1. A concise architecture summary and dependency view.
2. Cross-system findings, each with evidence, impact, recommendation, behavior-preservation analysis, confidence, and priority.
3. A prioritized roadmap of small, incremental, behavior-preserving changes. Explain dependencies between roadmap items and how to verify behavior after each step.
4. Explicit recommendations in four categories:
   - **Must address:** likely recurring defects or disproportionate extension cost.
   - **Should address:** worthwhile structural or maintainability improvements.
   - **Could address:** limited-risk cleanup with smaller payoff.
   - **Do not change:** imperfect-looking code with a valid reason or insufficient benefit to disturb.
5. A short list of unresolved questions and evidence gaps.

Do not provide implementation patches unless separately requested. Do not silently expand the scope into performance tuning, security auditing, feature design, dependency upgrades, or behavior changes. Mention those only when directly relevant to an architectural conclusion and label them outside scope.
