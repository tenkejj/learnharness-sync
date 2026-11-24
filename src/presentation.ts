import type { Status, Peer } from "./api";
export const peerName = (p: Pick<Peer, "name">) =>
  p.name === "This computer" ? "Other computer" : p.name;
export const localName = (name: string) =>
  name === "This computer" ? "Your computer" : name;
export function testCurrent(data: Status, peer: Peer | undefined) {
  return !!(
    data.head &&
    peer?.connected &&
    peer.receipt?.head === data.head &&
    peer.receipt?.received === data.head &&
    !peer.receipt.conflict
  );
}

export function homeState(data: Status) {
  const peer =
    data.transport.peers.find(
      (p) => p.connected && p.id === data.sync_test?.device,
    ) ??
    data.transport.peers.find((p) => p.connected) ??
    data.transport.peers[0];
  const conflict =
    data.conflicts.length > 0 ||
    data.transport.peers.some((p) => p.connected && p.receipt?.conflict);
  if (conflict)
    return {
      kind: "conflict",
      title: "Two versions need attention",
      text: "Both versions are safely preserved. Review them before running another test.",
      peer,
    };
  if (!peer)
    return {
      kind: "new",
      title: "Connect your computers",
      text: "Pair another computer for a private connection on your local network.",
      peer,
    };
  if (!peer.connected)
    return {
      kind: "offline",
      title: `${peerName(peer)} is offline`,
      text: "Keep LearnHarness Sync open on both computers on the same network. They will reconnect automatically.",
      peer,
    };
  if (testCurrent(data, peer))
    return {
      kind: "success",
      title: "Your test data is up to date",
      text: `${peerName(peer)} received, verified and adopted your test data.`,
      peer,
    };
  return {
    kind: "connected",
    title: `Connected to ${peerName(peer)}`,
    text: "Your private connection is ready. Run a harmless test to check that data reaches the other computer.",
    peer,
  };
}
export function eventTitle(kind: string) {
  if (kind.includes("Divergent")) return "Two versions need your attention";
  if (kind.includes("fast-forward"))
    return "Test data received · previous version backed up";
  if (kind.includes("Published")) return "Test data prepared on this computer";
  return kind;
}

/** Event time is second-granularity; count identical siblings in append order so
 * disclosure identity survives new activity being prepended on screen. */
export function keyedEvents(events: Status["events"]) {
  const counts = new Map<string, number>();
  return events
    .map((event) => {
      const stem = `${event.time}:${event.kind}`;
      const occurrence = counts.get(stem) ?? 0;
      counts.set(stem, occurrence + 1);
      return { ...event, key: `${stem}:${occurrence}` };
    })
    .toReversed();
}
