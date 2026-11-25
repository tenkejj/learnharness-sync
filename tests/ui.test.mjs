// Browser UI driven by two real, isolated native QUIC engines. No fake peers or
// success responses are used in these interaction tests.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtemp, mkdir, writeFile, unlink } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "@playwright/test";
import { build, preview } from "vite";
const evidence =
  process.env.HARNESS_SYNC_UI_EVIDENCE ||
  join(tmpdir(), "harnesssync-ux-evidence");
const children = [];
let browser, a, b, pageA, pageB, interval, server, appUrl;
let polling = false;
const errors = [];
async function start(name, root) {
  const home = join(root, name + "-home");
  await mkdir(home, { mode: 0o700 });
  const child = spawn(
    resolve("target/debug/harness-sync-fixture-peer"),
    [
      "--profile",
      join(root, name),
      "--name",
      name,
      "--synthetic-only",
      "--lan",
    ],
    {
      stdio: ["pipe", "pipe", "inherit"],
      env: {
        ...process.env,
        HOME: home,
        XDG_CONFIG_HOME: join(home, "config"),
        XDG_CACHE_HOME: join(home, "cache"),
        XDG_DATA_HOME: join(home, "data"),
        XDG_STATE_HOME: join(home, "state"),
      },
    },
  );
  children.push(child);
  const lines = createInterface({ input: child.stdout })[
    Symbol.asyncIterator
  ]();
  assert.equal(JSON.parse((await lines.next()).value).ready, true);
  let queue = Promise.resolve();
  const calls = [];
  return {
    profile: join(root, name),
    calls,
    request(command, args = {}) {
      const result = queue.then(async () => {
        calls.push(command);
        child.stdin.write(JSON.stringify({ command, args }) + "\n");
        const line = await lines.next();
        if (line.done) throw Error("Native fixture stopped");
        const value = JSON.parse(line.value);
        if (value.error) throw Error(value.error);
        return value.result;
      });
      queue = result.catch(() => {});
      return result;
    },
  };
}
async function bridge(native, overrides = {}) {
  const page = await browser.newPage({
    viewport: { width: 1180, height: 800 },
  });
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.exposeFunction("nativeInvoke", async (command, args) => {
    const override = overrides[command];
    return override ? override(args) : native.request(command, args);
  });
  await page.addInitScript(() => {
    const callbacks = new Set();
    window.publishNativeStatus = (status) =>
      callbacks.forEach((fn) => fn({ payload: status }));
    window.__TAURI__ = {
      core: {
        invoke: (_name, { command, args }) =>
          window.nativeInvoke(command, args),
      },
      event: {
        listen: async (_name, fn) => {
          callbacks.add(fn);
          return () => callbacks.delete(fn);
        },
      },
    };
  });
  await page.goto(appUrl);
  await page.waitForLoadState("networkidle");
  await page.getByRole("navigation").waitFor();
  return page;
}
async function push() {
  if (polling) return;
  polling = true;
  try {
    await Promise.all(
      [
        [a, pageA],
        [b, pageB],
      ]
        .filter(([, p]) => p && !p.isClosed())
        .map(async ([native, page]) => {
          const status = await native.request("status");
          await page.evaluate((s) => window.publishNativeStatus(s), status);
        }),
    );
  } finally {
    polling = false;
  }
}
async function shot(page, name) {
  await page.screenshot({
    path: join(evidence, name + ".png"),
    fullPage: true,
  });
}
async function noOverflow(page) {
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    true,
    "No horizontal page overflow",
  );
}
before(async () => {
  await mkdir(evidence, { recursive: true });
  const root = await mkdtemp(join(tmpdir(), "harnesssync-ui-native-"));
  const buildOptions = { outDir: join(root, "web"), emptyOutDir: true };
  await build({ build: buildOptions, logLevel: "warn" });
  server = await preview({
    build: buildOptions,
    preview: { host: "127.0.0.1", port: 0 },
  });
  appUrl = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({
    executablePath:
      process.env.HARNESS_SYNC_CHROMIUM ||
      (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined),
    headless: true,
  });
  a = await start("Studio desktop", root);
  b = await start("Travel laptop", root);
  pageA = await bridge(a);
  pageB = await bridge(b);
  interval = setInterval(
    () => void push().catch((e) => errors.push(String(e))),
    700,
  );
});
after(async () => {
  clearInterval(interval);
  await browser?.close();
  await server?.close();
  for (const child of children) child.kill();
  await writeFile(
    join(evidence, "browser-errors.json"),
    JSON.stringify(errors, null, 2),
  );
});

