import { useCallback, useEffect, useRef, useState } from "react";
import { loadStatus, subscribeStatus, type Status } from "./api";
export const errorText = (e: unknown) =>
  e instanceof Error
    ? e.message
    : "The operation could not be verified. Check status before retrying.";

export function useEngine() {
  const [data, setData] = useState<Status | null>(null);
  const [error, setError] = useState("");
  const [checking, setChecking] = useState(true);
  const alive = useRef(false);
  const epoch = useRef(0);
  const refresh = useCallback(async () => {
    const ticket = ++epoch.current;
    try {
      const value = await loadStatus();
      if (alive.current && ticket === epoch.current) {
        setData(value);
        setError("");
      }
    } catch (e) {
      if (alive.current && ticket === epoch.current) {
        setError(errorText(e));
      }
    } finally {
      if (alive.current) setChecking(false);
    }
  }, []);
  useEffect(() => {
    alive.current = true;
    let active = true;
    let dispose: (() => void) | undefined;
    let polling = true;
    subscribeStatus((value) => {
      if (active) {
        epoch.current++;
        setData(value);
        setError("");
        setChecking(false);
      }
    })
      .then((fn) => {
        if (active) dispose = fn;
        else fn();
      })
      .catch(() => {
        /* Polling remains available if event subscription fails. */
      });
    void refresh().finally(() => {
      polling = false;
    });
    const timer = setInterval(() => {
      if (!polling) {
        polling = true;
        void refresh().finally(() => {
          polling = false;
        });
      }
    }, 4000);
    return () => {
      active = false;
      alive.current = false;
      epoch.current++;
      clearInterval(timer);
      dispose?.();
    };
  }, [refresh]);
  return { data, error, checking, refresh };
}

/** The synchronous guard prevents double clicks before React re-renders. */
export function useOperation() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const lock = useRef(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const run = async <T>(task: () => Promise<T>): Promise<T | undefined> => {
    if (lock.current) return undefined;
    lock.current = true;
    setBusy(true);
    setError("");
    try {
      return await task();
    } catch (e) {
      if (alive.current) setError(errorText(e));
      return undefined;
    } finally {
      lock.current = false;
      if (alive.current) setBusy(false);
    }
  };
  return { busy, error, run };
}
