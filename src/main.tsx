import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ArrowRight,
  History as HistoryIcon,
  Home as HomeIcon,
  Laptop,
  LockKeyhole,
  RefreshCw,
  Settings,
  ShieldCheck,
  Unplug,
} from "lucide-react";
import { command, runSyncTest } from "./api";
import { ErrorNotice, Modal } from "./Controls";
import { Home } from "./Home";
import { Devices } from "./Devices";
import { History } from "./History";
import { Pairing } from "./Pairing";
import { SettingsContent } from "./Settings";
import { useEngine, useOperation } from "./useEngine";
import "./style.css";
const routes = [
  ["Home", HomeIcon],
  ["Devices", Laptop],
  ["History", HistoryIcon],
] as const;
type Route = (typeof routes)[number][0];
function App() {
  const [route, setRoute] = useState<Route>("Home");
  const [dialog, setDialog] = useState<"pairing" | "settings" | null>(null);
  const [notice, setNotice] = useState("");
  const [reduced, setReduced] = useState(() => {
    try {
      return localStorage.getItem("harnesssync.reduce-motion") === "true";
    } catch {
      return false;
    }
  });
  const { data, error, checking, refresh } = useEngine();
  const pairing = useOperation();
  const test = useOperation();
  const baseline = useRef<Set<string>>(new Set());
  const pairingSession = useRef(0);
  const main = useRef<HTMLElement>(null);
  useEffect(() => {
    document.documentElement.dataset.motion = reduced ? "reduced" : "system";
    try {
      localStorage.setItem("harnesssync.reduce-motion", String(reduced));
    } catch {
      /* Optional preference storage. */
    }
  }, [reduced]);
  useEffect(() => {
    if (dialog !== "pairing" || !data) return;
    const added = data.transport.peers.find(
      (p) => !baseline.current.has(p.id) && p.connected,
    );
    if (added) {
      setDialog(null);
      setRoute("Home");
      setNotice("");
      requestAnimationFrame(() => main.current?.focus());
      void command("close_pairing").catch(() => {});
    }
  }, [data, dialog]);
  const navigate = (name: Route) => {
    setRoute(name);
    main.current?.focus();
    main.current?.scrollTo({ top: 0 });
  };
  const add = () => {
    if (!data || pairing.busy || dialog) return;
    const session = ++pairingSession.current;
    baseline.current = new Set(data.transport.peers.map((p) => p.id));
    setDialog("pairing");
    setNotice("");
    void pairing.run(async () => {
      await command("open_pairing");
      if (session !== pairingSession.current) await command("close_pairing");
      await refresh();
    });
  };
  const close = () => {
    if (dialog === "pairing") {
      pairingSession.current++;
      void command("close_pairing")
        .then(refresh)
        .catch(() =>
          setNotice(
            "The pairing window could not be closed. Restart LearnHarness Sync to close new requests.",
          ),
        );
    }
    setDialog(null);
  };
  const runTest = (id: string) =>
    void test.run(async () => {
      await runSyncTest(id);
      await refresh();
    });
  return (
    <div className="app">
      <a className="skip" href="#main">
        Skip to content
      </a>
      <aside className="sidebar">
        <div className="brand">
          <img src="/brand/learnharness-sync-app-icon-512.png" alt="" />
          <div>
            <strong>LearnHarness Sync</strong>
            <span>Private, local sync</span>
          </div>
        </div>
        <nav aria-label="Main navigation">
          {routes.map(([name, Icon]) => (
            <button
              key={name}
              aria-current={route === name ? "page" : undefined}
              onClick={() => navigate(name)}
            >
              <Icon aria-hidden="true" />
              {name}
              {name === "History" && !!data?.conflicts.length && (
                <span
                  className="attention-dot"
                  aria-label="Conflict needs attention"
                />
              )}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="private-note">
            <LockKeyhole size={22} aria-hidden="true" />
            <p>
              Private by design<span>Direct. Local. Yours.</span>
            </p>
          </div>
          <button
            className="settings-button"
            aria-label="Settings"
            onClick={() => setDialog("settings")}
            disabled={!data}
          >
            <Settings aria-hidden="true" />
            <span>Settings</span>
          </button>
          <span className="version">Experimental · 0.2.0-alpha.1</span>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <span>{route}</span>
          <span className="app-availability">
            <span className={`dot ${error ? "unavailable" : ""}`} />
            {error
              ? "Connection unavailable"
              : checking
                ? "Starting…"
                : "Local connection"}
          </span>
        </header>
        <main id="main" ref={main} tabIndex={-1}>
          {error ? (
            <section className="engine-error" role="alert">
              <Unplug size={42} aria-hidden="true" />
              <h1>Let’s restore the connection</h1>
              <p>{error}</p>
              <button className="primary" onClick={() => void refresh()}>
                <RefreshCw aria-hidden="true" />
                Check again
              </button>
            </section>
          ) : !data ? (
            <div className="loading-state" role="status">
              <ShieldCheck size={42} aria-hidden="true" />
              <h1>Getting things ready</h1>
              <p>Checking your local connection…</p>
            </div>
          ) : (
            <div className="view" key={route}>
              {notice && (
                <p className="pair-success" role="status">
                  <ShieldCheck size={22} aria-hidden="true" />
                  {notice}
                </p>
              )}
              {route === "Home" && (
                <Home
                  data={data}
                  add={add}
                  review={() => navigate("History")}
                  testing={test.busy}
                  testError={test.error}
                  runTest={runTest}
                />
              )}
              {route === "Devices" && (
                <Devices
                  data={data}
                  add={add}
                  settings={() => setDialog("settings")}
                  changed={refresh}
                />
              )}
              {route === "History" && <History data={data} changed={refresh} />}
            </div>
          )}
        </main>
        <footer className="safety-footer">
          <ShieldCheck size={21} aria-hidden="true" />
          <div>
            <strong>
              {data?.detected
                ? "LearnHarness detected. Sync uses an isolated managed profile."
                : "Connect a managed LearnHarness profile in Settings."}
            </strong>
            <span>
              Only supported managed-profile data is exchanged. Keep both apps
              open.
            </span>
          </div>
          <button
            className="text-button"
            disabled={!data}
            onClick={() => setDialog("settings")}
          >
            About
            <ArrowRight size={17} aria-hidden="true" />
          </button>
        </footer>
      </div>
      {dialog && data && (
        <Modal
          title={dialog === "pairing" ? "Add a device" : "Settings"}
          onClose={close}
          wide={dialog === "settings"}
        >
          {dialog === "pairing" ? (
            <Pairing
              status={data}
              onChange={refresh}
              openError={pairing.error}
            />
          ) : (
            <SettingsContent
              data={data}
              changed={refresh}
              reduced={reduced}
              setReduced={setReduced}
            />
          )}
          <ErrorNotice error={dialog === "settings" ? pairing.error : ""} />
        </Modal>
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
