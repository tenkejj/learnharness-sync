// Independent readback of real LearnHarness records after its managed window exits.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const profile = process.argv[2];
assert.ok(profile?.startsWith("/slot/vault/session."));
const data = join(profile, "config/LearnHarness/data");
const chatId = readFileSync(join(profile, "transcript-id"), "utf8").trim();
const expected = JSON.parse(readFileSync(join(profile, "expected-transcript.json"), "utf8"));
const chatPath = `chats/${chatId}.json`;
const chat = JSON.parse(readFileSync(join(data, chatPath), "utf8"));
assert.equal(chat.id, chatId);
assert.deepEqual(chat.messages, expected.messages, "Persisted chat messages differ from LearnHarness application readback");
assert.equal(chat.messages.length, expected.messages.length);
assert.ok(chat.messages.length >= 4);
assert.ok(chat.messages.some((message) => message.content === "What is a triangle?"));
assert.ok(chat.messages.some((message) => message.content === "Czy ma trzy boki?"));
assert.ok(chat.messages.some((message) => JSON.stringify(message.content).includes("A triangle has three sides.")));

const jsonl = (name) =>
  readFileSync(join(data, "graph", name), "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
const propositions = jsonl("nodes.jsonl");
const chains = jsonl("chains.jsonl");
assert.deepEqual(propositions, expected.propositions, "Persisted propositions differ from LearnHarness application readback");
assert.deepEqual(chains, expected.chains, "Persisted graph chains differ from LearnHarness application readback");
assert.equal(propositions.length, 3);
assert.equal(chains.length, 2);
for (const proposition of propositions) {
  assert.ok(proposition.provenance.some((anchor) =>
    anchor.chatId === chatId && chat.messages.some((message) => message.id === anchor.messageId)),
  "Persisted graph proposition provenance is not linked to this conversation");
}
const positions = JSON.parse(readFileSync(join(data, "graph/positions.json"), "utf8"));
assert.deepEqual(positions.positions, expected.positions, "Persisted layout differs from LearnHarness application readback");
assert.equal(Object.keys(positions.positions).length, 4);

const manifest = [];
for (const dir of readdirSync(data, { withFileTypes: true })) {
  if (dir.name === "settings.json" && dir.isFile()) continue;
  assert.ok(dir.isDirectory() && ["chats", "graph", "jobs", "plugins"].includes(dir.name));
  if (dir.name === "jobs" || dir.name === "plugins") {
    assert.deepEqual(readdirSync(join(data, dir.name)), []);
    continue;
  }
  for (const entry of readdirSync(join(data, dir.name), { withFileTypes: true })) {
    assert.ok(entry.isFile());
    const path = `${dir.name}/${entry.name}`;
    assert.ok(path === chatPath || (dir.name === "graph" && [
      "format.json", "nodes.jsonl", "chains.jsonl", "coverage.jsonl", "positions.json",
    ].includes(entry.name)));
    const full = join(data, path);
    assert.ok(!lstatSync(full).isSymbolicLink());
    const bytes = readFileSync(full);
    manifest.push({
      path,
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    });
  }
}
manifest.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
assert.ok(manifest.length > 0 && manifest.reduce((n, record) => n + record.bytes, 0) > 0);
console.log("MANAGED_REAL_APP_REPORT=" + JSON.stringify({
  chatId,
  messageIds: chat.messages.map((message) => message.id),
  messageCount: chat.messages.length,
  propositionIds: propositions.map((item) => item.id).sort(),
  chainIds: chains.map((item) => item.id).sort(),
  chainRelationships: chains.map(({ premises, conclusion }) => ({ premises, conclusion })),
  manifest,
}));
