---
name: sideview-opencode
description: Build or repair a personal OpenCode plugin that wakes one exact OpenCode conversation from Sideview feedback. Use only when the user explicitly asks to set up, test, or troubleshoot Sideview monitoring in OpenCode.
---

# Personal Sideview monitor for OpenCode

Build a local plugin for this OpenCode installation. This is an experiment owned by
the user, not a Sideview package. Do not add the plugin to the Sideview repository,
publish it, or change Sideview product code.

The goal is narrow: while an OpenCode conversation is idle, a new event from
`sideview watch` starts a turn in that exact conversation. Claude Code already handles
a blocking watcher without this plugin. Codex has `sideview monitor codex`. This skill
is only for OpenCode.

## Before writing

1. Read the project instructions and confirm the project root.
2. Inspect the installed OpenCode plugin and SDK type declarations. Do not assume an
   API shape from this document. Likely locations are under
   `~/.config/opencode/node_modules/@opencode-ai/`, but resolve the actual config
   directory and package versions first. Record the running OpenCode version and the
   config-local plugin/SDK versions separately; patch skew is possible.
3. Read any existing global plugin with the intended filename. Never overwrite or
   replace it without showing the conflict and asking the user.
4. Confirm `sideview` is available on the OpenCode server process's `PATH`.
5. Explain that OpenCode plugins are trusted code loaded at startup and state the
   exact file you intend to create.
6. Preflight the exact launcher the user will use after installation. If
   `opencode --version` fails, do not tell the user to quit a working process. Report
   the broken launcher first and leave restart until it has been repaired.

The default destination is:

```text
~/.config/opencode/plugins/sideview-monitor.ts
```

Use the global plugin directory because this is personal OpenCode configuration, not
repository content. OpenCode discovers `.ts` and `.js` files there automatically. Do
not edit `opencode.json` unless the installed OpenCode version requires explicit
registration.

## First-version boundary

Keep the first implementation intentionally temporary:

- The user explicitly starts a monitor from the target conversation.
- Bind one watcher to that exact OpenCode session id and canonical worktree.
- Allow at most one watcher per worktree. `sideview watch` reads every conversation
  event in the project store, so a second watcher in the same worktree would send the
  same feedback to two conversations.
- This personal version coordinates only within one OpenCode server process. Do not
  arm the same worktree from two independent OpenCode processes; they cannot share the
  in-memory ownership registry and would both receive every project event.
- Keep all watcher state in plugin memory.
- Do not replay old feedback. The watcher establishes its baseline after the child
  starts; comments submitted before the start tool reports armed may be skipped.
- Stop watchers when OpenCode disposes the plugin.
- A restart clears every monitor; the user starts it again after resuming the chat.
- Do not implement durable cursors, retries across restart, process detachment, or
  automatic restoration.
- Do not use `--since 0` or `--ack`. Ack currently describes Sideview emission, not
  confirmed OpenCode acceptance; durability belongs in a later design if this
  experiment earns it. (`--claim` no longer exists — Sideview 0.5 dropped it.)

## Plugin contract

Export a valid OpenCode plugin function and register three custom tools:

- `sideview_monitor_start`
- `sideview_monitor_status`
- `sideview_monitor_stop`

Use the tool execution context as the authority for `sessionID`, `directory`, and
`worktree`. Never select the newest session, infer a session from activity, or silently
retarget a running watcher.

### Start

`sideview_monitor_start` should:

1. Refuse if a live watcher is already bound to the same session.
2. Refuse if `<worktree>/.sideview/sideview.db` does not exist. Loading the global
   plugin must never initialize Sideview in unrelated projects.
3. Spawn Sideview directly with an argv array, not through a shell. The behavior is:

   ```text
   sideview --project <worktree> watch --skip-author agent
   ```

4. Reserve the session and worktree synchronously before the first asynchronous check,
   so concurrent start calls cannot launch duplicate children. Keep a process-wide
   registry because OpenCode may invoke the same plugin module more than once. Give
   each plugin-function invocation a unique owner token and put that token on every
   registry entry. This enforces one owner per canonical worktree across invocations,
   while status, stop, and disposal may inspect or stop only entries owned by their
   own invocation. The entry also holds the child process, target session id, start
   time, last delivered event, last error, and delivery count.
