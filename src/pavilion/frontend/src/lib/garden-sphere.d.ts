/** Type declarations for the GardenSphere Three.js visualization library */

export interface SphereOptions {
  radius?: number;
  onHover?: (id: string | null) => void;
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
}

export class GardenSphere {
  stones: unknown[];
  constructor(container: HTMLElement, opts?: SphereOptions);
  setData(stones: unknown[]): void;
  updateStone(id: string, patch: unknown): void;
  addStone(stone: unknown): void;
  removeStone(id: string): void;
  offlineStone(id: string): void;
  onlineStone(id: string, patch?: unknown): void;
  resetView(): void;
  destroy(): void;
}

export function serviceKey(svc: { offering: string; instance_name: string | null }): string;
