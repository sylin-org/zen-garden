// Zen Garden Node resolver v0 (J1 — the connection promise, shipped).
// Ask for a capability; the room answers with a connection.
//
// Two ways in:
//   · discovery() — one UDP ask on the room's multicast group; every
//     moss that hears it answers with its stone identity.
//   · resolve(name, {stones}) — walk the room's mosses, query each
//     /api/v1/garden/stones, return the first matching service with its
//     connection URI. Bare-stem wishes accept any instance of the
//     capability; `name::instance` wants exactly itself (the same rule
//     rake's ensure obeys).

import dgram from "node:dgram";
import http from "node:http";
import os from "node:os";

/** The v1 room's discovery coordinates (glossary::discovery). */
export const DISCOVERY_GROUP = "239.255.42.199";
export const DISCOVERY_PORT = 7284;
export const DISCOVERY_TIMEOUT_MS = 2500;

/**
 * The best LAN interface for multicast: a private-range IPv4 address.
 * Multi-homed hosts (WSL, Docker) otherwise send the ask out a virtual
 * seam no moss hears.
 */
export function lanInterfaces() {
  const out = [];
  for (const ifaces of Object.values(os.networkInterfaces())) {
    for (const i of ifaces ?? []) {
      if (i.family !== "IPv4" || i.internal) continue;
      const isPrivate =
        i.address.startsWith("192.168.") ||
        i.address.startsWith("10.") ||
        /^172\.(1[6-9]|2\d|3[01])\./.test(i.address);
      if (isPrivate) out.push(i.address);
    }
  }
  return out;
}

/**
 * Ask the room who is here. One discovery request; every moss that
 * hears it answers with its stone card (ADR-0004 §1).
 * @returns {Promise<Array<{name: string, id: string, ip: string, port: number}>>}
 */
export function discover(timeoutMs = DISCOVERY_TIMEOUT_MS, group = DISCOVERY_GROUP, port = DISCOVERY_PORT) {
  return new Promise((resolve, reject) => {
    const sock = dgram.createSocket({ type: "udp4", reuseAddr: true });
    const stones = new Map();
    const done = (err) => {
      try { sock.close(); } catch { /* already closed */ }
      if (err) reject(err); else resolve([...stones.values()]);
    };
    sock.on("error", done);
    sock.on("message", (buf) => {
      try {
        const v = JSON.parse(buf.toString("utf8"));
        if (v.type !== "discovery_response") return;
        const stone = v.data?.stone ?? {};
        if (!stone.id || stones.has(stone.id)) return;
        stones.set(stone.id, {
          name: stone.name ?? "?",
          id: stone.id,
          ip: stone.network?.address?.ip ?? "?",
          port: stone.network?.address?.port ?? 0,
        });
      } catch { /* strangers ride by */ }
    });
    // Membership is the hearing aid; the multicast interface is the
    // speaking mouth. Multi-homed hosts need the LAN named explicitly —
    // the default route often points at a virtual switch no moss hears.
    sock.bind(port, () => {
      // Join on EVERY private interface (the house probe's law):
      // multi-homed hosts hear the room on one NIC only, and a
      // default-route join lands on the wrong one (WSL, Docker).
      for (const iface of lanInterfaces()) {
        try {
          sock.addMembership(group, iface);
        } catch { /* a refusal on one NIC must not silence the others */ }
      }
      const first = lanInterfaces()[0];
      if (first) {
        try { sock.setMulticastInterface(first); } catch { /* default may work */ }
      }
      const ask = Buffer.from(JSON.stringify({
        msg_id: cryptoRequestId(),
        type: "discovery_request",
        data: {
          discover: "moss",
          request_id: cryptoRequestId(),
          requester: "node-resolver",
          rich: false,
        },
      }));
      sock.send(ask, port, group, () => {});
    });
    setTimeout(() => done(null), timeoutMs);
  });
}

function cryptoRequestId() {
  return globalThis.crypto.randomUUID();
}

/**
 * The ensure lookup rule, mirrored from rake (R4.8's sibling law): a
 * bare-stem wish accepts any instance of the capability; a named
 * instance wants exactly itself.
 */
export function serviceMatches(wishFqn, wishNamedInstance, serviceName) {
  if (serviceName === wishFqn) return true;
  const stem = wishFqn.split("::")[0];
  return !wishNamedInstance && serviceName.split("::")[0] === stem;
}

/** Extract the first published host port from a service entry. */
function firstPort(ports) {
  const values = ports ? Object.values(ports) : [];
  return values.length ? values[0] : null;
}

/** The connection promise (J1) for one capability. */
export function connectionUri(stem, ip, port) {
  return port == null ? `${stem}://${ip}` : `${stem}://${ip}:${port}`;
}

/**
 * Resolve a capability by name: walk the given stones (or discover
 * them), query each moss's garden view, answer the first service whose
 * name satisfies the wish — the connection URI riding alongside.
 *
 * @param {string} name - catalog stem, FQN, or existing offering name
 * @param {{stones?: Array, timeoutMs?: number, group?: string, port?: number}} [opts]
 * @returns {Promise<{ensured: true, how: "found", name: string, stone: string, uri: string, status: string}>}
 */
export async function resolve(name, opts = {}) {
  const fqn = name.includes("::") ? name : `${name}::default`;
  const namedInstance = name.includes("::");
  const stones = opts.stones ?? await discover(opts.timeoutMs, opts.group, opts.port);
  if (!stones.length) {
    throw new Error("no moss answered the discovery ask — the room is out of reach");
  }
  for (const stone of stones) {
    const view = await fetchJson(stone.ip, stone.port, "/api/v1/garden/stones");
    for (const row of view.data?.stones ?? []) {
      for (const svc of row.inventory?.services?.items ?? []) {
        if (!serviceMatches(fqn, namedInstance, svc.name)) continue;
        const ip = row.stone?.network?.address?.ip ?? stone.ip;
        const port = firstPort(svc.ports);
        const stem = svc.stem ?? svc.name.split("::")[0];
        return {
          ensured: true,
          how: "found",
          name: svc.name,
          stone: row.stone?.name ?? "?",
          uri: connectionUri(stem, ip, port),
          status: svc.state?.status ?? "unknown",
        };
      }
    }
  }
  throw new Error(`no room member carries '${name}'`);
}

/** GET one JSON document from a moss. */
function fetchJson(ip, port, path, timeoutMs = 3000) {
  return new Promise((resolve, reject) => {
    const req = http.get({ host: ip, port, path, timeout: timeoutMs, headers: { Accept: "application/json" } }, (res) => {
      let body = "";
      res.on("data", (c) => { body += c; });
      res.on("end", () => {
        try { resolve(JSON.parse(body)); }
        catch (e) { reject(new Error(`moss answered unparsable: ${e.message}`)); }
      });
    });
    req.on("timeout", () => req.destroy(new Error("moss read timed out")));
    req.on("error", reject);
  });
}
