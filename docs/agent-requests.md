# Natural-language requests and agent discovery

Users describe their work in ordinary language. The current agent interprets
that request against AMH's policy catalogue; AMH validates the resulting decisions,
resolves dependencies/conflicts, and renders only applicable principles. Users do
not need to supply task tags or choose policy IDs.

## Existing agent: conversation sessions

After `amh init --id PROJECT --apply` and installation of desired packs,
the AGENTS.md bridge directs the current agent to:

1. Open one AMH session with `amh session open --apply` and retain its explicit
   handle in this conversation. A new conversation does not select the project's
   latest session or old deployed Task automatically.
2. Obtain a packet with `amh session prepare --id ID --text TEXT`. The current
   request, policy catalogue and typed session context inform applicability.
3. Interpret all relevant conditions. Initially, after policy/focus/context changes,
   with `--refresh`, or without both matching prior state/catalogue digests, provide full conditional coverage. Otherwise reuse every
   existing judgment or provide changed assessments. Reconsider previously
   excluded principles as well; this is semantic interpretation, not keyword filtering.
4. Send `{ "packet": PACKET, "response": RESPONSE }` over stdin to
   `amh session finish --input - --apply`. Read returned instructions and active/removed
   IDs, then perform the actual requested work. No separate request/packet/response
   files, Task ID, or completion criteria are required. The response_schema is the
   inner response; finish expects the outer packet/response envelope. `analysis`
   is also accepted as an alias for `response`, but not alongside it.

User messages and protocol values are handled by the existing agent, not user
forms. The bridge is an instruction integration; it does not intercept arbitrary
chats, detect every platform reset, or prove the agent followed it.

When the prior context is still available, `--known-catalog DIGEST` and
`--known-state DIGEST` avoid retransmitting the same catalogue/context bodies.
Those values come from the previous packet/output. If context was lost, compacted
or resumed with uncertain continuity, use `--refresh`; cached applicability must
be reviewed again. Even reuse checks current policies and deterministic guards.
It remains an agent judgment that earlier decisions fit the new request.

Each conversation stores one `.amh/sessions/ID/state.json`. Instructions are emitted
on stdout, avoiding a shared per-conversation instruction file. Unchanged reuse
leaves the file, revision and modification time unchanged. Context changes are
bounded; oversized content fails rather than silently truncating accepted conditions.
Notes distinguish verbatim user quotations, agent inferences, and unresolved agent questions. A valid clarification submitted with --apply retains its context while returning clarification_required and blocking the actual work; it does not activate a new policy set. A later answer can explicitly retire the corresponding question note. Recording a returned question does not prove that a human received it. Neither the
metadata nor the filesystem state authenticates a human approval.

`amh session inspect --id ID` reads state and freshness without selecting a different
session. `amh doctor` continues to diagnose the optional shared Task adapter;
`uncompiled` there does not mean a session requires a Task.

### Optional explicit work and verification

`amh session attach --id ID --task FILE --expected-revision N --apply` registers a
saved Task as information without selecting it. Several Tasks can be attached.
Select one using prepare `--task-id TASK_ID` only when contextual grounds establish
that scope; `--clear-focus` changes to an unrelated request. A single saved Task is
not proof of current intent or unfinished status. Task-scoped policies retain Task
identities, not session identities. Changed attached sources require explicit refresh
through attachment before reuse.

When an explicitly attached and focused Task has trusted caller-selected checks,
`amh session verify --id ID` previews them and `--run` executes them. The record
binds the session snapshot and checks its freshness afterwards. Natural analysis
responses cannot inject executable checks. Execution remains ordinary local tool
execution, not a sandbox or behavioral attestation.

Closing with `amh session close --id ID --expected-revision N --apply` prevents
further turns and does not declare all work complete. State and evidence remain.
A reviewed `amh.session-plan.v1` from a mutation without `--apply` can be applied
with `amh session apply --input -` or `--input FILE`; plans are recomputed and
stale/tampered inputs are rejected. Single-file state writes are atomically replaced;
there is no multi-file session transaction to recover. Platform/power-loss durability
and hostile filesystem peers are not guaranteed.

### Existing projects and explicit Task compatibility

Run the original `amh init --id PROJECT [--user-root PATH] --apply` again to upgrade
recognized previous AMH bridges while preserving user text. Explicit sync also
upgrades the bridge. The first shared generated Task file is never treated as an
implicit current conversational goal.

The existing `amh request` / `amh request finish` commands remain an optional
explicit-Task protocol. They require Task completion criteria and use the v1
request/analysis schemas described below. They do not implement the new session
protocol or automatically resume an existing conversation.

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
semantic evaluation is needed beyond protocol validation. The
[qualitative pilot](../evals/results/2026-11-17-pilot/report.md) records eight
fresh-session executions and two independent reviews per case; it does not
establish broad selection accuracy.

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

## Qualitative evidence and limits

The earlier single-turn pilot found that a saved Task was absent from a new request
packet and the actor asked for the intended work. The old record does not establish
that clarification was unnecessary: stored work alone does not select current intent.
See the [corrected pilot interpretation](../evals/results/2026-11-17-pilot/report.md).

The session workflow is evaluated with successive messages in the same fresh actor
session, plus independent two-stage reviewers. AMH calls including stdin are recorded;
commands voluntarily passed through the work recorder include before/after file
snapshots and real stdout/stderr. This is not access to a complete provider session
transcript. Actual process conclusions must name the observed evidence and its limits.
