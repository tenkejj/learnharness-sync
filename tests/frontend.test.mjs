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
const status = () => ({
  sync_test: null,
  mode: "monitor",
  state: "Ready",
  device: "Test",
  data_path: "/synthetic",
  detected: false,
  head: null,
  snapshots: [],
  backups: [],
  events: [],
  conflicts: [],
  automatic: false,
  integration: "Disabled",
  transport_installed: true,
  transport: {
    state: "listening",
    device_id: "a".repeat(64),
    peers: [],
    message: "LAN",
  },
  discovered: [],
  pending: [],
  pairing_open: false,
  listen_port: 12345,
  diagnostics: {},
  transfer: { verified_received_bytes: 0, verified_received_objects: 0 },
  limitations: [],
});
async function native(invoke, run) {
  const previous = global.window;
  global.window = { __TAURI__: { core: { invoke } } };
  try {
    await run();
  } finally {
    if (previous === undefined) delete global.window;
    else global.window = previous;
  }
}
test("engine offline is an error, never a synthetic success", async () => {
  await assert.rejects(api.loadStatus(), /could not be reached/);
});
test("native response is projected without injecting peers", async () =>
  native(
    async () => status(),
    async () => {
      assert.deepEqual((await api.loadStatus()).transport.peers, []);
    },
  ));
test("unexpected native content is rejected", async () =>
  native(
    async () => ({ wrong: true }),
    async () => {
      await assert.rejects(api.loadStatus(), /Unexpected engine response/);
    },
  ));
test("unapplied head stays explicit", () =>
  assert.equal(api.short(null), "No revision applied"));
test("malformed status shapes cannot reach the render tree", () => {
  for (const value of [
    { snapshots: [] },
    { ...status(), transport: { peers: null } },
    { ...status(), pending: [{}] },
    { ...status(), diagnostics: null },
  ])
    assert.equal(api.isStatus(value), false);
});
test("offline failure includes local recovery guidance", async () => {
  await assert.rejects(
    api.loadStatus(),
    /Start the LearnHarness Sync desktop app/,
  );
});
test("native command sends bounded JSON envelope without HTTP", async () =>
  native(
    async (name, args) => {
      assert.equal(name, "engine_request");
      assert.equal(args.command, "save_settings");
      assert.deepEqual(args.args, { name: "Laptop" });
      return { saved: true };
    },
    async () =>
      assert.deepEqual(await api.command("save_settings", { name: "Laptop" }), {
        saved: true,
      }),
  ));
test("explicit safety refusal cannot turn into success", async () =>
  native(
    async () => {
      throw "Explicit safety refusal";
    },
    async () => {
      await assert.rejects(
        api.command("production_import"),
        /Explicit safety refusal/,
      );
    },
  ));
test("managed launcher commands stay inside the native IPC boundary", async () =>
  native(
    async (name, args) => {
      assert.equal(name, "engine_request");
      assert.equal(args.command, "managed_recover");
      assert.deepEqual(args.args, { confirmed: true });
      return "recovered";
    },
    async () =>
      assert.equal(
        await api.command("managed_recover", { confirmed: true }),
        "recovered",
      ),
  ));
test("unverifiable native failure reports uncertain outcome", async () =>
  native(
    async () => {
      throw new Error("IPC disconnected");
    },
    async () => {
      await assert.rejects(
        api.command("start_pairing"),
        /may already have completed/,
      );
    },
  ));
test("oversized command is rejected before invocation", async () =>
  native(
    async () => assert.fail("Must not invoke"),
    async () => {
      await assert.rejects(
        api.command("synthetic_edit", { text: "x".repeat(100000) }),
        /size limit/,
      );
    },
  ));
test("malformed snapshots cannot imply verification", () => {
  const value = status();
  value.snapshots = [
    {
      id: "a",
      device: "b",
      time: 0,
      files: 1,
      bytes: 2,
      format: "v2",
      verified: false,
      applied: false,
    },
  ];
  assert.equal(api.isStatus(value), false);
});
test("real pairing and discovery fields are retained", async () =>
  native(
    async () => ({
      ...status(),
      pending: [
        {
          id: "session",
          peer_id: "b",
          name: "Laptop",
          code: "compare both screens",
        },
      ],
      discovered: [{ id: "b", name: "Laptop", address: "192.168.1.2:12345" }],
    }),
    async () => {
      const v = await api.loadStatus();
      assert.equal(v.pending[0].name, "Laptop");
      assert.equal(v.discovered[0].address, "192.168.1.2:12345");
    },
  ));
