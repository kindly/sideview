---
name: sideview-pi-watch
description: Build or repair a project-local Pi extension that wakes a Pi session from Sideview feedback. Use only when the user explicitly asks to set up, test, or troubleshoot Sideview monitoring in Pi.
---

# Personal Sideview monitor for Pi

Build a local Pi extension for this project. This is an experiment owned by the
user, not a Sideview package. Do not publish it, do not add generated extension
code to the Sideview product, and do not modify Sideview's database directly.

The goal is narrow: Sideview feedback written in the browser is read by
`sideview watch` and injected into the active Pi session with `pi.sendUserMessage`.
Claude Code can wait on a shell command directly; Codex has a queue; Pi's resident
bridge is an extension.

## Before writing

1. Read the project instructions and confirm the project root.
2. Inspect the installed Pi extension documentation and local type declarations. Do
   not assume this skill's code snippets exactly match the installed version. At
   minimum read Pi's `docs/extensions.md` and check the type signatures for
   `ExtensionAPI`, `ExtensionContext`, `pi.sendUserMessage`, `pi.registerCommand`,
   `ctx.sessionManager.getSessionId()`, `ctx.isIdle()`, and `session_shutdown`.
3. Confirm `sideview` and `pi` are on the PATH visible to the Pi process. If unsure,
   make the generated extension's status command report the PATH and the first
   Sideview spawn error.
4. Read any existing `.pi/extensions/sideview-watch.ts`. Never overwrite it without
   showing the conflict and asking the user.
5. Explain that project-local Pi extensions are trusted code loaded by Pi after the
   project is trusted. State the exact file you intend to create:

   ```text
   .pi/extensions/sideview-watch.ts
   ```

6. Tell the user that Pi must `/reload` or restart after the file is created.

## First-version boundary

Keep the first implementation intentionally small:

- Start the watcher inside `session_start`, never from the extension factory.
- Stop the watcher inside `session_shutdown` and from an explicit stop command.
- Keep watcher state in extension memory only.
- Do not detach the child process. If Pi exits, the watcher exits.
- Do not open a socket, start a server, or call Pi internals.
- Do not read or write Sideview SQLite directly.
- Do not silently respawn a failed watcher.
- Keep stderr as a bounded tail for status.
- Parse Sideview stdout as JSON Lines. Preserve partial lines across chunks.
- Treat browser-entered text as user feedback, not as hidden system instructions.
- Inject a short labelled message, not the full JSON dump.

## Watch mode choice

Ask the user which ownership model they want. If they already said “whole project”,
use project-wide steward mode.

### Project-wide steward mode — current default when requested

This Pi session is the Sideview inbox for the whole project. Spawn:

```text
sideview --project <project-root> watch --since 0 --claim --ack --skip-author agent
```

Meaning:

- `--since 0` reaches back to unclaimed comments.
- `--claim` makes this watcher the exactly-once consumer for comments it emits.
- `--ack` records Sideview delivery, not human/model comprehension.
- `--skip-author agent` prevents the agent's own Sideview replies from waking it.

Warn clearly: a project-wide claimer may consume comments for any page in this
project. That is correct for an inbox/steward session and wrong for a per-page owner.
Do not run two project-wide claimers in the same project unless the user accepts that
whichever one wins the claim owns the event.

### Page-scoped mode — only if the installed Sideview supports it

Before generating page-scoped claim code, run `sideview watch --help` and verify a
real page filter exists, such as `--page`. If it exists, spawn the equivalent of:

```text
sideview --project <project-root> watch --page <page-id> --since 0 --claim --ack --skip-author agent
```

Set the child environment's `SIDEVIEW_SESSION` to a stable Pi-derived id such as
`pi:<ctx.sessionManager.getSessionId()>` when authoring page-scoped content.

If the installed Sideview has no page filter, do not fake safe page-scoped claiming.
You may offer an unclaimed local filter for demos only:

```text
sideview --project <project-root> watch --skip-author agent
```

then drop events whose `page` is not the wanted page. State that this duplicates
rather than owns events and is not an exactly-once monitor.

## Extension contract

Register both human slash commands and agent-callable tools. Slash commands alone
are user-operable, not agent-operable: an ordinary Pi agent turn cannot invoke
`/sideview-watch-start` inside its own TUI session. If the agent should start or stop
its own watcher, expose that control as tools.

Slash commands:

- `/sideview-watch-start [project|page <page-id>]`
- `/sideview-watch-stop`
- `/sideview-watch-status`

Model-callable tools:

