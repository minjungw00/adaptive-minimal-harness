# Optional policy packs

`development.toml` and `inquiry.toml` are executable input contracts for
`amh pack install`; they are not enabled automatically. Install records the exact
policy definitions, pack ID/version and content digest in the selected registry.

Development includes TDD, refactoring, debugging, regression testing, code review,
API compatibility and optional mutation testing. Inquiry includes Socratic dialogue,
requirements elicitation, research verification, writing, argument evaluation and
knowledge organization. Each method supplies a natural-language applicability condition and a configurable MUST/SHOULD/MAY strength. Run `amh pack show NAME` to inspect every condition.

Verification guidance is included in policy text. Installing TDD does not
establish Red–Green–Refactor adherence. Installing refactoring does not prove
behavior preservation. Use relevant executed checks and independent review.

Custom packs have `id`, `version`, and a `policies` array matching the `Policy`
schema. Supply one with `amh pack install --file PACK.toml ...`. Editing installed
content uses the same version-checked policy operations as any other user policy.
No new method requires a core enum or dedicated runner.

Pack 0.2.0 adds natural-language discovery conditions. Previously installed definitions remain pinned; they are not overwritten or silently upgraded.