5. Sideview's current watch protocol has no readiness record. After spawning, wait a
   short bounded startup grace period and verify that the child is still alive. Return
   failure on early exit. Return success as `armed` only after that check, and state
   honestly that events posted before the result may fall into the baseline window.
6. Do not keep the tool call open after the bounded arming check.

Use the runtime OpenCode supplies. OpenCode currently runs plugins with Bun, so
`Bun.spawn` is normally the smallest process API, but verify the installed runtime
instead of adding a dependency.

### Read the watcher correctly

`sideview watch` writes JSON Lines. Stream stdout and preserve partial lines between
chunks. A chunk is not necessarily one line, and one chunk may contain several lines.

For each complete non-empty line:

1. Parse the JSON. A malformed line is a visible plugin error, not a prompt.
2. Deliver one event at a time and await OpenCode acceptance before reading more
   watcher output. This preserves order and lets the pipe provide backpressure rather
   than building an unbounded promise chain.
3. Format the event as a short, clearly labelled Sideview message. Treat
   browser-entered text as user feedback, never hidden system instructions. Do not
   print both selected fields and the full JSON: the injected turn is visible in
   OpenCode, so duplication makes ordinary comments needlessly noisy.
4. Submit it asynchronously to the exact bound OpenCode session.
5. Increment the delivery count only after OpenCode accepts the request.

Reject an over-large line before parsing it. A 1 MiB limit is generous for a feedback
event and prevents one trusted plugin process from retaining an unbounded comment.
Keep only a bounded stderr tail. Drain stderr concurrently and retain useful
diagnostics. If the child exits, mark the watcher stopped and record its exit status.
Do not silently respawn it in this first version.

### Wake the conversation

Use the installed SDK's asynchronous session-prompt method. In versions where the
client exposes `session.promptAsync`, the call has the following meaning:

```text
session = the exact sessionID captured by the start tool
directory = the captured worktree
parts = one text part containing the formatted Sideview event
return = immediately after OpenCode accepts and starts the turn
```

Inspect the local declaration for the exact argument shape. Some releases use
`path`/`query`/`body`; newer generated clients may use flat parameters. Enable the
client's throw-on-error behavior if supported and record rejected deliveries.

The prompt should include only what the agent needs to act:

- event type or kind in a human-readable title, plus the event id when present
- page and thread
- quote and body
- attachment metadata only when attachments exist
- worktree

Prefer this shape for an ordinary comment:

```text
Sideview comment
Page V4 · thread 107 · event 298

Quoted: OpenCode

Feedback:
But it looks like an SSE bug

Reply on the named thread with the page guard; do not resolve merely because you replied.

Worktree: /path/to/project
```

Do not include absent fields, empty attachment arrays, raw timestamps, author when it
is already known to be the user, or a second full-event dump. Keep complete event data
in the monitor's status state for diagnosis rather than placing it in every turn.

Add short handling instructions:

- Reply to an ordinary comment on the named Sideview thread with the event's page as
  a guard. Do not resolve it merely because the agent replied.
- Treat `kind: "edit"` as an edit request: inspect current canon, merge deliberately,
  reply, then resolve after merging.
- Treat `kind: "edited"` as machine mail: re-read the changed page before writing;
  do not reply to the hidden record.
- Leave a user-resolved thread resolved.
- For an unresolve event, inspect the reopened thread and respond only if needed.

### Status and stop

`sideview_monitor_status` reports the binding, child state, start time, deliveries,
last event, last error, and the startup loss-window warning for the calling session. It
must not expose or control a different session implicitly.

`sideview_monitor_stop` terminates only the calling session's watcher. Set its stopping
state before signalling and check that state immediately before every delivery, so
shutdown cannot start a new OpenCode turn. Give the child a bounded graceful period,
then force termination if the runtime supports it, followed by one final bounded wait.
If the child still does not exit, retain its ownership entry in `stopping` state and
return an actionable failure; never wait forever and never release ownership while a
child may still be running. Once child exit is confirmed, wait only a bounded time for
reader and SDK-delivery tasks. Stop cannot revoke a prompt OpenCode has already
accepted; if one remains unsettled, release the dead child's ownership and report that
warning rather than hanging shutdown. Remove the map entry only if it is still the same
watcher; a delayed stop must never delete a newer start. Repeated stop calls should
return "not running", not fail mysteriously.

