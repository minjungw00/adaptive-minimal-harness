# Natural-language requests and agent discovery

Users describe their work in ordinary language. The current agent interprets
that request against AMH's policy catalogue; AMH validates the resulting decisions,
resolves dependencies/conflicts, and renders only applicable principles. Users do
not need to supply task tags or choose policy IDs.

## User-level agent integration

Run `amh agent instructions` to print guidance for your existing agent. Add it
to your user-level instructions, such as `~/.codex/AGENTS.md`,
`~/.claude/CLAUDE.md` or `~/.gemini/GEMINI.md`. Register each intended Git
work tree with `amh project add --id ID --apply`.

1. Identify the task from the request and available conversation. Ask when
   ambiguity changes the work.
2. Run `amh policy discover --text 'VERBATIM REQUEST'`. For a registered work
   tree AMH returns `ready` and candidate conditions. Use the ordinary
   project workflow if the result is `not_registered`.
3. Assess each condition. If necessary, read one policy with
   `amh policy read --id ID --catalog-digest DIGEST`. Submit the object matching
   `response_schema` to `amh policy resolve --input -` via stdin. Resolve returns
   applicable instructions on stdout. Clarification returns structured
   questions on stderr with a nonzero exit status.
4. Use the selected instructions to do the work. Rediscover when the task,
   catalogue or available context changes. The host manages conversation
   continuity.

The agent interprets the request and provides exact request quotes. AMH checks
the work-tree binding, policy snapshot and structural guards. User-level agent
instructions are installed manually and can be used without any AMH-owned file
in the project.

## One-shot interpretation with an explicitly selected agent

An existing Codex CLI can do the interpretation in one invocation:

```sh
amh request --text 'Fix the parser bug without breaking the existing public API.' --agent codex
```

This interprets a Task contract and prints its JSON. The current agent performs
the underlying work. Use `amh request finish --packet FILE --analysis FILE` to
produce the same Task format with a response from the current agent.

The adapter invokes `codex exec` with `--sandbox read-only`, `--ephemeral`,
`--skip-git-repo-check`, `--output-schema`, and `--output-last-message`, with a
prompt on stdin. It runs from a private temporary directory under AMH home instead of recursively
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
questions, and registered work trees. The explicit Task and one-shot interpreter
tests cover compilation and verification separately.

Use the [qualitative evaluation tools](../evals/README.md) to examine actual
agent selection, outputs and observable execution. Earlier pilot results report
their own protocol and evidence limits.
