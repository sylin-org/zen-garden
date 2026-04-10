import { useEffect, useState, useCallback, useRef } from "react";
import { useParams, useSearchParams, useLocation } from "react-router-dom";
import { get } from "../../api/client";
import { useCatalog } from "../../contexts/CatalogContext";
import type { CatalogDetail, PersistedRequest } from "../../api/types";
import WorkspaceForm from "./WorkspaceForm";
import SkillPicker from "./SkillPicker";
import ResultPanel from "./ResultPanel";

export default function Workspace() {
  const { modality, leaf, skill } = useParams();
  const [searchParams] = useSearchParams();
  const { catalog } = useCatalog();

  const location = useLocation();
  const requestId = searchParams.get("r");   // view mode
  const forkFromId = searchParams.get("from"); // fork mode
  // Navigation state: the OverviewPanel passes the full request
  // object so we skip the fetch round-trip.
  const navRequest = (location.state as { request?: PersistedRequest } | null)?.request;

  const [detail, setDetail] = useState<CatalogDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<unknown>(null);
  const [streamText, setStreamText] = useState<string | undefined>(undefined);
  const [initialValues, setInitialValues] = useState<Record<string, unknown> | null>(null);
  const [sourceRequest, setSourceRequest] = useState<PersistedRequest | null>(null);

  const catalogPath = skill
    ? `/v1/catalog/${modality}/${leaf}/${skill}`
    : `/v1/catalog/${modality}/${leaf}`;

  // Load catalog detail
  useEffect(() => {
    setLoading(true);
    setError(null);
    get<CatalogDetail>(catalogPath)
      .then((d) => {
        setDetail(d);
        setLoading(false);
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : "Failed to load");
        setLoading(false);
      });
  }, [catalogPath]);

  // Load request record if ?r= or ?from= is present.
  // If the OverviewPanel passed the request via navigation state,
  // use it directly — no fetch needed.
  const sourceId = requestId ?? forkFromId;
  useEffect(() => {
    if (!sourceId) {
      setInitialValues(null);
      setSourceRequest(null);
      return;
    }

    const applyRequest = (req: PersistedRequest) => {
      setSourceRequest(req);
      const flat = flattenToDotted(req.input as Record<string, unknown>);
      setInitialValues(flat);
      if (requestId && req.output) {
        setResult({ output: req.output, _meta: req.meta });
      }
      if (requestId && req.error) {
        setResult({ error: req.error, _meta: req.meta });
      }
    };

    // Use navigation state if it matches the requested ID.
    if (navRequest && navRequest.id === sourceId) {
      applyRequest(navRequest);
      return;
    }

    // Otherwise fetch from the API.
    get<PersistedRequest>(`/v1/requests/${sourceId}`)
      .then(applyRequest)
      .catch(() => {
        setInitialValues(null);
        setSourceRequest(null);
      });
  }, [sourceId, requestId, navRequest]);

  // Reset when switching tools (but not when query params change)
  const prevCatalogPath = useRef(catalogPath);
  useEffect(() => {
    if (prevCatalogPath.current !== catalogPath) {
      setResult(null);
      setStreamText(undefined);
      setInitialValues(null);
      setSourceRequest(null);
      prevCatalogPath.current = catalogPath;
    }
  }, [catalogPath]);

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

        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          if (line.startsWith("data: ")) {
            try {
              const data = JSON.parse(line.slice(6));
              const delta =
                data?.output?.text?.delta ??
                data?.output?.text?.response ??
                data?.text?.delta ??
                data?.text?.response;
              if (typeof delta === "string") {
                setStreamText((prev) => (prev ?? "") + delta);
              }
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

  // Determine parent_id for fork lineage
  const parentId = forkFromId ?? (requestId ? requestId : undefined);

  const hasResult = result != null || streamText != null;

  // Detect output type for split ratio:
  // - Image results get more space (60% result)
  // - Text results get equal split (50/50)
  // - No result = form takes full width
  const outputHasImage =
    hasResult &&
    result &&
    typeof result === "object" &&
    (nested(result as Record<string, unknown>, "output.image.data") != null ||
      nested(result as Record<string, unknown>, "output.image.media_id") != null);

  const formBasis = !hasResult ? "100%" : outputHasImage ? "40%" : "50%";
  const resultBasis = outputHasImage ? "60%" : "50%";

  return (
    <div className="flex h-full">
      {/* Form panel — full width when no result, proportional split when result exists */}
      <div
        className="overflow-hidden shrink-0 transition-all duration-300"
        style={{ flexBasis: formBasis, minWidth: hasResult ? "320px" : undefined }}
      >
        {showSkillPicker ? (
          <SkillPicker
            skills={relatedSkills}
            modality={modality!}
            leaf={leaf!}
          />
        ) : (
          <WorkspaceForm
            key={sourceRequest?.id ?? catalogPath}
            detail={detail}
            initialValues={initialValues}
            parentId={parentId}
            sourceRequest={sourceRequest}
            onResult={handleResult}
            onError={handleError}
            onStreaming={handleStreaming}
          />
        )}
      </div>

      {/* Result panel — only visible when there's content */}
      {hasResult && (
        <div
          className="border-l border-border bg-surface overflow-hidden transition-all duration-300"
          style={{ flexBasis: resultBasis }}
        >
          <ResultPanel result={result} streaming={streamText} />
        </div>
      )}
    </div>
  );
}

/** Access a nested value via dotted path. */
function nested(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

/** Flatten a nested object to dotted-path keys. */
function flattenToDotted(
  obj: Record<string, unknown>,
  prefix = "",
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      Object.assign(result, flattenToDotted(value as Record<string, unknown>, path));
    } else {
      result[path] = value;
    }
  }
  return result;
}
