import { useEffect, useState } from "react";
import {
  Bug,
  ChevronDown,
  FlaskConical,
  Info,
  Network,
  Save,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import { command, loadSettings, type Configuration, type Status } from "./api";
import { ManagedSession } from "./ManagedSession";
import { ErrorNotice } from "./Controls";
import { useOperation } from "./useEngine";
function countLabel(value: number, noun: string) {
  return `${value} ${noun}${value === 1 ? "" : "s"}`;
}
function BackupInspection() {
  const [path, setPath] = useState("");
  const [result, setResult] = useState("");
  const { busy, error, run } = useOperation();
  return (
    <section>
      <h3>Check a LearnHarness backup</h3>
      <p>
        Inspect the 0.6.1 record format locally. Choose a read-only Btrfs backup
        containing LearnHarness data. Only counts appear here.
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setResult("");
          void run(async () => {
            const counts = await command<Record<string, number>>(
              "inspect_learnharness_backup",
              { path },
            );
            if (
              !counts ||
              !["sessions", "propositions", "chains"].every(
                (key) => Number.isSafeInteger(counts[key]) && counts[key] >= 0,
              )
            )
              throw new Error("The inspection result could not be verified.");
            setResult(
              `Matches the tested format. Found ${countLabel(counts.sessions, "session")}, ${countLabel(counts.propositions, "proposition")} and ${countLabel(counts.chains, "chain")}.`,
            );
          });
        }}
      >
        <label>
          Backup root
          <input
            required
            maxLength={4096}
            value={path}
            disabled={busy}
            onChange={(event) => {
              setPath(event.target.value);
              setResult("");
            }}
          />
        </label>
        <button
          className="secondary"
          type="submit"
          disabled={busy || !path.startsWith("/")}
        >
          {busy ? "Inspecting…" : "Inspect backup"}
        </button>
      </form>
      <ErrorNotice error={error} />
      <p role="status">{result}</p>
    </section>
  );
}
function Developer({ changed }: { changed: () => Promise<void> }) {
  const [text, setText] = useState("");
  const [message, setMessage] = useState("");
  const { busy, error, run } = useOperation();
  return (
    <section className="developer-panel">
      <h3>
        <FlaskConical size={22} aria-hidden="true" />
        Manual test fixture
      </h3>
      <p>
        Invented notes only. Saving creates a revision shared automatically with
        trusted online devices. Separate edits can create a preserved conflict.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            await command("initialize_synthetic", { confirmed: true });
            await command("synthetic_edit", { text });
            setMessage(
              "Test revision saved. Peer verification is reported separately on Home.",
            );
            await changed();
          });
        }}
      >
        <label>
          Invented note
          <textarea
            rows={3}
            maxLength={65536}
            required
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </label>
        <button disabled={busy || !text.trim()} type="submit">
          <Save aria-hidden="true" />
          Save test revision
        </button>
      </form>
      <ErrorNotice error={error} />
      <p role="status">{message}</p>
    </section>
  );
}
export function SettingsContent({
  data,
  changed,
  reduced,
  setReduced,
}: {
  data: Status;
  changed: () => Promise<void>;
  reduced: boolean;
  setReduced: (v: boolean) => void;
}) {
  const [config, setConfig] = useState<Configuration | null>(null);
  const [loadError, setLoadError] = useState("");
  const [message, setMessage] = useState("");
  const [developer, setDeveloper] = useState(false);
  const [diagnostics, setDiagnostics] = useState(false);
  const { busy, error, run } = useOperation();
  useEffect(() => {
    let active = true;
    loadSettings()
      .then((v) => {
        if (active) setConfig(v);
      })
      .catch((e) => {
        if (active)
          setLoadError(e instanceof Error ? e.message : "Settings unavailable");
      });
    return () => {
      active = false;
    };
  }, []);
  return (
    <div className="settings-content">
      <section>
        <h3>This computer</h3>
        <ErrorNotice error={error || loadError} id="settings-name-error" />
        {config ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await command("save_settings", { name: config.name.trim() });
                setMessage(
                  "Device name saved. Restart LearnHarness Sync to update the nearby discovery name.",
                );
                await changed();
              });
            }}
          >
            <label>
              Device name
              <input
                required
                maxLength={80}
                aria-describedby={error ? "settings-name-error" : undefined}
                aria-invalid={!!error}
                value={config.name}
                onChange={(e) => setConfig({ ...config, name: e.target.value })}
              />
            </label>
            <button
              className="secondary"
              type="submit"
              disabled={busy || !config.name.trim()}
            >
              <Save aria-hidden="true" />
              Save name
            </button>
          </form>
        ) : (
          !loadError && <p role="status">Loading device name…</p>
        )}
        <p role="status">{message}</p>
      </section>
      <section>
        <h3>Appearance & accessibility</h3>
        <label className="check-row">
          <input
            type="checkbox"
            checked={reduced}
            onChange={(e) => setReduced(e.target.checked)}
          />
          <span>
            <strong>Reduce motion</strong>
            <small>
              Your system’s reduced-motion preference is always respected.
            </small>
          </span>
        </label>
      </section>
      <details className="advanced">
        <summary>
          <SlidersHorizontal aria-hidden="true" />
          <span>Advanced</span>
          <ChevronDown aria-hidden="true" />
        </summary>
        <div>
          <p className="secondary-copy">
            Connection details and tools for troubleshooting isolated test data.
          </p>
          <button
            className="secondary"
            aria-expanded={diagnostics}
            aria-controls="diagnostics"
            onClick={() => setDiagnostics((v) => !v)}
          >
            <Network aria-hidden="true" />
            Diagnostics
          </button>
          {diagnostics && (
            <section id="diagnostics">
              <dl className="revision-details">
                <dt>Local identity</dt>
                <dd>
                  <code>{data.transport.device_id}</code>
                </dd>
                <dt>Local test storage</dt>
                <dd>
                  <code>{data.data_path}</code>
                </dd>
                <dt>Listening port</dt>
                <dd>{data.listen_port}</dd>
                <dt>Discovery</dt>
                <dd>{data.transport.message}</dd>
                <dt>Verified received</dt>
                <dd>
                  {data.transfer.verified_received_objects} objects ·{" "}
                  {data.transfer.verified_received_bytes.toLocaleString()} bytes
                </dd>
              </dl>
              <pre>{JSON.stringify(data.diagnostics, null, 2)}</pre>
              <ul className="limitations">
                {data.limitations.map((v) => (
                  <li key={v}>{v}</li>
                ))}
              </ul>
            </section>
          )}
          <label className="check-row">
            <input
              type="checkbox"
              checked={developer}
              onChange={(e) => setDeveloper(e.target.checked)}
            />
            <span>
              <strong>
                <Bug size={20} aria-hidden="true" />
                Developer Mode
              </strong>
              <small>
                Show manual fixture editing. Uses invented data only.
              </small>
            </span>
          </label>
          {developer && <Developer changed={changed} />}
          <BackupInspection />
        </div>
      </details>
      {data.detected && <ManagedSession />}
      <section className="about">
        <h3>
          <Info size={21} aria-hidden="true" />
          About LearnHarness Sync
        </h3>
        <p>
          Version 0.2.0-alpha.1 · Linux experimental candidate
          <br />
          LearnHarness Sync is an independent, unofficial companion application
          for LearnHarness. It is not affiliated with or endorsed by the
          LearnHarness developers.
        </p>
        {config && <p>{config.installation}</p>}
        <p>
          <ShieldCheck size={20} aria-hidden="true" />
          Synchronization is limited to the tested LearnHarness 0.6.1 managed
          profile.
        </p>
        <p className="secondary-copy">
          Manrope by The Manrope Project Authors · SIL Open Font License 1.1.
          Icons by Lucide · ISC license.
        </p>
      </section>
    </div>
  );
}
