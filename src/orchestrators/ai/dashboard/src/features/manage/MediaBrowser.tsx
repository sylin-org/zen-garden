import { useEffect, useState } from "react";
import { get } from "../../api/client";
import type { MediaEntry, MediaListResponse } from "../../api/types";

export default function MediaBrowser() {
  const [media, setMedia] = useState<MediaEntry[]>([]);
  const [selected, setSelected] = useState<MediaEntry | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    get<MediaListResponse>("/v1/media")
      .then((data) => setMedia(data.media))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  return (
    <div className="flex h-full">
      {/* Master: media grid */}
      <div className="flex-1 overflow-y-auto p-4">
        {loading ? (
          <div className="text-text-dimmer text-sm">Loading...</div>
        ) : media.length === 0 ? (
          <div className="text-text-dimmer text-sm italic">No media uploaded</div>
        ) : (
          <div className="grid grid-cols-4 gap-3">
            {media.map((entry) => {
              const isImage = entry.content_type.startsWith("image/");
              const isAudio = entry.content_type.startsWith("audio/");
              return (
                <div
                  key={entry.media_id}
                  onClick={() => setSelected(entry)}
                  className={[
                    "p-2 rounded-lg border cursor-pointer transition-colors",
                    selected?.media_id === entry.media_id
                      ? "border-accent bg-accent-bg"
                      : "border-border hover:border-border-focus bg-surface-2",
                  ].join(" ")}
                >
                  {isImage ? (
                    <img
                      src={`/v1/media/${entry.media_id}`}
                      alt=""
                      className="w-full h-20 object-cover rounded mb-1"
                    />
                  ) : isAudio ? (
                    <div className="w-full h-20 flex items-center justify-center text-2xl text-text-dimmer">
                      🔊
                    </div>
                  ) : (
                    <div className="w-full h-20 flex items-center justify-center text-2xl text-text-dimmer">
                      📄
                    </div>
                  )}
                  <div className="text-[10px] text-text-dimmer truncate">
                    {entry.content_type}
                  </div>
                  <div className="text-[9px] text-text-dimmer">
                    {formatSize(entry.size_bytes)}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Detail */}
      <div className="w-[300px] shrink-0 border-l border-border overflow-y-auto bg-surface">
        {selected ? (
          <MediaDetail entry={selected} />
        ) : (
          <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
            Select a media item
          </div>
        )}
      </div>
    </div>
  );
}

function MediaDetail({ entry }: { entry: MediaEntry }) {
  const isImage = entry.content_type.startsWith("image/");
  const isAudio = entry.content_type.startsWith("audio/");

  return (
    <div className="p-4">
      {isImage && (
        <img
          src={`/v1/media/${entry.media_id}`}
          alt=""
          className="w-full rounded-lg mb-3"
        />
      )}
      {isAudio && (
        <audio controls src={`/v1/media/${entry.media_id}`} className="w-full mb-3" />
      )}

      <div className="space-y-2">
        <KV k="ID" v={entry.media_id} mono />
        <KV k="Type" v={entry.content_type} />
        <KV k="Size" v={formatSize(entry.size_bytes)} />
        <KV k="Source" v={entry.source.kind} />
        <KV k="State" v={entry.lifecycle.state} />
        {entry.lifecycle.expires_at && (
          <KV k="Expires" v={new Date(entry.lifecycle.expires_at).toLocaleString()} />
        )}
        <KV k="Created" v={new Date(entry.created_at).toLocaleString()} />
      </div>
    </div>
  );
}

function KV({ k, v, mono }: { k: string; v: string; mono?: boolean }) {
  return (
    <div className="text-[11px]">
      <span className="text-text-dimmer">{k}: </span>
      <span className={`text-text ${mono ? "font-mono text-[10px]" : ""}`}>{v}</span>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
