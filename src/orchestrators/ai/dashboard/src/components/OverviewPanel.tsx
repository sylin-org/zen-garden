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
                    navigate(url, { state: { request: req } });
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
  const statusColor =
    request.status === "success"
      ? "bg-green"
      : request.status === "failure"
        ? "bg-red"
        : "bg-orange";

  // Use adapter-generated summary if available, fall back to action name
  const label = request.meta.summary ?? request.action;

  return (
    <div
      onClick={onClick}
      className="group flex items-center gap-1.5 py-1.5 px-1 rounded cursor-pointer
                 hover:bg-accent-bg transition-colors"
    >
      <div className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusColor}`} />
      <span className="text-[10px] text-text-dim truncate flex-1">{label}</span>
      <span className="text-[9px] text-text-dimmer shrink-0">
        {relativeTime(request.created_at)}
      </span>
      <button
        onClick={onPin}
        className={[
          "text-[10px] shrink-0 transition-opacity",
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

/** Compact relative timestamp: "12s", "4m", "2h", "3d" */
function relativeTime(iso: string): string {
  const seconds = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (seconds < 0) return "now";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}
