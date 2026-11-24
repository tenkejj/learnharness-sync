import { useState } from "react";
import {
  Laptop,
  Monitor,
  MoreHorizontal,
  Pencil,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { command, when, type Peer, type Status } from "./api";
import { AddButton, ErrorNotice, Modal } from "./Controls";
import { localName, peerName } from "./presentation";
import { useOperation } from "./useEngine";
function ManageDevice({
  peer,
  close,
  changed,
}: {
  peer: Peer;
  close: () => void;
  changed: () => Promise<void>;
}) {
  const [name, setName] = useState(() => peerName(peer));
  const [removing, setRemoving] = useState(false);
  const { busy, error, run } = useOperation();
  return (
    <Modal title={`Manage ${peerName(peer)}`} onClose={close}>
      <ErrorNotice error={error} id="device-name-error" />
      {removing ? (
        <div>
          <h3>Remove this device?</h3>
          <p>
            {peerName(peer)} will stop receiving test data from this computer.
            Existing history and backups will be kept. Remove this computer on
            the other device too.
          </p>
          <div className="button-row">
            <button
              disabled={busy}
              className="danger"
              onClick={() =>
                void run(async () => {
                  await command("revoke", { device: peer.id });
                  await changed();
                  close();
                })
              }
            >
              <Trash2 aria-hidden="true" />
              Remove Device
            </button>
            <button disabled={busy} onClick={() => setRemoving(false)}>
              Keep device
            </button>
          </div>
        </div>
      ) : (
        <>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await command("rename_peer", {
                  device: peer.id,
                  name: name.trim(),
                });
                await changed();
                close();
              });
            }}
          >
            <label>
              Device name
              <input
                required
                maxLength={80}
                aria-describedby={error ? "device-name-error" : undefined}
                aria-invalid={!!error}
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </label>
            <p>This name helps you recognize the other computer here.</p>
            <button
              type="submit"
              className="primary"
              disabled={busy || !name.trim()}
            >
              <Pencil aria-hidden="true" />
              Save name
            </button>
          </form>
          <div className="management-remove">
            <button className="text-button" onClick={() => setRemoving(true)}>
              <Trash2 aria-hidden="true" />
              Remove Device
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
export function Devices({
  data,
  add,
  settings,
  changed,
}: {
  data: Status;
  add: () => void;
  settings: () => void;
  changed: () => Promise<void>;
}) {
  const [manage, setManage] = useState<Peer | null>(null);
  return (
    <>
      <div className="page-heading">
        <div>
          <h1>Your devices</h1>
          <p>A private connection between computers you trust.</p>
        </div>
        <AddButton onClick={add} />
      </div>
      <section className="device-list" aria-label="Your computers">
        <article className="device-card local-device">
          <div className="card-icon">
            <Monitor size={32} aria-hidden="true" />
          </div>
          <div className="device-description">
            <h2>{localName(data.device)}</h2>
            <p>This device · available while LearnHarness Sync is open</p>
          </div>
          <button
            className="icon-button"
            aria-label="Rename this device"
            onClick={settings}
          >
            <Pencil size={21} aria-hidden="true" />
          </button>
        </article>
        {data.transport.peers.map((p) => (
          <article key={p.id} className="device-card">
            <div className="card-icon">
              <Laptop size={34} aria-hidden="true" />
            </div>
            <div className="device-description">
              <div className="device-title">
                <h2>{peerName(p)}</h2>
                <span className={`device-state ${p.connected ? "online" : ""}`}>
                  <span className="dot" />
                  {p.connected ? "Connected" : "Offline"}
                </span>
              </div>
              <p>
                {p.last_sync
                  ? `Last verified exchange · ${when(p.last_sync)}`
                  : "No verified exchange yet"}
              </p>
              {p.error && (
                <details className="peer-error">
                  <summary>Connection needs attention</summary>
                  <p>{p.error}</p>
                </details>
              )}
            </div>
            <button
              className="icon-button"
              aria-label={`Manage ${peerName(p)}`}
              onClick={() => setManage(p)}
            >
              <MoreHorizontal aria-hidden="true" />
            </button>
          </article>
        ))}
      </section>
      {!data.transport.peers.length && (
        <div className="empty-state">
          <Laptop size={44} aria-hidden="true" />
          <h2>A place for your other computer</h2>
          <p>
            Add a device, compare the verification string, and approve on both
            screens.
          </p>
        </div>
      )}
      <p className="quiet-line">
        <ShieldCheck size={22} aria-hidden="true" />
        Only paired computers can exchange test data. Closing the app stops the
        connection.
      </p>
      {manage && (
        <ManageDevice
          peer={manage}
          close={() => setManage(null)}
          changed={changed}
        />
      )}
    </>
  );
}
