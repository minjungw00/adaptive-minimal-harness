# Natural-language requests and agent discovery

Users describe their work in ordinary language. The current agent interprets
that request against AMH's policy catalogue; AMH validates the resulting decisions,
resolves dependencies/conflicts, and renders only applicable principles. Users do
not need to supply task tags or choose policy IDs.

## Existing agent: request-based policy lookup

After `amh init --id PROJECT --apply`, the managed block in `AGENTS.md` directs
the current agent to:

1. Identify the task from the request and available conversation. Stored work
   alone does not select the current task. Ask when ambiguity changes the work.
2. Run `amh policy discover --text 'VERBATIM REQUEST'`. The catalogue lists
   conditions, activation metadata and versions for active conditional user and
   project principles. If a condition is missing or insufficient, run
   `amh policy read --id ID --catalog-digest DIGEST` for that policy only.
3. Assess each candidate as `match`, `no_match` or `uncertain`. Use the
   `response_schema` from discovery and submit the analysis object itself
   to `amh policy resolve --input -` via stdin. The tool returns the applicable
   instructions, including unconditional policies, as JSON on stdout. A
   clarification response exits unsuccessfully with structured questions on stderr.
4. Do the work with the returned instructions. During the same work, use
   relevant instructions still in context. Discover again for a changed request
   or catalogue, or when those instructions are missing. The next request needs
   no earlier AMH response, conversation handle, or local state file.

The agent provides its applicability decisions and exact request quotes.
AMH verifies the policy snapshot, candidate coverage and structural guards.
The host owns conversation continuity and decides what remains in model input.
When the intended work is no longer clear, ask for the missing information.

To upgrade a recognized managed block, rerun the original
`amh init --id PROJECT [--user-root PATH] --apply`. Existing policy files,
explicit Tasks and user-authored text around the managed block are preserved.
Reload the agent's project instructions after updating the block.
Old `.amh/sessions/` folders are left in place as historical user data and may be
archived or removed by the user. The current commands do not consult them.

`amh request` / `amh request finish` and the one-shot agents below are optional
ways to author explicit Task contracts with completion criteria.
Explicit Tasks can also be compiled, synced and verified directly.

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

A principle can include a user-authored `applicability` condition with
exclusions. See the [custom example](../examples/p0/natural-policy.toml).
The agent interprets it in the context of the current request, including when
activation is `always`. For older policies that only declare tags, the agent
can use the labels and read the policy body as needed. `explicit_only` requires
a user request for the method. Explicit manual Tasks can still use their tags.

Both natural-language paths require a reason and exact nonempty request quotes
for each candidate ID and version. AMH validates these quotes and the current
catalogue binding, rejects missing or stale judgments, and checks mandatory
uncertainty, dependencies and conflicts. This establishes traceability and
structural consistency. Semantic accuracy and actual compliance need separate
evidence.

The explicit Task packet includes applicable unconditional policies and policy
bodies for the one-shot interpreter. It also requires completion criteria and
binds the interpreted Task to the policy catalogue and original request.
Changed policy inputs require reinterpreting or recompiling the contract.

## Verification scope

Automated tests exercise the read-only CLI handoff, scope composition, policy
body lookup, stale and incomplete decisions, mandatory uncertainty, dependencies,
questions, and upgrading the managed block. The explicit Task and one-shot
interpreter tests continue to cover deployment and verification separately.

Use the [qualitative evaluation tools](../evals/README.md) to examine actual
agent selection, outputs and observable execution. Earlier pilot results report
their own protocol and evidence limits.
