import { useEffect, useState } from "react";
import { command, type ManagedSnapshot } from "./api";
import { ErrorNotice } from "./Controls";
import { useOperation } from "./useEngine";

export function ManagedSession() {
  const [state, setState] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [statusError, setStatusError] = useState("");
  const [snapshot, setSnapshot] = useState<ManagedSnapshot | null>(null);
  const [peers, setPeers] = useState<{ id: string; name: string }[]>([]);
  const [peer, setPeer] = useState("");
  const [exportConsent, setExportConsent] = useState(false);
  const [exportReceipt, setExportReceipt] = useState<{
    files: number;
    bytes: number;
    applied: false;
  } | null>(null);
  const [staged, setStaged] = useState<
    { revision: string; files: number; bytes: number }[]
  >([]);
  const { busy, error, run } = useOperation();
  const idle = state === "saved" || state === "verified";
  useEffect(() => {
    let live = true;
    async function refresh() {
      try {
        const value = await command<string>("managed_status");
        const status = await command<{
          transport?: {
            peers?: { id: string; name: string; trusted?: boolean }[];
          };
          managed_exports?: {
            revision: string;
            files: number;
            bytes: number;
          }[];
        }>("status");
        const paired = (status.transport?.peers ?? [])
          .filter((p) => p.trusted !== false)
          .map(({ id, name }) => ({ id, name }));
        if (live) {
          setState(value);
          setPeers(paired);
          setStaged(status.managed_exports ?? []);
          setPeer((current) =>
            paired.some((p) => p.id === current)
              ? current
              : (paired[0]?.id ?? ""),
          );
          setStatusError("");
        }
      } catch (e) {
        if (live) setStatusError(String(e));
      }
    }
    void refresh();
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, []);
  return (
    <section>
      <h3>Managed LearnHarness</h3>
      <p>
        Open an isolated, offline LearnHarness window. Each clean exit saves a
        local generation. Your ordinary profile stays separate.
      </p>
      <p role="status">
        {state === "active"
          ? "Waiting for LearnHarness to close."
          : state === "recovery"
            ? "Recovery required. The interrupted session will be retained separately."
            : state === "saved"
              ? "Managed session saved locally."
              : state === "awaiting-readback"
                ? "Received data is applied to the managed profile. Open LearnHarness and close it cleanly to verify the application readback."
                : state === "verified"
                  ? "LearnHarness reopened and verified the applied conversation and graph data."
                  : ""}
      </p>
      {state === "recovery" && (
        <label className="check-row">
          <input
            type="checkbox"
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
          Resume the last clean generation, or an empty profile if this was the
          first session.
        </label>
      )}
      <button
        disabled={
          busy ||
          !!statusError ||
          !state ||
          state === "active" ||
          (state === "recovery" && !confirmed)
        }
        onClick={() =>
          void run(async () => {
            if (state === "recovery") {
              await command("managed_recover", { confirmed });
              setConfirmed(false);
            } else {
              await command("managed_launch");
            }
            setState(await command<string>("managed_status"));
          })
        }
      >
        {state === "recovery"
          ? "Recover managed session"
          : "Open managed LearnHarness"}
      </button>
      {idle && (
        <button
          disabled={busy || !!statusError}
          onClick={() =>
            void run(async () => {
              setSnapshot(await command<ManagedSnapshot>("managed_snapshot"));
            })
          }
        >
          Check managed snapshot
        </button>
      )}
      {idle && peers.length > 0 && (
        <div className="managed-export">
          <label htmlFor="managed-export-peer">Send a verified copy to</label>
          <select
            id="managed-export-peer"
            value={peer}
            onChange={(e) => {
              setPeer(e.target.value);
              setExportReceipt(null);
            }}
            disabled={busy}
          >
            {peers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <label className="check-row">
            <input
              type="checkbox"
              checked={exportConsent}
              onChange={(e) => setExportConsent(e.target.checked)}
            />
            I authorize sending my managed conversations and supported knowledge
            graph records to this paired device. The receiver will stage a copy;
            it will not be applied automatically.
          </label>
          <button
            disabled={busy || !peer || !exportConsent || !!statusError}
            onClick={() =>
              void run(async () => {
                const result = await command<{
                  files: number;
                  bytes: number;
                  applied: false;
                }>("managed_export", { device: peer, consent: true });
                setExportReceipt(result);
                setExportConsent(false);
              })
            }
          >
            Send verified copy
          </button>
          {exportReceipt && (
            <p role="status">
              Transfer verified and staged on the paired device (
              {exportReceipt.files} files,{" "}
              {exportReceipt.bytes.toLocaleString()} bytes). This is not
              application-level synchronization; nothing was applied.
            </p>
          )}
        </div>
      )}
      {staged.length > 0 && (
        <div className="managed-export">
          <h4>Received LearnHarness data</h4>
          <p>
            Received revisions remain separate until you explicitly apply one.
            Existing local history is retained and merged; conflicts are refused
            for review.
          </p>
          <p role="status">
            Real LearnHarness apply is disabled pending packaged two-device
            transaction qualification. Received data remains quarantined and
            your managed profile is unchanged.
          </p>
          {staged.map((revision) => (
            <div className="managed-staged" key={revision.revision}>
              <p>
                {revision.files} files · {revision.bytes.toLocaleString()} bytes
                · {revision.revision.slice(0, 12)}
              </p>
            </div>
          ))}
        </div>
      )}
      {snapshot && (
        <p role="status">
          Verified local LearnHarness snapshot: {snapshot.files} files,{" "}
          {snapshot.bytes} bytes.
        </p>
      )}
      <ErrorNotice error={error || statusError} />
    </section>
  );
}
