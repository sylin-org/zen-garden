import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getActivity } from "../api/client";
import { useSSE } from "../hooks/useSSE";
import type { ActivityEvent } from "../types/api";
import "./Activity.css";

function eventColor(type: string): string {
  if (type.includes("offline") || type.includes("error")) return "var(--red)";
  if (type.includes("warning") || type.includes("withering")) return "var(--clay)";
  if (type.includes("registered") || type.includes("success")) return "var(--sage)";
  return "var(--s5)";
}

function timeAgo(ts: string): string {
  const diff = Date.now() - new Date(ts).getTime();
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  return `${hr}h ago`;
}

export function ActivityView() {
  const { events: sseEvents, connected } = useSSE();
  const [historical, setHistorical] = useState<ActivityEvent[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        setHistorical(await getActivity());
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  // Merge SSE events with historical, newest first
  const allEvents = [...sseEvents, ...historical].slice(0, 100);

  if (loading) return <div className="view-empty">Loading activity...</div>;

  return (
    <div className="activity">
      <div className="act-header">
        <span className="act-summary">
          <strong>{allEvents.length}</strong> recent events
        </span>
        <span className={`act-sse ${connected ? "on" : ""}`}>
          {connected ? "SSE Connected" : "SSE Disconnected"}
        </span>
      </div>

      <div className="act-stream">
        {allEvents.map((evt, i) => (
          <div key={`${evt.timestamp}-${i}`} className="act-event">
            <span
              className="act-dot"
              style={{ background: eventColor(evt.event_type) }}
            />
            <span className="act-time">{timeAgo(evt.timestamp)}</span>
            <span className="act-type">{evt.event_type}</span>
            {evt.stone_name && (
              <Link to={`/stones/${evt.stone_name}`} className="act-stone">{evt.stone_name}</Link>
            )}
            <span className="act-msg">{evt.message}</span>
          </div>
        ))}

        {allEvents.length === 0 && (
          <div className="view-empty">
            No activity yet. Events will appear here as stones register and services change state.
          </div>
        )}
      </div>
    </div>
  );
}
