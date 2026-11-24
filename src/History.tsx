import { useEffect, useState } from "react";
import {
  Archive,
  Check,
  ChevronDown,
  CircleAlert,
  FileCheck2,
  History as HistoryIcon,
  RotateCcw,
} from "lucide-react";
import {
  command,
  inspectRevision,
  when,
  type Snapshot,
  type Status,
} from "./api";
import { ErrorNotice, Modal } from "./Controls";
import { eventTitle, keyedEvents } from "./presentation";
import { useOperation } from "./useEngine";
function RevisionDetails({ item }: { item: Snapshot }) {
  return (
    <dl className="revision-details">
      <dt>Created on</dt>
      <dd>{item.device}</dd>
      <dt>Integrity</dt>
      <dd>All files verified</dd>
      <dt>Contents</dt>
      <dd>
        {item.files} files · {item.bytes.toLocaleString()} bytes
      </dd>
      <dt>Revision</dt>
      <dd>
        <code>{item.id}</code>
      </dd>
    </dl>
  );
}
function RevisionContent({ item }: { item: Snapshot }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    inspectRevision(item.id)
      .then((value) => {
        if (active) setText(value);
      })
      .catch((e) => {
        if (active)
          setError(
            e instanceof Error ? e.message : "The note could not be read.",
          );
      });
    return () => {
      active = false;
    };
  }, [item.id]);
  return (
    <div className="revision-content">
      <h3>Preserved test note</h3>
      <ErrorNotice error={error} />
      {text === null ? (
        !error && <p role="status">Reading the verified note…</p>
      ) : (
        <pre className="note-preview">{text || "Empty test note"}</pre>
      )}
      <details className="revision-technical">
        <summary>
          Technical details
          <ChevronDown size={18} aria-hidden="true" />
        </summary>
        <RevisionDetails item={item} />
      </details>
    </div>
  );
}
function PreservedVersion({
  item,
  current,
}: {
  item: Snapshot;
  current: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>
        <FileCheck2 aria-hidden="true" />
        <span>
          {current
            ? "Current version on this computer"
            : "Preserved version from the other computer"}
        </span>
        <ChevronDown aria-hidden="true" />
      </summary>
      {open && <RevisionContent item={item} />}
    </details>
  );
}
function BackupEntry({
  item,
  number,
  restore,
}: {
  item: Snapshot;
  number: number;
  restore: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details
      className="history-entry"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>
        <Archive aria-hidden="true" />
        <span>
          <strong>Preserved test backup {number}</strong>
          <small>From {item.device} · verified and available to restore</small>
        </span>
        <ChevronDown aria-hidden="true" />
      </summary>
      {open && <RevisionContent item={item} />}
      <button className="secondary" onClick={restore}>
        <RotateCcw aria-hidden="true" />
        Restore this backup
      </button>
    </details>
  );
}
function Restore({
  item,
  close,
  changed,
}: {
  item: Snapshot;
  close: () => void;
  changed: () => Promise<void>;
}) {
  const [phrase, setPhrase] = useState("");
  const { busy, error, run } = useOperation();
  return (
    <Modal title="Restore test backup" onClose={close}>
      <p>
        This creates a new test revision from the backup. The current version is
        backed up first. Real LearnHarness data is never changed.
      </p>
      <ErrorNotice error={error} />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            await command("restore", {
              snapshot: item.id,
              confirmation: phrase,
            });
            await changed();
            close();
          });
        }}
      >
        <label>
          Type RESTORE SYNTHETIC BACKUP
          <input
            required
            value={phrase}
            onChange={(e) => setPhrase(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <button
          className="primary"
          type="submit"
          disabled={busy || phrase !== "RESTORE SYNTHETIC BACKUP"}
        >
          <RotateCcw aria-hidden="true" />
          Restore backup
        </button>
      </form>
    </Modal>
  );
}
export function History({
  data,
  changed,
}: {
  data: Status;
  changed: () => Promise<void>;
}) {
  const [restore, setRestore] = useState<Snapshot | null>(null);
  return (
    <>
      <div className="page-heading">
        <div>
          <h1>History & recovery</h1>
          <p>Verified exchanges, preserved versions, and a way back.</p>
        </div>
      </div>
      {data.conflicts.length > 0 && (
        <section
          className="conflict-section"
          aria-labelledby="conflict-heading"
        >
          <div className="section-heading">
            <CircleAlert size={28} aria-hidden="true" />
            <h2 id="conflict-heading">Two versions need attention</h2>
          </div>
          <p>
            Your computers changed separately. Both versions are preserved;
            automatic replacement is blocked. Compare the details below. Merging
            these test histories is not yet supported.
          </p>
          <div className="version-list">
            {[data.head, ...data.conflicts]
              .filter((v): v is string => !!v)
              .map((id, index) => {
                const item = data.snapshots.find((s) => s.id === id);
                return item ? (
                  <PreservedVersion
                    key={id}
                    item={item}
                    current={index === 0}
                  />
                ) : (
                  <p key={id}>
                    A preserved version is unavailable. Check Advanced
                    diagnostics.
                  </p>
                );
              })}
          </div>
        </section>
      )}
      {data.sync_test && (
        <section className="history-section">
          <h2>Latest sync test</h2>
          <details className="history-entry">
            <summary>
              {data.sync_test.state === "success" ? (
                <Check aria-hidden="true" />
              ) : (
                <CircleAlert aria-hidden="true" />
              )}
              <span>
                <strong>
                  {data.sync_test.state === "success"
                    ? `Synced test data with ${data.sync_test.name}`
                    : data.sync_test.state === "conflict"
                      ? "Two versions need your attention"
                      : data.sync_test.state === "failed"
                        ? "Sync test interrupted"
                        : "Test confirmation pending"}
                </strong>
                <small>
                  {when(data.sync_test.finished ?? data.sync_test.started)}
                </small>
              </span>
              <ChevronDown aria-hidden="true" />
            </summary>
            <p>{data.sync_test.message}</p>
            <dl className="revision-details">
              <dt>Expected test revision</dt>
              <dd>
                <code>{data.sync_test.snapshot}</code>
              </dd>
            </dl>
          </details>
        </section>
      )}
      <section className="history-section">
        <h2>Activity</h2>
        {data.events.length ? (
          <div className="history-list">
            {keyedEvents(data.events).map((event) => (
              <details className="history-entry" key={event.key}>
                <summary>
                  <HistoryIcon aria-hidden="true" />
                  <span>
                    <strong>{eventTitle(event.kind)}</strong>
                    <small>{when(event.time)}</small>
                  </span>
                  <ChevronDown aria-hidden="true" />
                </summary>
                <p>{event.kind}</p>
              </details>
            ))}
          </div>
        ) : (
          <div className="empty-state compact">
            <HistoryIcon size={36} aria-hidden="true" />
            <h3>Your story starts with a connection</h3>
            <p>Verified test exchanges and recovery events will appear here.</p>
          </div>
        )}
      </section>
      <section className="history-section">
        <div className="section-heading">
          <Archive size={26} aria-hidden="true" />
          <h2>Test backups</h2>
        </div>
        {data.backups.length ? (
          <div className="history-list">
            {data.backups.map((b, i) => (
              <BackupEntry
                key={b.id}
                item={b}
                number={i + 1}
                restore={() => setRestore(b)}
              />
            ))}
          </div>
        ) : (
          <p className="secondary-copy">
            When a received version replaces older test data, the previous
            version is backed up here.
          </p>
        )}
      </section>
      {restore && (
        <Restore
          item={restore}
          close={() => setRestore(null)}
          changed={changed}
        />
      )}
    </>
  );
}
