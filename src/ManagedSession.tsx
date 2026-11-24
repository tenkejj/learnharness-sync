import { useEffect, useState } from "react";
import { command, type ManagedSnapshot } from "./api";
import { ErrorNotice } from "./Controls";
import { useOperation } from "./useEngine";

export function ManagedSession() {
  const [state, setState] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [statusError, setStatusError] = useState("");
  const [snapshot, setSnapshot] = useState<ManagedSnapshot | null>(null);
  const { busy, error, run } = useOperation();
  useEffect(() => {
    let live = true;
    async function refresh() {
      try {
        const value = await command<string>("managed_status");
        if (live) {
          setState(value);
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
      {state === "saved" && (
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
      {snapshot && (
        <p role="status">
          Verified local LearnHarness snapshot: {snapshot.files} files, {snapshot.bytes} bytes.
        </p>
      )}
      <ErrorNotice error={error || statusError} />
    </section>
  );
}
