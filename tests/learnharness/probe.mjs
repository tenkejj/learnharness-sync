// Runs only inside the disposable namespace created by the shell driver.
import assert from "node:assert/strict";
import { chromium } from "/node_modules/playwright/index.mjs";
import { readFileSync, writeFileSync } from "node:fs";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
const until = Date.now() + 45000;
let page;
while (Date.now() < until) {
  page = browser
    .contexts()[0]
    .pages()
    .find((p) => /^http:\/\/127\.0\.0\.1:/.test(p.url()));
  if (page) break;
  await new Promise((r) => setTimeout(r, 250));
}
assert.ok(page, "LearnHarness window did not load");
console.log("LearnHarness window ready");
const [phase, label] = process.argv.slice(2);
const request = (path, method = "GET", body) =>
  page.evaluate(
    async ({ path, method, body }) => {
      const res = await fetch(path, {
        method,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!res.ok) throw Error("LearnHarness HTTP " + res.status);
      return res.json();
    },
    { path, method, body },
  );
if (
  phase === "transcript" ||
  phase === "verify-transcript" ||
  phase === "configure" ||
  phase === "rename-transcript" ||
  phase === "extend-transcript" ||
  phase === "branch" ||
  phase === "path-reference"
) {
  const { startProvider } = await import("/provider.mjs");
  const provider = await startProvider();
  let id;
  if (phase === "transcript" || phase === "configure" || phase === "extend-transcript") {
    await page.evaluate(async () => {
      const route = "/api/custom-provider";
      const previous = await fetch(route);
      const result = await fetch(route, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "If-Match": previous.headers.get("etag"),
        },
        body: JSON.stringify({
          baseUrl: "http://127.0.0.1:9401/v1",
          authMode: "keyless",
        }),
      });
      if (!result.ok) throw Error("Provider setup " + result.status);
    });
    await request("/api/settings/models", "PUT", {
      strong: { model: { provider: "custom", id: "qa-model" } },
      cheap: { model: { provider: "custom", id: "qa-model" } },
    });
  }
  if (phase === "transcript") {
    ({ id } = await request("/api/chats", "POST", {
      title: "Disposable transcript",
    }));
    writeFileSync("/profile/transcript-id", id);
    for (const text of ["What is a triangle?", "Czy ma trzy boki?"]) {
      await request("/api/chat", "POST", {
        chatId: id,
        input: { type: "text", text },
      });
      for (let i = 0; i < 100; i++) {
        const chat = await request("/api/chats/" + id);
        if (!chat.turnInFlight) break;
        await new Promise((resolve) => setTimeout(resolve, 150));
      }
    }
    await request("/api/graph/coverage/extract", "POST", { chatId: id });
    for (let i = 0; i < 100; i++) {
      if ((await request("/api/graph")).propositions.length) break;
      await new Promise((r) => setTimeout(r, 150));
    }
    if (process.env.LEARNHARNESS_SYNC_PROFILE_FORMAT === "learnharness-0.7.0-disposable-v1") {
      const created = await request("/api/graph/sources", "POST", {
        action: "create-folder", name: "Disposable 0.7 folder", parentId: null,
      });
      const folder = created.organization.folders.find((row) => row.name === "Disposable 0.7 folder");
      assert.ok(folder?.id);
      const moved = await request("/api/graph/sources", "POST", {
        action: "move", kind: "source", id, parentId: folder.id,
      });
      assert.equal(moved.organization.placements[id].folderId, folder.id);
    }
  } else if (phase === "extend-transcript") {
    const { chats } = await request("/api/chats");
    assert.equal(chats.length, 1, "The disposable peer profile must contain the imported session only");
    id = chats[0].id;
    writeFileSync("/profile/transcript-id", id);
  } else if (phase !== "configure")
    id = readFileSync("/profile/transcript-id", "utf8");
  if (phase !== "configure") {
    if (process.env.LEARNHARNESS_SYNC_PROFILE_FORMAT === "learnharness-0.7.0-disposable-v1") {
      const directory = await request("/api/graph/sources");
      const folders = directory.organization.folders;
      assert.equal(folders.length, 1);
      assert.equal(folders[0].name, "Disposable 0.7 folder");
      assert.equal(directory.organization.placements[id].folderId, folders[0].id);
    }
    if (phase === "path-reference") {
      writeFileSync("/profile/triangle.txt", "A disposable attachment");
      const text = 'Read "/profile/triangle.txt"';
      await request("/api/chat", "POST", {
        chatId: id,
        input: { type: "text", text },
      });
      for (let i = 0; i < 100; i++) {
        const current = await request("/api/chats/" + id);
        if (
          !current.turnInFlight &&
          current.messages.some((m) => m.content === text)
        )
          break;
        await new Promise((r) => setTimeout(r, 150));
      }
      assert.ok(
        (await request("/api/chats/" + id)).messages.some(
          (m) => m.content === text,
        ),
      );
    }
    if (phase === "rename-transcript") {
      await request("/api/chats/" + id, "PUT", { title: "Updated on D" });
      await request("/api/chat", "POST", { chatId: id, input: { type: "text", text: "Kontynuacja na D – 三角形" } });
      for (let i = 0; i < 150; i++) {
        const current = await request("/api/chats/" + id);
        if (current.messages.length === 6 && !current.turnInFlight) break;
        await new Promise((r) => setTimeout(r, 100));
      }
    }
    if (phase === "extend-transcript") {
      const text = "Follow-up created on the second packaged peer – 続き ✓";
      await request("/api/chat", "POST", { chatId: id, input: { type: "text", text } });
      for (let i = 0; i < 150; i++) {
        const current = await request("/api/chats/" + id);
        if (current.messages.length >= 6 && !current.turnInFlight) break;
        await new Promise((r) => setTimeout(r, 100));
      }
      assert.ok((await request("/api/chats/" + id)).messages.some((m) => m.content === text));
    }
    if (phase === "branch")
      await request("/api/chats/" + id, "PUT", {
        title: "Offline branch " + label,
      });
    const chat = await request("/api/chats/" + id);
    if (
      phase === "rename-transcript" ||
      (label === "C" && phase === "verify-transcript")
    )
      {
        assert.equal(chat.title, "Updated on D");
        assert.equal(chat.messages.length, 6);
        assert.ok(chat.messages.some((m) => m.content === "Kontynuacja na D – 三角形"));
      }
    assert.ok(chat.messages.length >= 4);
    const transcript = JSON.stringify(chat.messages);
    for (const text of [
      "What is a triangle?",
      "Czy ma trzy boki?",
      "A triangle has three sides.",
    ])
      assert.ok(transcript.includes(text));
    console.log(phase, chat.messages.length, "native messages");
    let graph = await request("/api/graph");
    if (phase === "transcript") {
      const values = Object.fromEntries(graph.nodes.map(({ id }, i) => [id, { x: i + 12, y: -i - 24 }]));
      const saved = await request("/api/graph/positions", "PUT", values);
      assert.deepEqual(saved.positions, values);
      graph = await request("/api/graph");
    }
    assert.equal(Object.keys(graph.positions).length, 4);
    assert.ok(graph.positions[graph.chains.find((c) => c.premises.length === 2).id]);
    const persisted = { messages: chat.messages, chains: graph.chains, positions: graph.positions, propositions: graph.propositions };
    const oracle = "/profile/expected-transcript.json";
    if (phase === "transcript" || phase === "rename-transcript" || phase === "extend-transcript") {
      writeFileSync(oracle, JSON.stringify(persisted));
    } else if (phase === "verify-transcript") {
      assert.deepEqual(persisted, JSON.parse(readFileSync(oracle, "utf8")));
    }
    assert.equal(graph.propositions.length, 3);
    assert.equal(graph.chains.length, 2);
    for (const node of graph.propositions) {
      assert.ok(
        node.provenance.some(
          (p) =>
            p.chatId === id && chat.messages.some((m) => m.id === p.messageId),
        ),
      );
    }
    console.log(
      "Verified three propositions, both chain types, saved layout and message provenance",
    );
  }
  provider.close();
} else {
  let expected;
  if (phase === "empty") {
    expected = [];
  } else if (phase === "create" || phase === "append") {
    expected =
      phase === "append"
        ? JSON.parse(readFileSync("/profile/expected.json"))
        : [];
    assert.equal((await request("/api/chats")).chats.length, expected.length);
    for (const kind of ["teach", "practice", "teach"]) {
      const { id } = await request("/api/chats", "POST", {
        title: label + " · Łódź 数学",
        kind,
      });
      const path = "/api/chats/" + id;
      const original = await request(path);
      const title = label + " – Δ" + "界".repeat(1800) + expected.length;
      for (const value of ["Pierwsza wersja", title])
        await request(path, "PUT", { title: value });
      const updated = await request(path);
      assert.equal(updated.updatedAt, original.updatedAt);
      const refusal = await page.evaluate(async (id) => {
        const res = await fetch("/api/chat", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            chatId: id,
            input: { type: "text", text: "Explain a triangle." },
          }),
        });
        return { status: res.status, body: await res.json() };
      }, id);
      assert.equal(refusal.status, 503);
      assert.equal(refusal.body.code, "models_not_configured");
      expected.push({ id, title, kind });
    }
    writeFileSync("/profile/expected.json", JSON.stringify(expected));
  } else expected = JSON.parse(readFileSync("/profile/expected.json"));
  assert.equal((await request("/api/chats")).chats.length, expected.length);
  for (const { id, title, kind } of expected) {
    const chat = await request("/api/chats/" + id);
    assert.equal(chat.id, id);
    assert.equal(chat.title, title);
    assert.equal(chat.kind ?? "teach", kind);
    assert.deepEqual(chat.messages, []);
  }
  console.log(
    phase,
    label,
    expected.length,
    "sessions verified through trusted desktop window",
  );
}
await page.evaluate(() => setTimeout(() => window.close(), 10));
await browser.close();
