---
status: accepted
date: 2026-10-31
---

# OpenCode chats share one server per app

All of Sirio's OpenCode chats talk to a single `opencode serve` that Sirio
starts. They do not get one process per chat, which is the rule for every
other native transport (ADR 0001). OpenCode 2.x's server is built to hold many
directories at once. Sixty of its operations take a `location[directory]`.
Every session is bound to a location. Locations load on first use and can be
evicted. `/api/event` streams every location's events. Each worktree still gets
its own configuration, skills and MCP servers inside the one process, so a
process per chat would only multiply servers that are each designed to be shared.

The server's life follows its chats. It starts with the first OpenCode chat and
stops when the last one closes, or when Sirio quits, so no OpenCode process
runs without an OpenCode chat. It listens on 127.0.0.1 behind a password that
Sirio picks at random for each start. It uses the user's own OpenCode data and
configuration, so sessions are shared with OpenCode's own interface, as
Claude's are. Each session's shell commands get the environment their worktree
would give a process of its own, through `PUT /api/session/{id}/environment`,
which the server's spec scopes to a session's local shell commands. The MCP
servers and plugins the server starts for a location still inherit the server's
own environment.

When the server dies, every OpenCode chat shows the error and none of them
falls back to ACP behind the user's back. The next action in any of them
restarts the server once. Each chat then re-reads its session: its messages
and its pending permissions and forms. The turn that was running is lost,
because it ran inside the server. Sirio never calls `location.reload`, which
cancels the pending permissions of every chat at once.

## Considered options

- **One server per chat.** Isolation for free, and the same lifetime as every
  other transport. But it runs N servers for a design meant to be shared, and
  each of them loads every configuration again.
- **Attach to the user's `opencode service`.** No process to manage. But Sirio
  would control neither its version nor its lifetime.

## Consequences

- One process failure drops every OpenCode chat at once, not just one tab.
- `/api/event` is volatile by contract: a slow reader loses the stream, and
  events sent while it is down are gone. It is read by a single reader that
  never blocks and routes events to chats by session. Each chat recovers from
  its session's durable log (`session.log`, read from a cursor). If the live
  probe shows that log cannot be relied on, the chat recovers by re-reading the
  session instead.
- The transport needs an app-level owner that counts the chats using it. Every
  other transport is owned by its tab alone.
