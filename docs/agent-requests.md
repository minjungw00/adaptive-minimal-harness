# Natural-language requests and agent discovery

Users describe their work in ordinary language. The current agent interprets
that request against AMH's policy catalogue; AMH validates the resulting decisions,
resolves dependencies/conflicts, and renders only applicable principles. Users do
not need to supply task tags or choose policy IDs.

## Existing agent: no second model process

After `amh init --id PROJECT --apply` and installation of any desired packs,
AGENTS.md tells an existing compatible agent to perform the following workflow
before substantive user work:

1. Preserve the user's verbatim request in a temporary UTF-8 file.
2. Run `amh request --request-file REQUEST.txt` and save its JSON discovery packet.
3. As the current agent, read the conditions and produce the packet's structured
   response. No second agent or provider is necessary.
4. Run `amh request finish --packet PACKET.json --analysis RESPONSE.json --apply`.
5. Read `.amh/generated/AGENTS.md` and continue the original user task. Preparing
   or deploying a contract is not completion of the actual work.

Packet/response files are protocol details handled by the agent, not forms that
users have to author. The adapter is an instruction integration, so the existing
agent must read AGENTS.md and have access to the native CLI. It does not forcibly
intercept arbitrary chats or prove that an agent followed its instructions.

Existing projects initialized by the earlier P0 implementation can run their
original `amh init --id PROJECT [--user-root PATH] --apply` again, or sync a task.
Only the recognized previous AMH block is upgraded; user text remains intact.
`doctor` reports `adapter_update_available` for a recognized older bridge.

## One-shot interpretation with an explicitly selected agent

An existing Codex CLI can do the interpretation in one invocation:

```sh
amh request --text 'Fix the parser bug without breaking the existing public API.' --agent codex --apply
```

This interprets and deploys a task contract. It does not ask the interpreter to
perform the underlying code change; the execution agent then uses that contract.
Without `--apply`, it produces a reviewable deployment plan.

The adapter invokes `codex exec` with `--sandbox read-only`, `--ephemeral`,
`--skip-git-repo-check`, `--output-schema`, and `--output-last-message`, with a
prompt on stdin. It runs from a private temporary directory instead of recursively
loading the consumer project's agent workflow. It uses the existing CLI's account,
model and provider configuration. AMH neither provisions a key nor silently starts
or authenticates a provider. See the official [non-interactive documentation](https://learn.chatgpt.com/docs/non-interactive-mode)
and [CLI reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli).

Other agents can implement the same JSON stdin/stdout protocol. Select an explicit
configuration file with `--agent-config CONFIG.json`:

```json
{
  "adapter": "stdio",
  "argv": ["/absolute/path/to/existing-agent-adapter"],
  "timeout_seconds": 120
}
```

The executable receives a discovery packet on stdin and must return exactly one
AgentResponse JSON object on stdout. Use absolute paths for file arguments;
PATH-resolved executable names are also supported. The external command is
explicitly trusted by the caller; generic adapters are not OS-sandboxed by AMH.
Timeouts kill the direct process, not an entire descendant process tree. Output
is bounded to 4 MiB per stream. Agent credentials, operating costs and data handling
belong to the selected provider. There is no automatically discovered executable,
mandatory cloud account, keyword classifier, retry loop or new agent runtime.

## Natural conditions and response validation

A principle can include a human-authored `applicability` condition, including
exclusions. See [the custom example](../examples/p0/natural-policy.toml).
This supports unfamiliar/custom methods without adding new core modes or task tags.
Existing tag-based policies are discoverable too: their activation labels and
policy text are presented to the agent for semantic interpretation, not matched
against raw request keywords. Explicit manual Task files remain supported.

A natural condition restricts a principle's activation. With `activation.mode =
"always"`, interpretation is required to determine whether that condition holds.
For existing tag policies, explicitly classified manual tasks can still use the
configured tags. An `explicit_only` condition must be interpreted as requiring an
explicit user request for that method.

Every active conditional principle in task scope appears as a candidate card.
The packet separately contains accepted unconditional principles, goals,
requirements and constraints that the response cannot remove. Candidate cards
contain selection information, not verification histories or unrelated procedures.
There is no keyword prefilter that might hide a mandatory policy. Oversized
catalogues fail visibly rather than being silently truncated.

The agent returns:

- the exact request and catalogue fingerprints;
- one `match`, `no_match`, or `uncertain` assessment for every candidate ID/revision;
- a reason and exact nonempty quotation(s) from the original request per decision;
- inferred completion criteria and any material clarification questions.

AMH rejects missing/duplicate/unknown/stale candidates, fabricated quotations,
modified packets, changed source policies, and edited interpreted tasks. Required
uncertainty and clarification questions stop deployment. Optional uncertainty is
reported without inventing certainty. Unconditional rules, declared conflicts and
dependencies still apply after interpretation. Interpretation cannot mutate the
registry, issue approval receipts, or inject automatically executable check commands.

The generated goal preserves the original request verbatim. Acceptance criteria
and applicability are labeled agent interpretations. Exact quotations and hashes
establish traceability and snapshot consistency, **not semantic truth or human
approval**. A confidently wrong `no_match` judgment remains possible; independent
semantic evaluations and measured selection quality remain follow-up work.

Changed policies make an interpreted deployment blocked/stale until the agent
interprets the request against the new catalogue. Materially changed user scope
must start a new packet, not reuse the previous interpretation. The default task
ID derives from the request; `--task-id` allows the caller to select an established
task identity when needed.

## Verification scope

Tests cover the current-agent handoff, one-command stdio path, custom rules without
tags, retained requirements, stale/tampered/omitted assessments, uncertainty,
conflicts, process failure/timeout/output limits, old bridge migration and the
Codex argument/file protocol. Backend tests use an explicitly identified scripted
protocol fixture, not an LLM or a measured semantic classifier. They do not prove
live provider compatibility, selection accuracy or actual downstream compliance.