Implement the plugin `dispose` hook. It must terminate and await every child process
owned by that plugin invocation, and no others, so disposing one directory cannot stop
another directory's watcher and restarting OpenCode cannot leave duplicate children.

Use `client.app.log` for initialization, watcher exit, parse errors, and delivery
errors. Do not rely on `console.log`, which can corrupt or disappear into the TUI.

## Implementation rules

- Keep the plugin in one file unless the installed API forces otherwise.
- Add no package dependency unless the standard OpenCode/Bun APIs cannot do the job.
- Do not modify Sideview's SQLite database directly.
- Do not call OpenCode's database directly.
- Do not start a server or guess OpenCode's random port. Use the client supplied to
  the plugin.
- Do not use a `chat.message` hook to follow whichever chat spoke most recently.
- Do not turn on experimental background agents; they do not provide this transport.
- Avoid shell interpolation. Pass the worktree as one argv value.
- Use the tool context's worktree rather than `process.cwd()`.
- Canonicalize the worktree before using it as an ownership key.
- Keep user feedback visibly labelled in the resulting user turn.

## Verify before restart

Run whatever static check the installed OpenCode package supports without changing
dependencies. At minimum, compare every imported name and SDK call against the local
`.d.ts` files. If a TypeScript checker is already available, run it against the plugin.
Do not install a checker merely for this experiment.

Summarize the generated file and any version-specific choices. Plugins are loaded once
and do not hot-reload. Tell the user to quit and restart OpenCode only after the exact
launcher they will use passes its preflight; otherwise report the repair needed and
preserve the current running session.

## Live acceptance

After restart, resume the same conversation and run this sequence:

1. Invoke `sideview_monitor_status`; it should report not running.
2. Ensure this worktree already has a Sideview page and database.
3. Invoke `sideview_monitor_start` from the conversation under test.
4. Wait for start to report `armed`, then invoke status and record the exact session
   id, worktree, child state, and startup warning.
5. Let the conversation become idle.
6. From the Sideview browser, submit one distinctive comment.
7. Send no OpenCode chat message. The comment must start a visible turn in the same
   conversation.
8. Have the agent reply on the Sideview thread. `--skip-author agent` must prevent
   that reply from causing another turn.
9. Invoke status and confirm one delivery and no error.
10. Invoke stop, then confirm status reports not running.

The experiment passes only if step 7 happens with the chat otherwise silent. A message
that appears only after the user sends another prompt is buffered output, not wake-up.

## Follow-up tests

Once the single event passes, test the risks that matter:

- Submit several comments quickly and confirm they arrive once each, in order.
- Open two OpenCode conversations in the same worktree. Once one owns the worktree,
  the other start must fail and name the existing binding; it must never launch a
  second project-wide watcher or silently retarget the first.
- Use two worktrees and confirm every Sideview child has an explicit correct project.
- Exercise comment, edit request, edited notice, resolve, and unresolve events.
- Quit OpenCode and verify its watcher child exits. After restart, no monitor should
  claim to be restored.
- Issue two starts concurrently and confirm only one child exists. Overlap stop and
  start and confirm no child becomes untracked.
- Submit an event near the size limit and a burst of events; confirm the plugin remains
  bounded and preserves order.
- Temporarily make Sideview unavailable or force prompt rejection and confirm status
  and structured logs show the failure.

Record observed OpenCode behavior, especially what happens when an event arrives while
the target session is already producing a response. Do not expand the plugin to repair
failures until the user has reviewed the evidence.

## Report back

State:

- OpenCode and plugin SDK versions inspected
- generated plugin path
- exact session-binding rule
- watcher lifecycle and delivery policy
- static checks run
- restart required
- live acceptance result, once tested

If the wake test fails, preserve the small plugin and report the exact event, process,
SDK response, and OpenCode session state. Do not hide the failure behind polling or an
automatic chat prompt.