test("three destinations, large local typography, honest first launch at three sizes", async () => {
  assert.deepEqual(await pageA.locator("nav button").allTextContents(), [
    "Home",
    "Devices",
    "History",
  ]);
  assert.equal(
    await pageA.evaluate(() => document.fonts.check("16px Manrope")),
    true,
  );
  for (const [width, height] of [
    [1180, 800],
    [900, 680],
    [720, 600],
    [420, 740],
  ]) {
    await pageA.setViewportSize({ width, height });
    await noOverflow(pageA);
    assert.equal(
      await pageA
        .getByRole("button", { name: "Add Device", exact: true })
        .isVisible(),
      true,
    );
    await shot(pageA, `home-empty-${width}`);
  }
  await pageA.setViewportSize({ width: 720, height: 600 });
  await pageA.evaluate(() => {
    document.documentElement.style.fontSize = "32px";
  });
  await noOverflow(pageA);
  assert.equal(
    await pageA
      .getByRole("button", { name: "Settings", exact: true })
      .isVisible(),
    true,
  );
  await shot(pageA, "home-text-scale-200");
  await pageA.evaluate(() => {
    document.documentElement.style.fontSize = "";
  });
  await pageA.setViewportSize({ width: 1180, height: 800 });
  assert.equal(
    await pageA
      .getByText("Connect a managed LearnHarness profile in Settings.", {
        exact: true,
      })
      .isVisible(),
    true,
  );
});

test("discovery opens in one click; full string, mutual approval and automatic Home return", async () => {
  await pageA.getByRole("button", { name: "Add Device", exact: true }).click();
  await pageB.getByRole("button", { name: "Add Device", exact: true }).click();
  const dialogA = pageA.getByRole("dialog");
  await dialogA
    .getByRole("heading", { name: "Nearby computers", exact: true })
    .waitFor();
  await shot(pageA, "discovery");
  assert.equal(
    await dialogA
      .getByText("Invitation from the other computer", { exact: true })
      .count(),
    0,
  );
  await dialogA
    .locator("li")
    .filter({ hasText: "Travel laptop" })
    .getByRole("button", { name: "Pair", exact: true })
    .click();
  await dialogA.locator(".verification").waitFor();
  await pageB.locator(".verification").waitFor();
  const code = await dialogA.locator(".verification").textContent();
  assert.equal(code.trim().split(/\s+/).length, 8);
  assert.equal(code, await pageB.locator(".verification").textContent());
  await shot(pageA, "pairing-verification");
  await dialogA
    .getByRole("button", { name: "Strings match — approve" })
    .click();
  assert.equal(
    (await a.request("status")).transport.peers.length,
    0,
    "One local approval is insufficient",
  );
  await pageB.getByRole("button", { name: "Strings match — approve" }).click();
  await pageA
    .getByRole("heading", { name: "Connected to Travel laptop", exact: true })
    .waitFor({ timeout: 20000 });
  await pageB
    .getByRole("heading", { name: "Connected to Studio desktop", exact: true })
    .waitFor({ timeout: 20000 });
  assert.equal(await pageA.getByRole("dialog").count(), 0);
  await shot(pageA, "home-connected");
});

test("one click generates, transfers, verifies and adopts real test data; double click is guarded", async () => {
  await pageA
    .getByRole("button", { name: "Run sync test", exact: true })
    .evaluate((el) => {
      el.click();
      el.click();
    });
  await pageA
    .getByRole("heading", { name: "Checking your connection", exact: true })
    .waitFor();
  await shot(pageA, "test-running");
  await pageA
    .getByRole("heading", { name: "Your test data is up to date", exact: true })
    .waitFor({ timeout: 45000 });
  assert.equal(a.calls.filter((c) => c === "run_sync_test").length, 1);
  const [sa, sb] = await Promise.all([
    a.request("status"),
    b.request("status"),
  ]);
  assert.equal(sa.sync_test.state, "success");
  assert.equal(sb.head, sa.sync_test.snapshot);
  assert.equal(sa.transport.peers[0].receipt.received, sa.sync_test.snapshot);
  assert.equal(sa.transport.peers[0].receipt.head, sa.sync_test.snapshot);
  assert.ok(sb.transfer.verified_received_objects > 0);
  await shot(pageA, "test-success");
  await pageA.setViewportSize({ width: 720, height: 600 });
  await noOverflow(pageA);
  await shot(pageA, "test-success-720");
  await pageA.setViewportSize({ width: 1180, height: 800 });
});

