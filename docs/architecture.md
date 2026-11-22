# Architecture

AMH is a local policy layer for existing agents. Users control policy contents,
scope and lifecycle. Built-in methods use the same policy format as user-authored
principles.

## Storage

The AMH home holds Git work-tree registrations and two kinds of event registry.
One registry is common to the user's registered work trees. Each project ID has
another registry that can be shared by explicitly registered roots. The CLI
replays history to derive current policy and serializes writes under a home lock
using atomic file replacement. Registration and policy writes leave the work
tree alone. See the [data contracts](contracts.md).

## Policy application

The host provides the current request and conversational context to its
existing agent. AMH lists the conditional policies available to the current
work tree. The agent determines relevance and may read an individual policy.
AMH validates the submitted selection, combines it with unconditional policies
and delivers the selected policy text. Stale catalogues and unresolved declared
relationships cause an explicit error. See [agent integration](agent-requests.md).

Registration defines where personal AMH policies are available. Shared
repository rules and execution permissions come from the repository and host.
The response identifies which policy text was returned. Assessing appropriateness and
actual compliance requires evidence from the work itself.
