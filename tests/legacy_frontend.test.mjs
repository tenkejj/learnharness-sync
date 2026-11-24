import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
const source = readFileSync("src/api.ts", "utf8");
const js = ts.transpile(source, {
  module: ts.ModuleKind.ESNext,
  target: ts.ScriptTarget.ES2022,
});
const api = await import(
  "data:text/javascript;base64," + Buffer.from(js).toString("base64")
);
test("engine offline is an error, never a synthetic success", async () => {
  const original = global.fetch;
  global.fetch = async () => new Response("", { status: 503 });
  try {
    await assert.rejects(api.loadStatus(), /could not be reached/);
  } finally {
    global.fetch = original;
  }
});
test("engine response is projected without injecting peers", async () => {
  const original = global.fetch;
  global.fetch = async () =>
    Response.json({
      mode: "monitor",
      state: "Setup required",
      device: "test",
      data_path: "/synthetic",
      detected: false,
      head: null,
      snapshots: [],
      backups: [],
      events: [],
      conflicts: [],
      automatic: false,
      integration: "Deferred",
      transport_installed: false,
      transport: { state: "not_configured", peers: [], message: "No key" },
      limitations: [],
    });
  try {
    const status = await api.loadStatus();
    assert.deepEqual(status.transport.peers, []);
    assert.equal(status.mode, "monitor");
  } finally {
    global.fetch = original;
  }
});
test("unexpected server content is rejected", async () => {
  const original = global.fetch;
  global.fetch = async () => Response.json({ wrong: true });
  try {
    await assert.rejects(api.loadStatus(), /Unexpected engine response/);
  } finally {
    global.fetch = original;
  }
});
test("unapplied head stays explicit", () =>
  assert.equal(api.short(null), "No revision applied"));

test("malformed status shape cannot reach the render tree", async () => {
  const original = global.fetch;
  try {
    for (const value of [
      { snapshots: [] },
      { snapshots: [], transport: { peers: null } },
    ]) {
      global.fetch = async () => Response.json(value);
      await assert.rejects(api.loadStatus(), /Unexpected engine response/);
    }
  } finally {
    global.fetch = original;
  }
});
test("network failure includes local recovery guidance", async () => {
  const original = global.fetch;
  global.fetch = async () => {
    throw new TypeError("Failed to fetch");
  };
  try {
    await assert.rejects(api.loadStatus(), /Start the HarnessSync desktop app/);
  } finally {
    global.fetch = original;
  }
});

test("authenticated command obtains session and sends bounded JSON envelope", async () => {
  const original = global.fetch;
  const calls = [];
  global.fetch = async (url, options) => {
    calls.push({ url, options });
    return calls.length === 1
      ? Response.json({ token: "SYNTHETICSESSION" })
      : Response.json({ result: { saved: true } });
  };
  try {
    assert.deepEqual(
      await api.command("save_settings", { changes: { port: 8385 } }),
      { saved: true },
    );
    assert.equal(
      calls[1].options.headers["X-HarnessSync-Token"],
      "SYNTHETICSESSION",
    );
    assert.equal(JSON.parse(calls[1].options.body).command, "save_settings");
  } finally {
    global.fetch = original;
  }
});
test("command errors do not turn into success", async () => {
  const original = global.fetch;
  let count = 0;
  global.fetch = async () =>
    ++count === 1
      ? Response.json({ token: "test" })
      : Response.json({ error: "Explicit safety refusal" }, { status: 409 });
  try {
    await assert.rejects(api.command("apply", {}), /Explicit safety refusal/);
  } finally {
    global.fetch = original;
  }
});
test("native bridge uses same command boundary without HTTP", async () => {
  const previous = global.window;
  const calls = [];
  global.window = {
    __TAURI__: {
      core: {
        invoke: async (name, args) => {
          calls.push({ name, args });
          return { saved: true };
        },
      },
    },
  };
  try {
    assert.deepEqual(await api.command("settings"), { saved: true });
    assert.equal(calls[0].name, "engine_request");
    assert.equal(calls[0].args.command, "settings");
  } finally {
    if (previous === undefined) delete global.window;
    else global.window = previous;
  }
});

test("lost mutation response reports uncertain outcome before retry", async () => {
  const original = global.fetch;
  let calls = 0;
  global.fetch = async () => {
    if (++calls === 1) return Response.json({ token: "test" });
    throw new TypeError("Failed to fetch");
  };
  try {
    await assert.rejects(api.command("pair", {}), /may already have completed/);
  } finally {
    global.fetch = original;
  }
});
test("invalid mutation JSON cannot imply failure without side effects", async () => {
  const original = global.fetch;
  let calls = 0;
  global.fetch = async () =>
    ++calls === 1
      ? Response.json({ token: "test" })
      : new Response("invalid", { status: 200 });
  try {
    await assert.rejects(
      api.command("save_settings", {}),
      /Refresh local status/,
    );
  } finally {
    global.fetch = original;
  }
});
