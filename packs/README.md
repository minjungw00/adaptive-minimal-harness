# Agent workflow policy pack

`agent-workflow` is the built-in pack. Installation is optional. Inspect it with
`amh pack show agent-workflow` and install it with
`amh pack install agent-workflow --apply`. Add `--scope user` to make it available
to all work trees registered in the AMH home.

The pack provides one short working baseline and twenty conditional policies.
Each conditional policy includes an applicability condition, an action, and a
stopping point where useful. The agent selects policies for the current work
and can use one piece of evidence to satisfy several policies. The pack assigns
`must` to the baseline, authorization checks, and API compatibility. Other
methods can be selected as relevant.

## Source mapping

The [principles](https://github.com/minjungw00/agent-principle-project/blob/6dbbc4cf393f1de005d718abacd5ab1fb35e6d5e/PRINCIPLES.md)
provide the reasoning behind the reusable instructions.

| Principle | Policies that put it into practice |
| --- | --- |
| 1. Fidelity to goals and context | `P-WORKING`, `P-CONTEXT-UPDATE`, `P-WRITING` |
| 2. Accuracy and honesty | `P-WORKING`, `P-VERIFY-EVIDENCE`, `P-KNOWLEDGE` |
| 3. Understanding and reviewability | `P-VERIFY-EVIDENCE`, `P-DECISION-REVIEW`, `P-REVIEW` |
| 4. User judgment and selective delegation | `P-ELICIT`, `P-DECISION-REVIEW`, `P-AUTHORITY` |
| 5. Autonomy in thinking and learning | `P-SOCRATIC`, `P-HAVRUTA`, `P-RECALL` |
| 6. Proportionate methods and intervention | `P-WORKING`, `P-ELICIT`, `P-TECH-DEBT` |
| 7. Long-term quality | `P-REFACTOR`, `P-TECH-DEBT`, `P-VERIFY-EVIDENCE` |
| 8. Authority and risk | `P-AUTHORITY` |

The [methods](https://github.com/minjungw00/agent-principle-project/blob/6dbbc4cf393f1de005d718abacd5ab1fb35e6d5e/METHODS.md)
supply the conditions and actions below.

| Method | Policy |
| --- | --- |
| Clarify requirements and verification criteria | `P-ELICIT` |
| Test-driven development | `P-TDD` |
| Refactoring | `P-REFACTOR` |
| Technical debt management | `P-TECH-DEBT` |
| Evidence-based verification | `P-VERIFY-EVIDENCE` |
| Socratic dialogue | `P-SOCRATIC` |
| Grill Me | `P-GRILL-ME` |
| Havruta | `P-HAVRUTA` |
| Self-explanation and recall | `P-RECALL` |
| Decision review | `P-DECISION-REVIEW` |
| Critical review of claims and alternatives | `P-ARGUMENT` |
| Integrating changes and checking the result | `P-CONTEXT-UPDATE` |

The pack also includes defect reproduction, regression checks, API
compatibility, implementation review, mutation testing, document drafting,
and source traceability. Tests and CI use the project's tools. The host
controls execution permissions. The reference [implementation patterns](https://github.com/minjungw00/agent-principle-project/blob/6dbbc4cf393f1de005d718abacd5ab1fb35e6d5e/IMPLEMENTATION.md)
and [profiles](https://github.com/minjungw00/agent-principle-project/tree/6dbbc4cf393f1de005d718abacd5ab1fb35e6d5e/profiles)
inform how to apply the guidance in each environment.

## Custom packs and provenance

A pack file has an `id`, a `version`, and a `policies` array. Install one with
`amh pack install --file PACK.toml`. Installation preserves explicit policy
sources, and the registry tracks the installed definitions, pack ID, version,
and content digest. Use `amh policy list --full` to inspect sources and the
pack fingerprint.