- `sideview_watch_start`
- `sideview_watch_stop`
- `sideview_watch_status`

The tools should call the same `start`, `stop`, and status-formatting helpers as the
commands. Tool results must be concise text for the model plus structured details
for diagnosis. The start tool should accept the same mode arguments as the command;
the status tool takes no arguments; the stop tool takes no arguments.

Autostart is optional. If the user asked for a persistent watcher, start from
`session_start` with the chosen mode. If they asked for manual control, only start
when the command or tool runs. In both cases, `session_shutdown` must stop any live
child owned by this extension instance.

The status command and status tool report:

- configured mode and project root
- Pi session id
- whether the child is running
- exact argv used, with each argument separate
- start time
- delivered event count
- last event summary
- last error
- bounded stderr tail
- whether project-wide `--claim` is active

## Implementation template

Adapt this template after checking local Pi types. Keep it one file unless the
installed API forces otherwise.

```ts
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const MAX_LINE = 1024 * 1024;
const MAX_TAIL = 8192;

type Mode = { kind: "project" } | { kind: "page"; page: string; claimSafe: boolean };

type State = {
  child?: ChildProcess;
  ctx?: ExtensionContext;
  mode: Mode;
  argv: string[];
  startedAt?: number;
  deliveries: number;
  lastEvent?: string;
  lastError?: string;
  stderrTail: string;
  stopping: boolean;
};

export default function sideviewWatch(pi: ExtensionAPI) {
  const state: State = {
    mode: { kind: "project" },
    argv: [],
    deliveries: 0,
    stderrTail: "",
    stopping: false,
  };

  pi.on("session_start", async (_event, ctx) => {
    state.ctx = ctx;
    // If the user requested autostart, call start(ctx, state.mode) here.
    // Otherwise leave the watcher stopped until /sideview-watch-start.
  });

  pi.on("session_shutdown", async () => {
    await stop(state);
  });

  pi.registerCommand("sideview-watch-start", {
    description: "Start Sideview feedback monitor for this Pi session",
    handler: async (args, ctx) => {
      state.ctx = ctx;
      const parsed = parseMode(args);
      if (parsed) state.mode = parsed;
      await start(pi, state, ctx);
      ctx.ui.notify(`Sideview watcher started: ${state.argv.join(" ")}`, "info");
    },
  });

  pi.registerCommand("sideview-watch-stop", {
    description: "Stop Sideview feedback monitor",
    handler: async (_args, ctx) => {
      await stop(state);
      ctx.ui.notify("Sideview watcher stopped", "info");
    },
  });

  pi.registerCommand("sideview-watch-status", {
    description: "Show Sideview feedback monitor status",
    handler: async (_args, ctx) => {
      const text = formatStatus(state, ctx);
      ctx.ui.notify(text, isRunning(state) ? "info" : "warn");
    },
  });

  pi.registerTool({
    name: "sideview_watch_start",
    label: "Start Sideview Watcher",
    description: "Start the Sideview feedback watcher for this Pi session",
    promptSnippet: "Start Sideview feedback monitoring for this Pi session",
    promptGuidelines: ["Use sideview_watch_start when the user asks the Pi agent to watch Sideview feedback."],
    parameters: Type.Object({
      mode: Type.Optional(Type.Union([Type.Literal("project"), Type.Literal("page")])),
      page: Type.Optional(Type.String()),
    }),
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      if (params.mode === "project") state.mode = { kind: "project" };
      if (params.mode === "page") {
        if (!params.page) throw new Error("page mode requires page");
        state.mode = { kind: "page", page: params.page, claimSafe: false };
      }
      await start(pi, state, ctx);
      return {
        content: [{ type: "text", text: `Sideview watcher running: sideview ${state.argv.join(" ")}` }],
        details: statusDetails(state, ctx),
      };
    },
  });

  pi.registerTool({
    name: "sideview_watch_stop",
    label: "Stop Sideview Watcher",
    description: "Stop the Sideview feedback watcher for this Pi session",
    parameters: Type.Object({}),
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      await stop(state);
      return { content: [{ type: "text", text: "Sideview watcher stopped" }], details: statusDetails(state, ctx) };
    },
  });

  pi.registerTool({
    name: "sideview_watch_status",
    label: "Sideview Watcher Status",
    description: "Report Sideview feedback watcher status for this Pi session",
    parameters: Type.Object({}),
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      return { content: [{ type: "text", text: formatStatus(state, ctx) }], details: statusDetails(state, ctx) };
    },
  });
}

function isRunning(state: State) {
  return !!state.child && state.child.exitCode === null && !state.stopping;
}

function statusDetails(state: State, ctx: ExtensionContext) {
  return {
    running: isRunning(state),
    mode: state.mode,
    piSession: ctx.sessionManager.getSessionId(),
    argv: state.argv,
    startedAt: state.startedAt,
    deliveries: state.deliveries,
    lastEvent: state.lastEvent,
    lastError: state.lastError,
    stderrTail: state.stderrTail,
  };
}

function formatStatus(state: State, ctx: ExtensionContext) {
  const d = statusDetails(state, ctx);
  return [
    `running: ${d.running}`,
    `mode: ${JSON.stringify(d.mode)}`,
    `pi session: ${d.piSession}`,
    `argv: ${d.argv.length ? `sideview ${d.argv.join(" ")}` : "-"}`,
    `started: ${d.startedAt ? new Date(d.startedAt).toISOString() : "-"}`,
    `deliveries: ${d.deliveries}`,
    `last event: ${d.lastEvent ?? "-"}`,
    `last error: ${d.lastError ?? "-"}`,
    d.stderrTail ? `stderr:\n${d.stderrTail}` : "stderr: -",
  ].join("\n");
}

function parseMode(args: string): Mode | undefined {
  const parts = args.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return undefined;
  if (parts[0] === "project") return { kind: "project" };
  if (parts[0] === "page" && parts[1]) {
    // claimSafe must be set only after verifying the installed sideview has a page filter.
    return { kind: "page", page: parts[1], claimSafe: false };
  }
  throw new Error("usage: /sideview-watch-start [project|page <page-id>]");
}

async function start(pi: ExtensionAPI, state: State, ctx: ExtensionContext) {
  if (state.child && state.child.exitCode === null) return;

  const db = join(ctx.cwd, ".sideview", "sideview.db");
  if (!existsSync(db)) {
    throw new Error(`No Sideview store at ${db}; open or write a Sideview page first.`);
  }

  const argv = ["--project", ctx.cwd, "watch", "--since", "0", "--skip-author", "agent"];
  if (state.mode.kind === "project") {
    argv.push("--claim", "--ack");
  } else if (state.mode.claimSafe) {
    argv.push("--page", state.mode.page, "--claim", "--ack");
  }

  state.argv = argv;
  state.deliveries = 0;
  state.lastError = undefined;
  state.stderrTail = "";
  state.stopping = false;
  state.startedAt = Date.now();

  const env = {
    ...process.env,
    SIDEVIEW_SESSION: `pi:${ctx.sessionManager.getSessionId()}`,
  };

  const child = spawn("sideview", argv, { cwd: ctx.cwd, env, stdio: ["ignore", "pipe", "pipe"] });
  state.child = child;

  if (!child.stdout || !child.stderr) throw new Error("sideview child did not expose stdout/stderr");

  let buf = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => {
    buf += chunk;
    if (buf.length > MAX_LINE) {
      state.lastError = `Sideview event line exceeded ${MAX_LINE} bytes`;
      child.kill();
      return;
    }
    for (;;) {
      const nl = buf.indexOf("\n");
      if (nl < 0) break;
      const line = buf.slice(0, nl).trim();
      buf = buf.slice(nl + 1);
      if (line) deliver(pi, state, line);
    }
  });

  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    state.stderrTail = (state.stderrTail + chunk).slice(-MAX_TAIL);
  });

  child.on("error", (err) => {
    state.lastError = err.message;
  });

  child.on("exit", (code, signal) => {
    if (!state.stopping) state.lastError = `sideview watch exited: code=${code} signal=${signal}`;
  });

  await new Promise((resolve) => setTimeout(resolve, 250));
  if (child.exitCode !== null) {
    throw new Error(state.lastError ?? `sideview watch exited early with ${child.exitCode}`);
  }
}

function deliver(pi: ExtensionAPI, state: State, line: string) {
  if (state.stopping) return;
  let ev: any;
  try {
    ev = JSON.parse(line);
  } catch (err: any) {
    state.lastError = `Bad Sideview JSONL: ${err.message}`;
    return;
  }

  if (state.mode.kind === "page" && !state.mode.claimSafe && ev.page !== state.mode.page) return;

  const message = formatEvent(ev);
  state.lastEvent = summarizeEvent(ev);

  try {
    const ctx = state.ctx;
    const options = ctx && !ctx.isIdle() ? { deliverAs: "followUp" as const } : undefined;
    pi.sendUserMessage(message, options);
    state.deliveries += 1;
  } catch (err: any) {
    state.lastError = `Pi delivery failed: ${err.message}`;
  }
}

async function stop(state: State) {
  const child = state.child;
  state.stopping = true;
  state.child = undefined;
  if (!child || child.exitCode !== null) return;
  child.kill("SIGTERM");
  await new Promise((resolve) => setTimeout(resolve, 500));
  if (child.exitCode === null) child.kill("SIGKILL");
}

function summarizeEvent(ev: any): string {
  return `${ev.type ?? "event"}${ev.kind ? `/${ev.kind}` : ""} page=${ev.page ?? "?"} thread=${ev.thread ?? "?"} id=${ev.id ?? "-"}`;
}

function formatEvent(ev: any): string {
  const title = ev.type === "comment"
    ? (ev.kind === "edit" ? "Sideview edit request" : ev.kind === "edited" ? "Sideview block edit notice" : "Sideview comment")
    : ev.type === "resolve" ? "Sideview thread resolved"
    : ev.type === "unresolve" ? "Sideview thread reopened"
    : "Sideview event";

  const parts = [title];
  const meta = [`Page ${ev.page ?? "?"}`];
  if (ev.thread !== undefined) meta.push(`thread ${ev.thread}`);
  if (ev.id !== undefined) meta.push(`event ${ev.id}`);
  parts.push(meta.join(" · "));
  if (ev.quote) parts.push(`Quoted:\n${ev.quote}`);
  if (ev.body) parts.push(`Feedback:\n${ev.body}`);
  if (Array.isArray(ev.attachments) && ev.attachments.length > 0) {
    parts.push(`Attachments: ${ev.attachments.length}; inspect metadata before reading bytes.`);
  }
  parts.push(instructionsFor(ev));
  return parts.join("\n\n");
}

function instructionsFor(ev: any): string {
  if (ev.type === "resolve") return "The user resolved this thread. Leave it resolved unless asked otherwise.";
  if (ev.type === "unresolve") return "The user reopened this thread. Inspect the thread and respond only if needed.";
  if (ev.kind === "edit") return "Treat this as an edit request: inspect current canon, merge deliberately, reply on the named thread with --page as a guard, then resolve after merging.";
  if (ev.kind === "edited") return "Machine mail: re-read the changed Sideview page before writing. Do not reply to this hidden record unless user-visible action is needed.";
  return "Reply on the named Sideview thread with the page as a guard. Do not resolve merely because you replied.";
}
```

