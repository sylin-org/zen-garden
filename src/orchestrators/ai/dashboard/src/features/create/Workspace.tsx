import { useEffect, useState, useCallback, useRef } from "react";
import { useParams } from "react-router-dom";
import { get } from "../../api/client";
import { useCatalog } from "../../contexts/CatalogContext";
import type { CatalogDetail } from "../../api/types";
import WorkspaceForm from "./WorkspaceForm";
import SkillPicker from "./SkillPicker";
import ResultPanel from "./ResultPanel";

export default function Workspace() {
  const { modality, leaf, skill } = useParams();
  const { catalog } = useCatalog();

  const [detail, setDetail] = useState<CatalogDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<unknown>(null);
  const [streamText, setStreamText] = useState<string | undefined>(undefined);

  const path = skill
    ? `/v1/catalog/${modality}/${leaf}/${skill}`
    : `/v1/catalog/${modality}/${leaf}`;

  useEffect(() => {
    setLoading(true);
    setError(null);
    get<CatalogDetail>(path)
      .then((d) => {
        setDetail(d);
        setLoading(false);
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : "Failed to load");
        setLoading(false);
      });
  }, [path]);

  // Reset result when switching tools
  const prevPath = useRef(path);
  useEffect(() => {
    if (prevPath.current !== path) {
      setResult(null);
      setStreamText(undefined);
      prevPath.current = path;
    }
  }, [path]);

  const handleResult = useCallback((r: unknown) => {
    setStreamText(undefined);
    setResult(r);
  }, []);

  const handleError = useCallback((e: unknown) => {
    setStreamText(undefined);
    setResult(e);
  }, []);

  const handleStreaming = useCallback((reader: ReadableStreamDefaultReader<Uint8Array>) => {
    setResult(null);
    setStreamText("");
    const decoder = new TextDecoder();
    let buffer = "";

    function read() {
      reader.read().then(({ done, value }) => {
        if (done) return;
        buffer += decoder.decode(value, { stream: true });

        // Parse SSE lines
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          if (line.startsWith("data: ")) {
            try {
              const data = JSON.parse(line.slice(6));
              // Append text deltas
              const delta =
                data?.output?.text?.delta ??
                data?.output?.text?.response ??
                data?.text?.delta ??
                data?.text?.response;
              if (typeof delta === "string") {
                setStreamText((prev) => (prev ?? "") + delta);
              }
              // Check for completion
              if (data?.output?.text?.finish_reason || data?.done) {
                setResult(data);
                setStreamText(undefined);
              }
            } catch {
              // Ignore malformed SSE data
            }
          }
        }
        read();
      });
    }
    read();
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-text-dim text-sm">
        Loading...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-red text-sm">
        {error}
      </div>
    );
  }

  if (!detail) return null;

  // If primitive has no fields and skills exist → show skill picker
  const hasFields = detail.fields && detail.fields.length > 0;
  const primitiveAction = `${modality}.${leaf}`;
  const relatedSkills = catalog?.skills.filter((s) => s.primitive === primitiveAction) ?? [];
  const showSkillPicker = !hasFields && relatedSkills.length > 0 && !skill;

  return (
    <div className="flex h-full">
      {/* Center panel */}
      <div className="flex-1 overflow-hidden">
        {showSkillPicker ? (
          <SkillPicker
            skills={relatedSkills}
            modality={modality!}
            leaf={leaf!}
          />
        ) : (
          <WorkspaceForm
            detail={detail}
            onResult={handleResult}
            onError={handleError}
            onStreaming={handleStreaming}
          />
        )}
      </div>

      {/* Right panel */}
      <div className="w-[340px] shrink-0 border-l border-border bg-surface overflow-hidden">
        <ResultPanel result={result} streaming={streamText} />
      </div>
    </div>
  );
}