test("settings gear, progressive diagnostics, keyboard containment, Escape and reduced motion", async () => {
  await pageA.getByRole("button", { name: "Settings", exact: true }).click();
  const dialog = pageA.getByRole("dialog");
  assert.equal(
    await dialog.getByText("Local identity", { exact: true }).count(),
    0,
  );
  await pageA.keyboard.press("Tab");
  assert.equal(
    await pageA.evaluate(() => !!document.activeElement.closest("dialog")),
    true,
  );
  await dialog.getByText("Advanced", { exact: true }).click();
  await dialog
    .getByRole("button", { name: "Diagnostics", exact: true })
    .click();
  await shot(pageA, "settings-advanced");
  await dialog.getByRole("checkbox", { name: /Reduce motion/ }).check();
  assert.equal(
    await pageA.evaluate(() => document.documentElement.dataset.motion),
    "reduced",
  );
  for (let i = 0; i < 20; i++) {
    await pageA.keyboard.press("Tab");
    assert.equal(
      await pageA.evaluate(() => !!document.activeElement.closest("dialog")),
      true,
    );
  }
  await pageA.keyboard.press("Escape");
  assert.equal(await pageA.getByRole("dialog").count(), 0);
  assert.equal(
    await pageA
      .getByRole("button", { name: "Settings", exact: true })
      .evaluate((el) => el === document.activeElement),
    true,
  );
});

test("device management names and confirmation preserve trust when canceled", async () => {
  await pageA
    .getByRole("navigation")
    .getByRole("button", { name: "Devices", exact: true })
    .click();
  await pageA
    .getByRole("heading", { name: "Studio desktop", exact: true })
    .waitFor();
  await pageA
    .getByRole("button", { name: "Manage Travel laptop", exact: true })
    .click();
  await pageA
    .getByRole("button", { name: "Remove Device", exact: true })
    .click();
  await pageA
    .getByRole("heading", { name: "Remove this device?", exact: true })
    .waitFor();
  await pageA.getByRole("button", { name: "Keep device", exact: true }).click();
  await pageA.keyboard.press("Escape");
  assert.equal((await a.request("status")).transport.peers.length, 1);
  await shot(pageA, "devices");
});

test("receiver refusal produces a genuine failed result and remains recoverable", async () => {
  await pageA
    .getByRole("navigation")
    .getByRole("button", { name: "Home", exact: true })
    .click();
  await writeFile(
    join(b.profile, "WRITER_ACTIVE"),
    "Isolated UI refusal check",
  );
  await pageA
    .getByRole("button", { name: "Run sync test", exact: true })
    .click();
  await pageA
    .getByText("The sync test didn’t finish", { exact: true })
    .waitFor({ timeout: 45000 });
  assert.equal((await a.request("status")).sync_test.state, "failed");
  await shot(pageA, "test-failed");
  await unlink(join(b.profile, "WRITER_ACTIVE"));
  // Automatic retries may already have verified the formerly refused revision
  // before the next UI event. Both Connected and Up to date are valid only with
  // a real connected native peer; do not demand the transient heading.
  await pageA
    .locator(".connection-map.is-connected")
    .waitFor({ timeout: 20000 });
  assert.equal((await a.request("status")).transport.peers[0].connected, true);
});

test("offline state never reports current test success; production operations remain blocked", async () => {
  await pageA
    .getByRole("navigation")
    .getByRole("button", { name: "Home", exact: true })
    .click();
  // Removing on B disconnects A. A retains its trusted, now-offline computer.
  await b.request("revoke", {
    device: (await a.request("status")).transport.device_id,
  });
  await push();
  await pageA
    .getByRole("heading", { name: "Travel laptop is offline", exact: true })
    .waitFor({ timeout: 12000 });
  assert.equal(
    await pageA
      .getByRole("button", { name: "Run sync test", exact: true })
      .count(),
    0,
  );
  await shot(pageA, "home-offline");
  await assert.rejects(a.request("production_import"), /disabled/i);
  await assert.rejects(a.request("production_export"), /disabled/i);
  assert.deepEqual(errors, []);
});

