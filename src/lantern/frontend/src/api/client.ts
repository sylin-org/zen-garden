/** Typed API client for Lantern endpoints */

import type {
  Stone,
  OfferingGroup,
  PondMember,
  SeedBankView,
  ActivityEvent,
  HealthResponse,
} from "../types/api";

const BASE = "";

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json() as Promise<T>;
}

async function postAction(
  path: string,
  body?: unknown,
): Promise<{ status: number; data: unknown }> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : "{}",
  });
  const data = await res.json().catch(() => null);
  return { status: res.status, data };
}

// ── Read endpoints ───────────────────────────────────────────

export const getHealth = () => fetchJson<HealthResponse>("/health");

export const getStones = () => fetchJson<Stone[]>("/api/v1/garden/stones");

export const getStone = (id: string) =>
  fetchJson<Stone>(`/api/v1/garden/stones/${encodeURIComponent(id)}`);

export const getOfferings = () =>
  fetchJson<OfferingGroup[]>("/api/v1/garden/offerings");

export const getSeeds = () =>
  fetchJson<SeedBankView[]>("/api/v1/garden/seeds");

export const getPond = () => fetchJson<PondMember[]>("/api/v1/garden/pond");

export const getActivity = () =>
  fetchJson<ActivityEvent[]>("/api/v1/garden/activity");

// ── Action endpoints ─────────────────────────────────────────

export const restService = (stoneId: string, svc: string) =>
  postAction(
    `/api/v1/garden/stones/${encodeURIComponent(stoneId)}/services/${encodeURIComponent(svc)}/rest`,
  );

export const wakeService = (stoneId: string, svc: string) =>
  postAction(
    `/api/v1/garden/stones/${encodeURIComponent(stoneId)}/services/${encodeURIComponent(svc)}/wake`,
  );

export const deployOffering = (stoneId: string, body: unknown) =>
  postAction(
    `/api/v1/garden/stones/${encodeURIComponent(stoneId)}/offerings`,
    body,
  );
