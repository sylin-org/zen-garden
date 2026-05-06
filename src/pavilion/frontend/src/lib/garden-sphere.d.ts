/** Type declarations for the GardenSphere Three.js visualization library */

export type NodeKind = "stone" | "bank";

export interface SphereOptions {
  radius?: number;
  /// `id` is the picked node's id (stone_id or bank id); `kind`
  /// distinguishes which pool. `null` is "nothing under cursor".
  onHover?: (id: string | null, kind?: NodeKind | null) => void;
  onTrack?: (data: TrackData) => void;
  onTransition?: (data: TransitionData) => void;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  onDataChange?: (stones: any[]) => void;
}

export interface TrackData {
  selected: { id: string; pos: { x: number; y: number } } | null;
  departing: { id: string; pos: { x: number; y: number } } | null;
  hovered: { id: string; pos: { x: number; y: number } } | null;
  progress: number;
}

export interface TransitionData {
  selectedId: string | null;
  departingId: string | null;
  /// Kind of the newly-selected node (`null` when nothing
  /// selected or when the selection was cleared).
  kind?: NodeKind | null;
}

export class GardenSphere {
  stones: unknown[];
  banks: unknown[];
  constructor(container: HTMLElement, opts?: SphereOptions);
  // Stone API (existing)
  setData(stones: unknown[]): void;
  updateStone(id: string, patch: unknown): void;
  addStone(stone: unknown): void;
  removeStone(id: string): void;
  offlineStone(id: string): void;
  onlineStone(id: string, patch?: unknown): void;
  // Bank API (ORCH-0039 Frame 2)
  setBanks(banks: unknown[]): void;
  addBank(bank: unknown): void;
  removeBank(id: string): void;
  updateBank(id: string, patch: unknown): void;
  setSeedCount(id: string, count: number): void;
  // Camera + lifecycle
  resetView(): void;
  destroy(): void;
}

export function serviceKey(svc: { offering: string; instance_name: string | null }): string;
export function bankIdOf(bank: unknown): string;
