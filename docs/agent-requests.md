# Agent integration

Run `amh agent instructions` as a user when setting up the integration. Copy the
output into the host agent's user-level instructions, such as
`~/.codex/AGENTS.md`, `~/.claude/CLAUDE.md`, or `~/.gemini/GEMINI.md`.
Register the Git work trees where you want the policies available.

1. The agent identifies the intended work from the user's request, available
   conversation, and project evidence. It asks the user directly when a
   consequential ambiguity remains.
2. The agent calls `amh policy discover`. It proceeds with shared project
   instructions when AMH reports `not_registered`. If AMH returns `resolved`,
   it can use the returned instructions immediately.
3. If AMH reports `ready`, the agent assesses the candidate summaries. It can
   read `amh policy read ID --catalog-digest DIGEST` when a condition needs
   elaboration. An `explicit_only` method requires an explicit request, and
   activation tags can help interpret the intent of older policy definitions.
4. Send the judgements to `amh policy read --selection` on stdin. Report
   `match`, `no_match`, or `uncertain` for every `must` candidate. Optional
   candidates may be omitted and are included only on `match`.

Example selection:

```json
{
  "catalog_digest": "DIGEST_FROM_DISCOVER",
  "decisions": [
    {"id": "P-API", "applicability": "match"},
    {"id": "P-DEBUG", "applicability": "no_match"}
  ]
}
```

AMH checks the catalogue, mandatory decisions, declared dependencies and declared
conflicts before returning `selected` and `instructions`. The response includes
the active unconditional policies. If the catalogue changed, repeat discovery.
Resolve mandatory uncertainty and conflicts before relying on the output.

Request text and conversation state remain with the host. AMH can check whether
a required decision was supplied. The agent assesses whether a decision is
correct and applies the returned policies. Qualitative evaluation uses observed
calls and results to assess that work.
