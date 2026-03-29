// Deterministic stone color from name. Same stone = same color everywhere.

export interface StoneColorEntry {
  bg: string;
  border: string;
  text: string;
  hex: string;
}

const STONE_PALETTE: StoneColorEntry[] = [
  { bg: "bg-blue-500", border: "border-blue-500", text: "text-blue-400", hex: "#3b82f6" },
  { bg: "bg-emerald-500", border: "border-emerald-500", text: "text-emerald-400", hex: "#10b981" },
  { bg: "bg-amber-500", border: "border-amber-500", text: "text-amber-400", hex: "#f59e0b" },
  { bg: "bg-rose-500", border: "border-rose-500", text: "text-rose-400", hex: "#f43f5e" },
  { bg: "bg-violet-500", border: "border-violet-500", text: "text-violet-400", hex: "#8b5cf6" },
  { bg: "bg-cyan-500", border: "border-cyan-500", text: "text-cyan-400", hex: "#06b6d4" },
  { bg: "bg-orange-500", border: "border-orange-500", text: "text-orange-400", hex: "#f97316" },
  { bg: "bg-pink-500", border: "border-pink-500", text: "text-pink-400", hex: "#ec4899" },
];

export function stoneColor(name: string): StoneColorEntry {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash * 31 + name.charCodeAt(i)) | 0;
  }
  return STONE_PALETTE[Math.abs(hash) % STONE_PALETTE.length];
}
