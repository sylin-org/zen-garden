import { useEffect, useState, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { get } from "../api/client";
import { useCatalog } from "../contexts/CatalogContext";
import type { PersistedRequest, RequestListResponse } from "../api/types";
import { useSSE } from "../hooks/useSSE";

interface Props {
  open: boolean;
  onToggle: () => void;
}

export default function OverviewPanel({ open, onToggle }: Props) {
  const { catalog } = useCatalog();
  const navigate = useNavigate();
  const [requests, setRequests] = useState<PersistedRequest[]>([]);

  const fetchRequests = useCallback(async () => {
    try {
      const data = await get<RequestListResponse>("/v1/requests?limit=30");
      setRequests(data.requests);
    } catch {
      // Non-fatal
    }
  }, []);

  useEffect(() => {
    fetchRequests();
  }, [fetchRequests]);

  // Refresh when any dispatch completes
  useSSE({
    focus: "jobs.*",
    onEvent: (_topic, _payload) => {
      fetchRequests();
    },
    enabled: open,
  });

  const handlePin = useCallback(
    async (id: string, e: React.MouseEvent) => {
      e.stopPropagation();
      try {
        await fetch(`/v1/requests/${id}/pin`, { method: "PATCH" });
        fetchRequests();
      } catch {
        // Non-fatal
      }
    },
    [fetchRequests],
  );

  if (!open) {
    return (
      <button
        onClick={onToggle}
        className="absolute right-0 top-1/2 -translate-y-1/2 w-6 h-16 bg-surface border border-border
                   border-r-0 rounded-l-lg flex items-center justify-center text-text-dimmer
                   hover:text-text-dim transition-colors z-10"
        title="Open overview"
      >
        ‹
      </button>
    );
  }

  return (
    <div className="w-[280px] shrink-0 border-l border-border bg-surface flex flex-col overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold">
          Overview
        </span>
        <button
          onClick={onToggle}
          className="text-text-dimmer hover:text-text-dim text-xs transition-colors"
          title="Close overview"
        >
          ›
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {/* Status section */}
        {catalog && (
          <div className="px-4 py-3 border-b border-border">
            <div className="grid grid-cols-3 gap-2 text-center">
              <StatBox label="Primitives" value={catalog.primitives.length} />
              <StatBox label="Skills" value={catalog.skills.length} />
              <StatBox
                label="Providers"
                value={catalog.providers.filter((p) => p.enabled).length}
              />
            </div>

            {/* Provider health */}
            <div className="mt-3 space-y-1">
              {catalog.providers.map((p) => (
                <div key={p.name} className="flex items-center gap-1.5 text-[10px]">
                  <div
                    className={`w-1.5 h-1.5 rounded-full ${p.enabled ? "bg-green" : "bg-red"}`}
                  />
                  <span className="text-text-dim flex-1 truncate">{p.name}</span>
                  <span className="text-text-dimmer">
                    {p.capability_count}c {p.skill_count}s
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Request history */}
        <div className="px-4 py-3">
          <div className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold mb-2">
            History
          </div>
          {requests.length === 0 ? (
            <div className="text-[11px] text-text-dimmer italic">No requests yet</div>
          ) : (
            <div className="space-y-1">
              {requests.map((req) => (
                <HistoryEntry
                  key={req.id}
                  request={req}
                  onClick={() => {
                    const action = req.action;
                    const url = `/create/${action.replace(/\./g, "/")}?r=${req.id}`;
                    navigate(url);
                  }}
                  onPin={(e) => handlePin(req.id, e)}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function StatBox({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="text-lg font-bold text-accent">{value}</div>
      <div className="text-[9px] text-text-dimmer uppercase tracking-wider">{label}</div>
    </div>
  );
}

function HistoryEntry({
  request,
  onClick,
  onPin,
}: {
  request: PersistedRequest;
  onClick: () => void;
  onPin: (e: React.MouseEvent) => void;
}) {
  const time = new Date(request.created_at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });

  const statusColor =
    request.status === "success"
      ? "bg-green"
      : request.status === "failure"
        ? "bg-red"
        : "bg-orange";

  // Extract a preview from the input
  const inputPreview = extractPreview(request.input);

  return (
    <div
      onClick={onClick}
      className="group flex items-start gap-2 p-2 rounded-md cursor-pointer
                 hover:bg-accent-bg transition-colors"
    >
      <div className={`w-1.5 h-1.5 rounded-full mt-1.5 shrink-0 ${statusColor}`} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1">
          <span className="text-[10px] text-text-dimmer">{time}</span>
          <span className="text-[10px] text-text-dim font-medium truncate">
            {request.action}
          </span>
        </div>
        {inputPreview && (
          <div className="text-[10px] text-text-dimmer truncate mt-0.5">
            {inputPreview}
          </div>
        )}
        {request.meta.latency_ms != null && (
          <span className="text-[9px] text-text-dimmer">
            {request.meta.provider} · {request.meta.latency_ms}ms
          </span>
        )}
      </div>
      <button
        onClick={onPin}
        className={[
          "text-[12px] shrink-0 transition-opacity mt-0.5",
          request.pinned
            ? "text-accent opacity-100"
            : "text-text-dimmer opacity-0 group-hover:opacity-100",
        ].join(" ")}
        title={request.pinned ? "Unpin" : "Pin"}
      >
        {request.pinned ? "📌" : "📍"}
      </button>
    </div>
  );
}

function extractPreview(input: unknown): string | null {
  if (!input || typeof input !== "object") return null;
  const obj = input as Record<string, unknown>;

  // Try common prompt paths
  const paths = [
    "text.prompt.user",
    "text.body",
    "image.prompt.positive",
  ];
  for (const path of paths) {
    const val = getNestedString(obj, path);
    if (val) return val.slice(0, 60);
  }
  return null;
}

function getNestedString(obj: Record<string, unknown>, dotted: string): string | null {
  const parts = dotted.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (!current || typeof current !== "object") return null;
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === "string" ? current : null;
}
