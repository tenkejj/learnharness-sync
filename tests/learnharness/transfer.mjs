// The runner mounts only generated profiles at /lab after all application PIDs exit.
import { peerLab } from "/native-peer-lab.mjs";
import {
  readdirSync,
  readFileSync,
  writeFileSync,
  lstatSync,
} from "node:fs";
import assert from "node:assert/strict";
const [source, target, mode] = process.argv.slice(2);
const base = (name) => "/lab/" + name + "/config/LearnHarness/data";
function inventory(root) {
  const files = {};
  for (const dir of readdirSync(root, { withFileTypes: true })) {
    if (dir.name === "settings.json" && dir.isFile()) continue; // local model endpoint and credential-binding ID
    assert.ok(
      dir.isDirectory() &&
        ["chats", "jobs", "plugins", "graph"].includes(dir.name),
    );
    for (const entry of readdirSync(root + "/" + dir.name, {
      withFileTypes: true,
    })) {
      assert.ok(
        entry.isFile() &&
          ((dir.name === "chats" &&
            /^chat_[a-z0-9]+_[0-9a-f]{8}\.json$/.test(entry.name)) ||
            (dir.name === "graph" &&
              [
                "format.json",
                "chains.jsonl",
                "nodes.jsonl",
                "coverage.jsonl",
                "positions.json",
              ].includes(entry.name))),
      );
      const path = dir.name + "/" + entry.name;
      const full = root + "/" + path;
      assert.ok(lstatSync(full).size < 65536);
      const bytes = readFileSync(full);
      files[path] = new TextDecoder("utf-8", {
        fatal: true,
        ignoreBOM: true,
      }).decode(bytes);
    }
  }
  return files;
}
const { start, wait, pair, close } = peerLab("/lab", "/peer");
try {
  if (mode === "path-reference") {
    const peer = await start("engine-" + source);
    const before = (await peer.request("status")).head;
    await assert.rejects(
      peer.request("qa_capture"),
      /nonportable file reference/,
    );
    assert.equal((await peer.request("status")).head, before);
    await peer.stop();
    console.log(
      "Refused persisted file reference and preserved previous revision",
    );
  } else if (mode === "conflict") {
    const local = await start("engine-" + source);
    const left = inventory(base(source)),
      right = inventory(base(target));
    const remote = await start("engine-" + target);
    const remoteHead = await remote.request("qa_capture");
    // Transport receives the remote head before the first device has published
    // its offline application edit. Applying it would destroy that edit.
    await wait(local, (s) => s.head === remoteHead);
    await assert.rejects(local.request("qa_stage_received", { head: remoteHead }), /unpublished edits/);
    const localHead = await local.request("qa_capture");
    assert.notEqual(localHead, remoteHead);
    await local.stop();
    const restarted = await start("engine-" + source);
    for (const [peer, own, other] of [
      [restarted, localHead, remoteHead],
      [remote, remoteHead, localHead],
    ]) {
      const status = await wait(peer, (s) => s.conflicts.includes(other));
      assert.equal(status.head, own);
      assert.deepEqual(
        await peer.request("qa_read", { head: localHead }),
        left,
      );
      assert.deepEqual(
        await peer.request("qa_read", { head: remoteHead }),
        right,
      );
    }
    await restarted.stop();
    await remote.stop();
    console.log(
      "Both offline transcript and graph branches verified on both peers",
    );
  } else {
    const a = await start("engine-" + source),
      b = await start("engine-" + target);
    await a.request("initialize_synthetic", { confirmed: true });
    await b.request("initialize_synthetic", { confirmed: true });
    const files = inventory(base(source));
    const sample = Object.keys(files).find((key) => key.startsWith("chats/"));
    for (const invalid of [
      { ...files, [sample]: "{" },
      {
        ...files,
        [sample]: JSON.stringify({
          ...JSON.parse(files[sample]),
          version: 999,
        }),
      },
      { ...files, "../auth.json": "{}" },
      {
        ...files,
        [sample]: JSON.stringify({
          ...JSON.parse(files[sample]),
          attachment: "/outside",
        }),
      },
    ])
      await assert.rejects(a.request("qa_publish", invalid));
    if (files["graph/nodes.jsonl"]) {
      const chatsOnly = Object.fromEntries(
        Object.entries(files).filter(([path]) => path.startsWith("chats/")),
      );
      await assert.rejects(a.request("qa_publish", chatsOnly));
      await assert.rejects(
        a.request("qa_publish", {
          ...files,
          "graph/nodes.jsonl": "{broken}\n",
        }),
      );
      const noChat = { ...files };
      delete noChat[sample];
      await assert.rejects(a.request("qa_publish", noChat));
      await assert.rejects(
        a.request("qa_publish", {
          ...files,
          "graph/chains.jsonl": files["graph/chains.jsonl"].replace(
            /"conclusion":"[^"]+"/,
            '"conclusion":"missing"',
          ),
        }),
      );
    }
    const head = await a.request("qa_capture");
    assert.equal(await a.request("qa_capture"), head);
    if (["A", "C"].includes(source)) await pair(a, b);
    await wait(b, (s) => s.head === head);
    const received = await b.request("qa_read", { head });
    assert.deepEqual(received, files);
    const old = inventory(base(target));
    if (["A", "C"].includes(source)) assert.deepEqual(old, {});
    else
      assert.deepEqual(
        old,
        JSON.parse(readFileSync("/lab/previous-" + target)),
      );
    if (["A", "C"].includes(source))
      writeFileSync("/lab/previous-" + source, JSON.stringify(files), {
        flag: "wx",
      });
    await b.request("qa_stage_received", { head });
    assert.deepEqual(inventory("/lab/" + target + "/received"), files);
    await a.stop();
    await b.stop();
    console.log(
      source,
      "→",
      target,
      Object.keys(files).length,
      "verified content files",
    );
  }
} finally {
  close();
}
