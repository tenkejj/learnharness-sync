import {
  ArrowRight,
  Check,
  CircleAlert,
  FlaskConical,
  History,
  Laptop,
  LockKeyhole,
  Monitor,
  ShieldCheck,
  Unplug,
} from "lucide-react";
import { when, type Peer, type Status, type SyncTest } from "./api";
import { AddButton, ErrorNotice, Working } from "./Controls";
import { homeState, localName, peerName } from "./presentation";
type HomeState = ReturnType<typeof homeState>;
const stateIcons: Record<string, typeof LockKeyhole> = {
  conflict: CircleAlert,
  offline: Unplug,
  success: ShieldCheck,
};
function ConnectionMap({
  name,
  peer,
  testing,
}: {
  name: string;
  peer: Peer | undefined;
  testing: boolean;
}) {
  const description = peer
    ? `${localName(name)} to ${peerName(peer)}: ${peer.connected ? "connected" : "offline"}`
    : "Your computer is ready to pair";
  const remoteStatus = peer
    ? peer.connected
      ? "Connected securely"
      : "Waiting to reconnect"
    : "Ready when you are";
  return (
    <div
      className={`connection-map ${peer?.connected ? "is-connected" : ""}`}
      role="group"
      aria-label={description}
    >
      <div className="device-node">
        <div className="device-symbol">
          <Monitor size={62} strokeWidth={1.35} aria-hidden="true" />
        </div>
        <strong>{localName(name)}</strong>
        <span>This device</span>
      </div>
      <div className="connection-path" aria-hidden="true">
        <span />
        <div>
          {testing ? (
            <Working>Testing</Working>
          ) : peer?.connected ? (
            <LockKeyhole size={22} />
          ) : (
            <Unplug size={22} />
          )}
        </div>
        <span />
      </div>
      <div className={`device-node ${!peer ? "unpaired" : ""}`}>
        <div className="device-symbol">
          <Laptop size={66} strokeWidth={1.35} aria-hidden="true" />
        </div>
        <strong>{peer ? peerName(peer) : "Your other computer"}</strong>
        <span>{remoteStatus}</span>
      </div>
    </div>
  );
}
function ContextAction({
  state,
  add,
  review,
  testing,
  runTest,
}: {
  state: HomeState;
  add: () => void;
  review: () => void;
  testing: boolean;
  runTest: (id: string) => void;
}) {
  if (state.kind === "new") return <AddButton onClick={add} />;
  if (state.kind === "conflict")
    return (
      <button className="primary" onClick={review}>
        <CircleAlert aria-hidden="true" />
        Review conflict
        <ArrowRight aria-hidden="true" />
      </button>
    );
  const peer = state.peer;
  if (!peer?.connected)
    return (
      <span className="waiting-label">
        <History size={20} aria-hidden="true" />
        Reconnecting automatically
      </span>
    );
  return (
    <button
      className={state.kind === "success" ? "secondary" : "primary"}
      disabled={testing}
      onClick={() => runTest(peer.id)}
    >
      {testing ? (
        <Working>Running sync test…</Working>
      ) : (
        <>
          <FlaskConical aria-hidden="true" />
          Run sync test
        </>
      )}
    </button>
  );
}
function LastTest({ test, review }: { test: SyncTest; review: () => void }) {
  const labels = {
    success: `Test verified with ${test.name}`,
    failed: "The sync test didn’t finish",
    conflict: "The test found different versions",
    pending: "Waiting for test confirmation",
  };
  const Icon =
    test.state === "success"
      ? Check
      : test.state === "pending"
        ? History
        : CircleAlert;
  return (
    <section
      className="exchange-note"
      aria-label="Last sync test"
      role="status"
    >
      <span className="small-symbol">
        <Icon aria-hidden="true" />
      </span>
      <div>
        <strong>{labels[test.state]}</strong>
        <p>
          {test.state === "success"
            ? when(test.finished ?? test.started)
            : test.message}
        </p>
      </div>
      {test.state === "success" && (
        <button className="text-button" onClick={review}>
          View history
          <ArrowRight size={18} aria-hidden="true" />
        </button>
      )}
    </section>
  );
}
export function Home({
  data,
  add,
  review,
  testing,
  testError,
  runTest,
}: {
  data: Status;
  add: () => void;
  review: () => void;
  testing: boolean;
  testError: string;
  runTest: (id: string) => void;
}) {
  const state = homeState(data);
  const { peer } = state;
  const Icon = stateIcons[state.kind] ?? LockKeyhole;
  const captions: Record<string, string> = {
    new: "Open LearnHarness Sync on both computers to begin.",
    conflict: "No version will be overwritten automatically.",
    offline: "Both apps need to stay open.",
  };
  return (
    <>
      <section
        className="home-status"
        aria-labelledby="connection-title"
        aria-busy={testing}
      >
        <div className="status-heading">
          <span className="state-symbol">
            <Icon size={34} aria-hidden="true" />
          </span>
          <div className="status-copy">
            <div aria-live="polite" aria-atomic="true">
              <h1 id="connection-title">
                {testing ? "Checking your connection" : state.title}
              </h1>
              <p>
                {testing
                  ? "Sending harmless test data and waiting for the other computer’s confirmation."
                  : state.text}
              </p>
            </div>
            {peer?.last_sync && !testing && (
              <p className="last-exchange-time">
                Last verified exchange · {when(peer.last_sync)}
              </p>
            )}
          </div>
        </div>
        <ConnectionMap name={data.device} peer={peer} testing={testing} />
        <div className="home-action">
          <ContextAction
            state={state}
            add={add}
            review={review}
            testing={testing}
            runTest={runTest}
          />
          <span className="action-caption">
            {captions[state.kind] ??
              "Test data only. Your LearnHarness data stays untouched."}
          </span>
        </div>
      </section>
      <ErrorNotice error={testError} />
      {data.sync_test && data.sync_test.state !== "success" && !testing && (
        <LastTest test={data.sync_test} review={review} />
      )}
    </>
  );
}
