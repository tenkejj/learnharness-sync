// Two independent Rust processes, real loopback QUIC, persistent profiles.
import { spawn } from "node:child_process";

import { join } from "node:path";
import { createInterface } from "node:readline";
import assert from "node:assert/strict";
export function peerLab(root, binary) {
  const children = [];
  async function start(name) {
    const child = spawn(
      binary,
      ["--profile", join(root, name), "--name", name, "--synthetic-only"],
      { stdio: ["pipe", "pipe", "inherit"] },
    );
    children.push(child);
    const lines = createInterface({ input: child.stdout })[
      Symbol.asyncIterator
    ]();
    const next = async () => {
      const timer = setTimeout(() => child.kill(), 20000);
      try {
        const value = await lines.next();
        if (value.done) throw Error(`${name} stopped unexpectedly`);
        return JSON.parse(value.value);
      } finally {
        clearTimeout(timer);
      }
    };
    const ready = await next();
    assert.equal(ready.ready, true);
    return {
      identity: ready.identity,
      async request(command, args = {}) {
        child.stdin.write(JSON.stringify({ command, args }) + "\n");
        const response = await next();
        if (response.error) throw Error(response.error);
        return response.result;
      },
      async stop() {
        child.stdin.end(JSON.stringify({ command: "exit" }) + "\n");
        await new Promise((r) => child.once("exit", r));
      },
    };
  }
  async function wait(peer, predicate) {
    for (let i = 0; i < 120; i++) {
      const s = await peer.request("status");
      if (predicate(s)) return s;
      await new Promise((r) => setTimeout(r, 200));
    }
    throw Error("Timed out waiting for peer state");
  }
  async function pair(a, b) {
    await a.request("open_pairing");
    await b.request("open_pairing");
    const { invitation } = await b.request("invitation");
    // Loopback tests choose the known loopback socket, rather than an interface hint.
    const value = JSON.parse(Buffer.from(invitation, "base64").toString());
    value.addresses = [`127.0.0.1:${(await b.request("status")).listen_port}`];
    await a.request("start_pairing", {
      invitation: Buffer.from(JSON.stringify(value)).toString("base64"),
    });
    const pa = (await wait(a, (s) => s.pending.length)).pending[0];
    const pb = (await wait(b, (s) => s.pending.length)).pending[0];
    assert.equal(pa.code, pb.code);
    await a.request("pairing_decision", { id: pa.id, accept: true });
    await b.request("pairing_decision", { id: pb.id, accept: true });
    await wait(a, (s) => s.transport.peers.length === 1);
    await wait(b, (s) => s.transport.peers.length === 1);
  }
  return {
    start,
    wait,
    pair,
    close() {
      for (const child of children) if (child.exitCode === null) child.kill();
    },
  };
}
