export type Snapshot = {
  id: string;
  device: string;
  time: number;
  files: number;
  bytes: number;
  format: string;
  verified: boolean;
  applied: boolean;
};
export type Configuration = {
  version: number;
  name: string;
  profile: string;
  installation: string;
  production_mutation: false;
};
export type ManagedSnapshot = {
  format: string;
  digest: string;
  files: number;
  bytes: number;
  manifest: { path: string; bytes: number; sha256: string }[];
};
export type EngineCommand =
  | "managed_status"
  | "managed_launch"
  | "managed_recover"
  | "managed_snapshot"
  | "managed_export"
  | "inspect_revision"
  | "inspect_learnharness_backup"
  | "run_sync_test"
  | "close_pairing"
  | "rename_peer"
  | "status"
  | "settings"
  | "save_settings"
  | "open_pairing"
  | "invitation"
  | "start_pairing"
  | "pairing_decision"
  | "revoke"
  | "disconnect"
  | "initialize_synthetic"
  | "synthetic_edit"
  | "export"
  | "apply"
  | "restore"
  | "production_export"
  | "production_import"
  | "diagnostics";
declare global {
  interface Window {
    __TAURI__?: {
      core: {
        invoke: <T>(name: string, args: Record<string, unknown>) => Promise<T>;
      };
      event?: {
        listen: <T>(
          name: string,
          callback: (event: { payload: T }) => void,
        ) => Promise<() => void>;
      };
    };
  }
}
export async function command<T = unknown>(
  name: EngineCommand,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (typeof window === "undefined" || !window.__TAURI__)
    throw new Error(
      "The local engine could not be reached. Start the LearnHarness Sync desktop app. Browser previews have no synchronization engine.",
    );
  if (JSON.stringify(args).length > 96 * 1024)
    throw new Error(
      "Command exceeds the local size limit; no command was sent.",
    );
  try {
    return await window.__TAURI__.core.invoke<T>("engine_request", {
      command: name,
      args,
    });
  } catch (error) {
    throw new Error(
      typeof error === "string"
        ? error
        : "The engine response could not be verified. A requested change may already have completed. Refresh local status before retrying.",
    );
  }
}
export async function loadSettings(): Promise<Configuration> {
  const value = await command<Configuration>("settings");
  if (
    !value ||
    value.version !== 2 ||
    typeof value.name !== "string" ||
    typeof value.profile !== "string" ||
    typeof value.installation !== "string" ||
    value.production_mutation !== false
  )
    throw new Error("Unsupported local settings");
  return value;
}
export type SyncTest = {
  state: "pending" | "success" | "failed" | "conflict";
  device: string;
  name: string;
  snapshot: string;
  started: number;
  finished: number | null;
  message: string;
};
export type Receipt = {
  head: string | null;
  received: string | null;
  conflict: boolean;
};
export type Peer = {
  id: string;
  name: string;
  connected: boolean;
  last_sync: number | null;
  error: string | null;
  receipt: Receipt | null;
};
export type Status = {
  sync_test: SyncTest | null;
  mode: "monitor" | "synthetic";
  state: string;
  device: string;
  data_path: string;
  detected: boolean;
  head: string | null;
  snapshots: Snapshot[];
  backups: Snapshot[];
  events: { time: number; kind: string }[];
  conflicts: string[];
  automatic: boolean;
  integration: string;
  transport_installed: boolean;
  transport: {
    state: string;
    device_id: string;
    peers: Peer[];
    message: string;
  };
  discovered: { id: string; name: string; address: string }[];
  pending: { id: string; peer_id: string; name: string; code: string }[];
  pairing_open: boolean;
  listen_port: number;
  diagnostics: Record<string, string>;
  transfer: {
    verified_received_bytes: number;
    verified_received_objects: number;
  };
  limitations: string[];
};
const object = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const string = (v: unknown): v is string => typeof v === "string";
const number = (v: unknown): v is number =>
  typeof v === "number" && Number.isFinite(v) && v >= 0;
