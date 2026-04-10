import { useState, useCallback, useRef } from "react";
import { useSSE } from "../../hooks/useSSE";

interface EventEntry {
  id: number;
  topic: string;
  payload: unknown;
  at: string;
}

export default function EventLog() {
  const [events, setEvents] = useState<EventEntry[]>([]);
  const [focus, setFocus] = useState("*");
  const [selected, setSelected] = useState<EventEntry | null>(null);
  const [focusInput, setFocusInput] = useState("*");
  const listRef = useRef<HTMLDivElement>(null);

  const handleEvent = useCallback((topic: string, payload: unknown) => {
    const entry: EventEntry = {
      id: Date.now() + Math.random(),
      topic,
      payload,
      at: new Date().toISOString(),
    };
    setEvents((prev) => [entry, ...prev].slice(0, 500));
  }, []);

  useSSE({ focus, onEvent: handleEvent });

  const applyFilter = useCallback(() => {
    setFocus(focusInput.trim() || "*");
    setEvents([]);
  }, [focusInput]);

  // Auto-scroll is implicit since newest is at top

  return (
    <div className="flex h-full">
      {/* Master: event list */}
      <div className="flex-1 flex flex-col overflow-hidden border-r border-border">
        {/* Filter bar */}
        <div className="flex gap-2 p-3 border-b border-border shrink-0">
          <input
            type="text"
            placeholder="Focus pattern (e.g. skills.*, jobs.*)"
            className="flex-1 px-3 py-1.5 bg-surface-2 border border-border rounded text-[11px] text-text
                       font-mono placeholder:text-text-dimmer outline-none focus:border-accent"
            value={focusInput}
            onChange={(e) => setFocusInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applyFilter()}
          />
          <button
            onClick={applyFilter}
            className="px-3 py-1.5 bg-accent hover:bg-accent-dim text-white text-[10px] font-semibold rounded transition-colors"
          >
            Apply
          </button>
          <button
            onClick={() => setEvents([])}
            className="px-3 py-1.5 text-[10px] text-text-dimmer hover:text-text border border-border rounded transition-colors"
          >
            Clear
          </button>
        </div>

        {/* Event stream */}
        <div ref={listRef} className="flex-1 overflow-y-auto">
          {events.length === 0 ? (
            <div className="p-4 text-text-dimmer text-sm italic">
              Listening for events on "{focus}"...
            </div>
          ) : (
            events.map((ev) => (
              <div
                key={ev.id}
                onClick={() => setSelected(ev)}
                className={[
                  "flex items-center gap-2 px-4 py-1.5 border-b border-border cursor-pointer text-[11px] transition-colors",
                  selected?.id === ev.id ? "bg-accent-bg" : "hover:bg-surface",
                ].join(" ")}
              >
                <span className="text-text-dimmer shrink-0 w-[55px]">
                  {new Date(ev.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
                <span className="text-accent font-mono truncate">{ev.topic}</span>
              </div>
            ))
          )}
        </div>

        <div className="px-4 py-1.5 border-t border-border text-[10px] text-text-dimmer shrink-0">
          {events.length} events · focus: {focus}
        </div>
      </div>

      {/* Detail */}
      <div className="w-[380px] shrink-0 overflow-y-auto bg-surface">
        {selected ? (
          <div className="p-4">
            <div className="text-[10px] text-text-dimmer mb-1">Topic</div>
            <div className="text-[12px] font-mono text-accent mb-3">{selected.topic}</div>
            <div className="text-[10px] text-text-dimmer mb-1">Timestamp</div>
            <div className="text-[11px] text-text mb-3">{new Date(selected.at).toISOString()}</div>
            <div className="text-[10px] text-text-dimmer mb-1">Payload</div>
            <pre className="text-[10px] text-text-dim bg-surface-2 p-3 rounded-lg overflow-auto whitespace-pre-wrap max-h-[60vh]">
              {JSON.stringify(selected.payload, null, 2)}
            </pre>
          </div>
        ) : (
          <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
            Click an event to inspect
          </div>
        )}
      </div>
    </div>
  );
}
