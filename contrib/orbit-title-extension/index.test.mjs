#!/usr/bin/env node
/**
 * Behaviour tests for the Orbit auto-title extension entry.
 *
 * Run: node --test contrib/orbit-title-extension/index.test.mjs
 *
 * Drives the extension the way pi does — `activate(pi)`, then an
 * `agent_settled` event — with a fake `pi`/`ctx` and a temp HOME holding the
 * same `~/.orbit-pi/auto-title.json` Orbit writes, asserting exactly which
 * model is asked and which title reaches `pi.setSessionName`.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { beforeEach } from "node:test";

import activate, { pendingTitle } from "./index.js";

let home;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-title-entry-"));
  fs.mkdirSync(path.join(home, ".orbit-pi"), { recursive: true });
  process.env.HOME = home;
});

function setConfig(config) {
  fs.writeFileSync(path.join(home, ".orbit-pi", "auto-title.json"), JSON.stringify(config));
}

function fakePi({ name = "" } = {}) {
  const handlers = new Map();
  const commands = new Map();
  const state = { name, set: [] };
  return {
    state,
    commands,
    on(event, handler) {
      handlers.set(event, handler);
    },
    registerCommand(name, options) {
      commands.set(name, options);
    },
    fire(event, payload, ctx) {
      const handler = handlers.get(event);
      assert.ok(handler, `no handler for ${event}`);
      return handler(payload, ctx);
    },
    getSessionName() {
      return state.name;
    },
    setSessionName(title) {
      state.name = title;
      state.set.push(title);
    },
  };
}

function exchange() {
  return [
    { type: "message", message: { role: "user", content: "fix the redirect loop on login" } },
    {
      type: "message",
      message: { role: "assistant", content: [{ type: "text", text: "I traced it to the oauth callback." }] },
    },
  ];
}

function fakeCtx({ title = "Fix login redirect", model = { provider: "active", id: "session-model" } } = {}) {
  const calls = [];
  return {
    calls,
    model,
    signal: undefined,
    sessionManager: { getBranch: () => exchange() },
    modelRegistry: {
      find: () => undefined,
      hasConfiguredAuth: () => true,
      complete: async (chosen, request, options) => {
        calls.push({ chosen, request, options });
        return { content: [{ type: "text", text: `"${title}"` }] };
      },
    },
  };
}

test("titles the session from the first exchange with the active model", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
  assert.equal(ctx.calls.length, 1);
  assert.deepEqual(ctx.calls[0].chosen, { provider: "active", id: "session-model" });
  assert.match(ctx.calls[0].request.messages[0].content[0].text, /fix the redirect loop on login/);
  // The budget has to cover a reasoning model's thinking, or the answer is
  // starved and no title is ever produced.
  assert.ok(ctx.calls[0].options.maxTokens >= 256);
});

test("uses the configured model when the setting names one", async () => {
  setConfig({ enabled: true, model: { provider: "ollama", id: "glm-5.3" } });
  const pi = fakePi();
  activate(pi);
  const chosen = { provider: "ollama", id: "glm-5.3" };
  const ctx = fakeCtx();
  ctx.modelRegistry.find = (provider, id) =>
    provider === "ollama" && id === "glm-5.3" ? chosen : undefined;
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.equal(ctx.calls[0].chosen, chosen);
  assert.equal(pi.state.name, "Fix login redirect");
});

test("does nothing when the setting is disabled", async () => {
  setConfig({ enabled: false });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.equal(ctx.calls.length, 0);
  assert.deepEqual(pi.state.set, []);
});

test("never clobbers a name that is already set", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi({ name: "Manual title" });
  activate(pi);
  const ctx = fakeCtx();
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.equal(ctx.calls.length, 0, "an already-named session is left alone");
  assert.deepEqual(pi.state.set, []);
});

test("titles a first turn that called tools (several assistant messages)", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // A real tool-using turn streams toolCall-only assistants before the final
  // text reply; with one user prompt the session is still fresh and titleable.
  ctx.sessionManager.getBranch = () => [
    { type: "message", message: { role: "user", content: "fix the redirect loop on login" } },
    { type: "message", message: { role: "assistant", content: [{ type: "toolCall", name: "read" }] } },
    { type: "message", message: { role: "toolResult", content: [{ type: "text", text: "…" }] } },
    {
      type: "message",
      message: { role: "assistant", content: [{ type: "text", text: "I traced it to the oauth callback." }] },
    },
  ];
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
  assert.equal(ctx.calls.length, 1);
  assert.match(ctx.calls[0].request.messages[0].content[0].text, /I traced it to the oauth callback/);
});

test("titles from a reply that leads with a thinking block", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // Reasoning models emit a `thinking` block before the text answer; the
  // title comes from the text, never the thinking.
  ctx.modelRegistry.complete = async (chosen, request, options) => {
    ctx.calls.push({ chosen, request, options });
    return {
      content: [
        { type: "thinking", thinking: "The user wants a short title…" },
        { type: "text", text: '"Fix login redirect"' },
      ],
    };
  };
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
});

test("does not retry a completed ask that produced no title", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // A clean completion with no text (the whole budget went to thinking) is
  // deterministic: asking again would only repeat the empty reply.
  ctx.modelRegistry.complete = async (chosen, request, options) => {
    ctx.calls.push({ chosen, request, options });
    return { content: [{ type: "thinking", thinking: "…" }] };
  };
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.equal(ctx.calls.length, 1);
  assert.deepEqual(pi.state.set, []);
});

test("the /generate-title command re-titles an already-named session", async () => {
  // Manual titling is an explicit request: it works even when the automatic
  // setting is off and even when the session already carries a name.
  setConfig({ enabled: false });
  const pi = fakePi({ name: "Old title" });
  activate(pi);
  const ctx = fakeCtx();
  const command = pi.commands.get("generate-title");
  assert.ok(command, "generate-title is registered");

  await command.handler("", ctx);

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
  assert.equal(ctx.calls.length, 1);
});

test("the /generate-title command titles a resumed session", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // More than one user prompt means the automatic pass would skip it; a
  // manual request still describes the session from its first exchange.
  ctx.sessionManager.getBranch = () => [
    ...exchange(),
    { type: "message", message: { role: "user", content: "again" } },
    { type: "message", message: { role: "assistant", content: [{ type: "text", text: "done" }] } },
  ];

  await pi.commands.get("generate-title").handler("", ctx);

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
});

test("titles a resumed session that carries no name", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // A session Orbit reopened from disk has several user prompts already. It is
  // untitled, so the next settle names it — "fresh vs resumed" is answered by
  // whether a name exists, not by how many prompts the branch holds.
  ctx.sessionManager.getBranch = () => [
    ...exchange(),
    { type: "message", message: { role: "user", content: "again" } },
    { type: "message", message: { role: "assistant", content: [{ type: "text", text: "done" }] } },
  ];
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
  assert.equal(ctx.calls.length, 1);
});

test("titles through a steering message queued during the first run", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // Sending a second prompt while the first turn runs used to leave the session
  // with two user prompts at settle time, which read as "resumed" and cost it
  // the title.
  ctx.sessionManager.getBranch = () => [
    { type: "message", message: { role: "user", content: "fix the redirect loop on login" } },
    { type: "message", message: { role: "assistant", content: [{ type: "text", text: "Working on it." }] } },
    { type: "message", message: { role: "user", content: "also add a test" } },
    {
      type: "message",
      message: { role: "assistant", content: [{ type: "text", text: "I traced it to the oauth callback." }] },
    },
  ];
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
});

test("a settle with no reply to describe does not write the session off", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  // A first settle that lands before the assistant produced text (an aborted or
  // tool-only turn) has nothing to describe; the next turn still titles it.
  ctx.sessionManager.getBranch = () => [
    { type: "message", message: { role: "user", content: "fix the redirect loop on login" } },
  ];
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();
  assert.equal(ctx.calls.length, 0, "nothing to describe yet");

  ctx.sessionManager.getBranch = () => exchange();
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();

  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
  assert.equal(ctx.calls.length, 1);
});

test("a failed ask is retried on the next settle", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  let attempt = 0;
  ctx.modelRegistry.complete = async (chosen, request) => {
    attempt += 1;
    if (attempt === 1) {
      throw new Error("provider hiccup");
    }
    return { content: [{ type: "text", text: '"Fix login redirect"' }] };
  };
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();
  assert.deepEqual(pi.state.set, []);

  // The retry happens after the user's next prompt, so the branch has grown by
  // then — which is exactly the case the old "only one user prompt" rule
  // rejected, making the retry dead in practice.
  ctx.sessionManager.getBranch = () => [
    ...exchange(),
    { type: "message", message: { role: "user", content: "thanks, now add a test" } },
    { type: "message", message: { role: "assistant", content: [{ type: "text", text: "Added." }] } },
  ];
  await pi.fire("agent_settled", {}, ctx);
  await pendingTitle();
  assert.deepEqual(pi.state.set, ["Fix login redirect"]);
});

test("gives up after three transient failures", async () => {
  setConfig({ enabled: true, model: null });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx();
  let asked = 0;
  ctx.modelRegistry.complete = async () => {
    asked += 1;
    throw new Error("model unavailable");
  };
  for (let settle = 0; settle < 5; settle += 1) {
    await pi.fire("agent_settled", {}, ctx);
    await pendingTitle();
  }

  assert.equal(asked, 3, "bounded: a broken model is not asked every turn");
  assert.deepEqual(pi.state.set, []);
});