const boolean = (v: unknown): v is boolean => typeof v === "boolean";
const array = (v: unknown, check: (x: unknown) => boolean): boolean =>
  Array.isArray(v) && v.length <= 4096 && v.every(check);
const snapshot = (v: unknown): boolean =>
  object(v) &&
  ["id", "device", "format"].every((k) => string(v[k])) &&
  ["time", "files", "bytes"].every((k) => number(v[k])) &&
  v.verified === true &&
  boolean(v.applied);
export function isStatus(v: unknown): v is Status {
  if (!object(v) || !object(v.transport)) return false;
  const t = v.transport;
  return (
    (v.mode === "monitor" || v.mode === "synthetic") &&
    ["state", "device", "data_path", "integration"].every((k) =>
      string(v[k]),
    ) &&
    ["detected", "automatic", "transport_installed", "pairing_open"].every(
      (k) => boolean(v[k]),
    ) &&
    (v.head === null || string(v.head)) &&
    (v.sync_test === null || isSyncTest(v.sync_test)) &&
    array(v.snapshots, snapshot) &&
    array(v.backups, snapshot) &&
    array(v.events, (x) => object(x) && number(x.time) && string(x.kind)) &&
    array(v.conflicts, string) &&
    array(v.limitations, string) &&
    number(v.listen_port) &&
    object(v.transfer) &&
    number(v.transfer.verified_received_bytes) &&
    number(v.transfer.verified_received_objects) &&
    object(v.diagnostics) &&
    Object.values(v.diagnostics).every(string) &&
    string(t.state) &&
    string(t.message) &&
    string(t.device_id) &&
    array(
      t.peers,
      (x) =>
        object(x) &&
        string(x.id) &&
        string(x.name) &&
        boolean(x.connected) &&
        (x.last_sync === null || number(x.last_sync)) &&
        (x.error === null || string(x.error)) &&
        (x.receipt === null || isReceipt(x.receipt)),
    ) &&
    array(
      v.discovered,
      (x) => object(x) && string(x.id) && string(x.name) && string(x.address),
    ) &&
    array(
      v.pending,
      (x) =>
        object(x) &&
        ["id", "peer_id", "name", "code"].every((k) => string(x[k])),
    )
  );
}
export async function loadStatus(signal?: AbortSignal): Promise<Status> {
  const value: unknown = await command("status");
  if (signal?.aborted) throw new Error("Status request cancelled");
  if (!isStatus(value))
    throw new Error(
      "Unexpected engine response. Restart the desktop app and check diagnostics.",
    );
  return value;
}
export async function subscribeStatus(
  callback: (status: Status) => void,
): Promise<() => void> {
  if (!window.__TAURI__?.event) return () => {};
  return window.__TAURI__.event.listen<unknown>("engine-status", (event) => {
    if (isStatus(event.payload)) callback(event.payload);
  });
}
export const short = (id: string | null | undefined) =>
  id ? id.slice(0, 12) : "No revision applied";
export const when = (time: number) =>
  time > 0 ? new Date(time * 1000).toLocaleString() : "Not recorded";

export function isReceipt(v: unknown): v is Receipt {
  return (
    object(v) &&
    (v.head === null || string(v.head)) &&
    (v.received === null || string(v.received)) &&
    boolean(v.conflict)
  );
}
export function isSyncTest(v: unknown): v is SyncTest {
  return (
    object(v) &&
    ["pending", "success", "failed", "conflict"].includes(String(v.state)) &&
    ["device", "name", "snapshot", "message"].every((k) => string(v[k])) &&
    number(v.started) &&
    (v.finished === null || number(v.finished))
  );
}
export async function runSyncTest(device: string): Promise<SyncTest> {
  const value = await command("run_sync_test", { device });
  if (!isSyncTest(value))
    throw new Error(
      "The test result could not be verified. Check device status before retrying.",
    );
  return value;
}

export async function inspectRevision(snapshot: string): Promise<string> {
  const result: unknown = await command("inspect_revision", { snapshot });
  if (
    !object(result) ||
    typeof result.text !== "string" ||
    result.text.length > 65536
  )
    throw new Error("The preserved note could not be verified.");
  return result.text;
}