test("divergent histories stay preserved and Home leads directly to conflict review", async () => {
  await a.request("revoke", {
    device: (await b.request("status")).transport.device_id,
  });
  const left = await a.request("synthetic_edit", {
    text: "Invented desktop divergent version",
  });
  const right = await b.request("synthetic_edit", {
    text: "Invented laptop divergent version",
  });
  await push();
  await pageA.getByRole("button", { name: "Add Device", exact: true }).click();
  await pageB.getByRole("button", { name: "Add Device", exact: true }).click();
  await pageA
    .getByRole("dialog")
    .locator("li")
    .filter({ hasText: "Travel laptop" })
    .getByRole("button", { name: "Pair", exact: true })
    .click();
  try {
    await pageA.locator(".verification").waitFor({ timeout: 12000 });
  } catch (error) {
    await shot(pageA, "conflict-pairing-failure-a");
    await shot(pageB, "conflict-pairing-failure-b");
    await writeFile(
      join(evidence, "conflict-pairing-debug.json"),
      JSON.stringify(
        await Promise.all([a.request("status"), b.request("status")]),
        null,
        2,
      ),
    );
    throw error;
  }
  await pageA.setViewportSize({ width: 720, height: 600 });
  await noOverflow(pageA);
  await shot(pageA, "pairing-verification-720");
  await pageA.getByRole("button", { name: "Strings match — approve" }).click();
  await pageB.getByRole("button", { name: "Strings match — approve" }).click();
  await pageA
    .getByRole("heading", { name: "Two versions need attention", exact: true })
    .waitFor({ timeout: 20000 });
  assert.equal((await a.request("status")).head, left.snapshot);
  assert.equal((await b.request("status")).head, right.snapshot);
  await shot(pageA, "home-conflict-720");
  await pageA
    .getByRole("button", { name: "Review conflict", exact: true })
    .click();
  await pageA
    .getByText("Current version on this computer", { exact: true })
    .click();
  await shot(pageA, "conflict-review-720");
  await pageA.setViewportSize({ width: 1180, height: 800 });
  await shot(pageA, "conflict-review");
  assert.deepEqual(errors, []);
});

test("managed apply requires explicit consent and reports application readback separately", async () => {
  const applied = [];
  const managedPage = await bridge(a, {
    settings: async () => ({
      version: 2,
      name: "Studio desktop",
      production_mutation: false,
      installation: "LearnHarness 0.6.1 verified",
      compatible: true,
      profile: "/managed/test",
      detected: true,
    }),
    managed_status: async () => "saved",
    status: async () => ({
      ...(await a.request("status")),
      detected: true,
      managed_exports: [{ revision: "a".repeat(64), files: 6, bytes: 4932 }],
    }),
    managed_apply: async (args) => {
      applied.push(args);
      return { state: "awaiting_readback", applied: true };
    },
  });
  try {
    await managedPage
      .getByRole("button", { name: "Settings", exact: true })
      .click();
    await managedPage
      .getByRole("heading", { name: "Managed LearnHarness", exact: true })
      .waitFor();
    await managedPage
      .getByRole("heading", { name: "LearnHarness compatibility" })
      .waitFor();
    await managedPage
      .getByText("LearnHarness 0.6.1 verified", { exact: true })
      .waitFor();
    const apply = managedPage.getByRole("button", {
      name: "Apply verified revision",
      exact: true,
    });
    assert.equal(
      await apply.isDisabled(),
      true,
      "Real-data apply must start unchecked",
    );
    await managedPage
      .getByRole("checkbox", {
        name: /I authorize applying this verified revision/,
      })
      .check();
    await apply.click();
    await managedPage
      .getByText(/managed profile is updated and backed up/i)
      .waitFor();
    assert.deepEqual(applied, [{ revision: "a".repeat(64), confirmed: true }]);
    assert.equal(
      await managedPage
        .getByRole("button", { name: "Apply verified revision", exact: true })
        .count(),
      0,
    );
  } finally {
    await managedPage.close();
  }
});
