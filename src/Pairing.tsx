import { useEffect, useRef, useState } from "react";
import {
  ArrowRight,
  Check,
  Copy,
  Laptop,
  Radar,
  ShieldCheck,
  X,
} from "lucide-react";
import { command, type Status } from "./api";
import { ErrorNotice, Working } from "./Controls";
import { useOperation } from "./useEngine";
import { peerName } from "./presentation";
export function Pairing({
  status,
  onChange,
  openError,
}: {
  status: Status;
  onChange: () => Promise<void>;
  openError: string;
}) {
  const [selected, setSelected] = useState("");
  const [fallback, setFallback] = useState(false);
  const [invitation, setInvitation] = useState("");
  const [own, setOwn] = useState("");
  const [copied, setCopied] = useState(false);
  const [decided, setDecided] = useState<Record<string, boolean>>({});
  const [message, setMessage] = useState("");
  const { busy, error, run } = useOperation();
  const lastPending = useRef(false);
  const pending = status.pending[0];
  useEffect(() => {
    if (pending) lastPending.current = true;
    if (!pending && lastPending.current) {
      setSelected("");
      setMessage(
        "Pairing ended. If the other computer isn’t connected, select it to try again.",
      );
      lastPending.current = false;
    }
  }, [pending]);
  useEffect(() => {
    if (!selected || pending) return;
    const timer = setTimeout(() => {
      setSelected("");
      setMessage(
        "The other computer didn’t respond. Click Add Device there, then try again.",
      );
    }, 15000);
    return () => clearTimeout(timer);
  }, [selected, pending]);
  const choose = (args: Record<string, unknown>, id: string) =>
    void run(async () => {
      await command("start_pairing", args);
      setSelected(id);
      setMessage("");
      await onChange();
    });
  const decide = (accept: boolean) =>
    pending &&
    void run(async () => {
      await command("pairing_decision", { id: pending.id, accept });
      setDecided((v) => ({ ...v, [pending.id]: accept }));
      setMessage(
        accept
          ? "Approved here. Waiting for the other computer to approve the same string."
          : "Pairing rejected. No new permission was granted.",
      );
      await onChange();
    });
  return (
    <div className="pairing-flow">
      <ErrorNotice error={error || openError} />
      {pending ? (
        <section className="pair-verification" aria-labelledby="verify-title">
          <span className="pair-shield">
            <ShieldCheck size={36} aria-hidden="true" />
          </span>
          <h3 id="verify-title">Is this {peerName(pending)}?</h3>
          <p>
            Compare all eight groups on both computers, in person or on a
            trusted call. Approve only if every group matches.
          </p>
          <code
            className="verification"
            aria-label={`Verification string: ${pending.code}`}
          >
            {pending.code.split(" ").map((group, i) => (
              <span key={i}>{group} </span>
            ))}
          </code>
          <p className="secondary-copy">
            Both computers must approve. A nearby name alone does not verify a
            device.
          </p>
          <div className="button-row">
            <button
              className="primary"
              disabled={busy || pending.id in decided}
              onClick={() => decide(true)}
            >
              <Check aria-hidden="true" />
              Strings match — approve
            </button>
            <button
              className="secondary"
              disabled={busy || pending.id in decided}
              onClick={() => decide(false)}
            >
              <X aria-hidden="true" />
              Reject
            </button>
          </div>
        </section>
      ) : (
        <>
          <p>
            Open LearnHarness Sync on your other computer and click{" "}
            <strong>Add Device</strong>. Keep both computers on the same
            network.
          </p>
          <div className="discovery-heading">
            <Radar size={24} aria-hidden="true" />
            <h3>
              {selected ? "Connecting to your computer…" : "Nearby computers"}
            </h3>
            {status.pairing_open && (
              <span className="discovery-pulse" aria-label="Discovery active" />
            )}
          </div>
          {selected ? (
            <p role="status">
              <Working>Waiting for a secure pairing request…</Working>
            </p>
          ) : status.discovered.length ? (
            <ul className="discovered-list">
              {status.discovered.map((p) => (
                <li key={p.id}>
                  <Laptop size={32} aria-hidden="true" />
                  <div>
                    <strong>
                      {p.name === "This computer" ? "Other computer" : p.name}
                    </strong>
                    <span>Nearby · not yet verified</span>
                  </div>
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() => choose({ device: p.id }, p.id)}
                  >
                    Pair
                    <ArrowRight size={20} aria-hidden="true" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="discovery-empty">
              <Laptop size={38} aria-hidden="true" />
              <strong>Looking for your other computer</strong>
              <p>It will appear here when it’s available for pairing.</p>
            </div>
          )}
          {!status.pairing_open && (
            <div className="feedback">
              <p>The pairing window has closed.</p>
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await command("open_pairing");
                    await onChange();
                  })
                }
              >
                Look again
              </button>
            </div>
          )}
          {!selected && (
            <button
              className="text-button fallback-toggle"
              aria-expanded={fallback}
              aria-controls="invitation-fallback"
              onClick={() => setFallback((v) => !v)}
            >
              Can’t find your computer?
              <ArrowRight size={18} aria-hidden="true" />
            </button>
          )}
          {fallback && (
            <section className="fallback" id="invitation-fallback">
              <h3>Connect with an invitation</h3>
              <p>
                Discovery can be blocked by a network. Exchange an invitation
                using a trusted channel; you’ll still verify the full string on
                both screens.
              </p>
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const value = await command<{ invitation: string }>(
                      "invitation",
                    );
                    setOwn(value.invitation);
                  })
                }
              >
                Create this computer’s invitation
              </button>
              {own && (
                <>
                  <label>
                    Your invitation
                    <textarea readOnly rows={3} value={own} />
                  </label>
                  <button
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        await navigator.clipboard.writeText(own);
                        setCopied(true);
                      })
                    }
                  >
                    <Copy aria-hidden="true" />
                    {copied ? "Copied" : "Copy invitation"}
                  </button>
                </>
              )}
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  choose({ invitation: invitation.trim() }, "invitation");
                }}
              >
                <label>
                  Invitation from the other computer
                  <textarea
                    required
                    maxLength={16384}
                    rows={3}
                    value={invitation}
                    onChange={(e) => setInvitation(e.target.value)}
                  />
                </label>
                <button
                  className="secondary"
                  disabled={busy || !!selected || !invitation.trim()}
                  type="submit"
                >
                  Connect and verify
                  <ArrowRight aria-hidden="true" />
                </button>
              </form>
            </section>
          )}
        </>
      )}
      <p className="pair-message" role="status">
        {message}
      </p>
    </div>
  );
}