test("unknown creation time is not fabricated", () =>
  assert.equal(api.when(0), "Not recorded"));
test("production frontend has no HTTP bridge or obsolete credentials", () => {
  assert.doesNotMatch(source, /fetch\(|api_key|Syncthing|Python/);
  assert.doesNotMatch(
    readFileSync("src/Controls.tsx", "utf8"),
    /Syncthing|API key|namespace/,
  );
});

const presentationJs = ts.transpile(
  readFileSync("src/presentation.ts", "utf8"),
  { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
);
const presentation = await import(
  "data:text/javascript;base64," +
    Buffer.from(presentationJs).toString("base64")
);
const paired = (connected = true) => ({
  ...status(),
  device: "Studio desktop",
  transport: {
    ...status().transport,
    peers: [
      {
        id: "b",
        name: "Travel laptop",
        connected,
        last_sync: null,
        error: null,
        receipt: null,
      },
    ],
  },
});
test("Home distinguishes first launch, connected and offline without user navigation", () => {
  assert.equal(presentation.homeState(status()).kind, "new");
  assert.equal(
    presentation.homeState(paired()).title,
    "Connected to Travel laptop",
  );
  assert.equal(
    presentation.homeState(paired(false)).title,
    "Travel laptop is offline",
  );
});
test("local and remote names cannot both be This computer", () => {
  assert.equal(presentation.localName("This computer"), "Your computer");
  assert.equal(
    presentation.peerName({ name: "This computer" }),
    "Other computer",
  );
});
test("connectivity and a past test never substitute for matching remote evidence", () => {
  const value = paired();
  value.head = "expected";
  value.sync_test = {
    state: "success",
    device: "b",
    name: "Travel laptop",
    snapshot: "expected",
    started: 1,
    finished: 2,
    message: "Verified",
  };
  assert.equal(presentation.homeState(value).kind, "connected");
  value.transport.peers[0].receipt = {
    head: "expected",
    received: "expected",
    conflict: false,
  };
  assert.equal(presentation.homeState(value).kind, "success");
  value.transport.peers[0].receipt.head = "different";
  assert.equal(presentation.homeState(value).kind, "connected");
  value.transport.peers[0].receipt.conflict = true;
  assert.equal(presentation.homeState(value).kind, "conflict");
});
test("malformed receipts and test results are refused", async () => {
  const value = paired();
  value.transport.peers[0].receipt = { head: 5 };
  assert.equal(api.isStatus(value), false);
  await native(
    async () => ({ state: "success" }),
    async () => {
      await assert.rejects(api.runSyncTest("b"), /could not be verified/);
    },
  );
});
test("failed native test is retained as failure, never rewritten to success", async () => {
  const result = {
    state: "failed",
    device: "b",
    name: "Laptop",
    snapshot: "test",
    started: 1,
    finished: 2,
    message: "Receiver refused",
  };
  await native(
    async () => result,
    async () => assert.deepEqual(await api.runSyncTest("b"), result),
  );
});
test("native event validation and unsubscription remain intact", async () => {
  const previous = global.window;
  let callback,
    disposed = false;
  const values = [];
  global.window = {
    __TAURI__: {
      event: {
        listen: async (name, cb) => {
          assert.equal(name, "engine-status");
          callback = cb;
          return () => {
            disposed = true;
          };
        },
      },
    },
  };
  try {
    const dispose = await api.subscribeStatus((v) => values.push(v));
    callback({ payload: { broken: true } });
    callback({ payload: status() });
    assert.equal(values.length, 1);
    dispose();
    assert.equal(disposed, true);
  } finally {
    if (previous === undefined) delete global.window;
    else global.window = previous;
  }
});

test("activity disclosure keys remain stable when new events arrive", () => {
  const events = [
    { time: 1, kind: "Published" },
    { time: 1, kind: "Published" },
  ];
  const original = presentation.keyedEvents(events).map((e) => e.key);
  const updated = presentation
    .keyedEvents([...events, { time: 2, kind: "Published" }])
    .slice(1)
    .map((e) => e.key);
  assert.deepEqual(updated, original);
  assert.equal(new Set(original).size, 2);
});