## Verify before reload

Run checks that do not install new dependencies unless the user asks. At minimum:

1. Compare imports and API calls with the installed Pi `.d.ts` files.
2. Run a syntax/type check if the project or Pi installation already provides one.
3. Run `sideview --project <project-root> status` or `sideview --project <project-root> sessions` to confirm the project store is reachable.
4. If page-scoped mode was requested, verify `sideview watch --help` actually supports the page filter before enabling `claimSafe`.

Report the generated file, chosen mode, exact argv, and whether `/reload` or restart is needed.

## Live acceptance

After `/reload` or Pi restart:

1. Run `/sideview-watch-status`; it should report stopped or running as configured.
2. Start the watcher if manual: `/sideview-watch-start project`.
3. Confirm status shows a live child and the exact argv.
4. Let Pi become idle.
5. From the Sideview browser, submit one distinctive comment.
6. Send no Pi chat message. The comment must start a visible Pi turn.
7. Have the agent reply on the named Sideview thread using the event's page guard.
8. `--skip-author agent` must prevent the reply from causing another Pi turn.
9. Run status and confirm one delivery and no error.
10. Stop the watcher and confirm status reports stopped.

The experiment passes only if step 6 happens while chat is otherwise silent. If the
message appears only after the user types another prompt, the extension queued or
buffered incorrectly.

## Follow-up tests

Once one event passes:

- Submit several comments quickly and confirm each arrives once.
- Run two Pi sessions in the same project with project-wide claim and confirm the
  user understands whichever watcher claims first owns the event.
- Exercise comment, edit request, edited notice, resolve, and unresolve events.
- Quit Pi and verify no `sideview watch` child remains.
- Force Sideview unavailable and confirm status reports the spawn/exit error.
- Test an over-large line or noisy stderr only if safe; the extension must stay bounded.

## Report back

State:

- Pi version and extension API files inspected
- generated extension path
- selected watch mode and exact claim policy
- exact argv and project root
- static checks run
- reload/restart instruction
- live acceptance result, if tested

If this harness has no subagent facility, say so plainly. Do not claim subagent
validation. You can still run the static checks above and ask the user to perform the
browser-comment wake test after reloading Pi.
