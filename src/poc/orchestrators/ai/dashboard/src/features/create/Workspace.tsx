import { useEffect, useState, useCallback, useRef } from "react";
import { useParams, useSearchParams, useLocation } from "react-router-dom";
import { get } from "../../api/client";
import type { WorkspaceSpec, PersistedRequest } from "../../api/types";
import WorkspaceForm from "./WorkspaceForm";
import SkillPicker from "./SkillPicker";
import ResultPanel from "./ResultPanel";

export default function Workspace() {
  const { modality, leaf, skill } = useParams();
  const [searchParams] = useSearchParams();
  const location = useLocation();
  const requestId = searchParams.get("r");
  const navRequest = (location.state as { request?: PersistedRequest } | null)?.request;

  const [spec, setSpec] = useState<WorkspaceSpec | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<unknown>(null);
  const [streamText, setStreamText] = useState<string | undefined>(undefined);
  const [sourceRequest, setSourceRequest] = useState<PersistedRequest | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<string | undefined>(undefined);
  // ORCH-0038: the model the user has picked. When set, the introspect
  // call passes `?model=` so the winning adapter can return a
  // context-aware field surface (e.g. reasoning-mode controls).
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);

  // Fetch workspace spec from the introspect endpoint.
  // Includes ?provider= and/or ?model= when the user has picked them.
  const basePath = skill
    ? `/v1/${modality}/${leaf}/${skill}`
    : `/v1/${modality}/${leaf}`;
  const introspectUrl = (() => {
    const params = new URLSearchParams();
    if (selectedProvider) params.set("provider", selectedProvider);
    if (selectedModel) params.set("model", selectedModel);
    const qs = params.toString();
    return qs ? `${basePath}?${qs}` : basePath;
  })();

  useEffect(() => {
    setLoading(true);
    setError(null);
    get<WorkspaceSpec>(introspectUrl)
      .then((s) => {
        setSpec(s);
        setLoading(false);
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : "Failed to load");
        setLoading(false);
      });
  }, [introspectUrl]);

  // Load source request for ?r= (view/fork mode)
  useEffect(() => {
    if (!requestId) {
      setSourceRequest(null);
      return;
    }
    if (navRequest && navRequest.id === requestId) {
      setSourceRequest(navRequest);
      if (navRequest.output) setResult({ output: navRequest.output, _meta: navRequest.meta });
      if (navRequest.error) setResult({ error: navRequest.error, _meta: navRequest.meta });
      return;
    }
    get<PersistedRequest>(`/v1/requests/${requestId}`)
      .then((req) => {
        setSourceRequest(req);
        if (req.output) setResult({ output: req.output, _meta: req.meta });
        if (req.error) setResult({ error: req.error, _meta: req.meta });
      })
      .catch(() => setSourceRequest(null));
  }, [requestId, navRequest]);

  // Reset when switching tools
  const prevBasePath = useRef(basePath);
  useEffect(() => {
    if (prevBasePath.current !== basePath) {
      setResult(null);
      setStreamText(undefined);
      setSourceRequest(null);
      setSelectedProvider(undefined);
      setSelectedModel(undefined);
      prevBasePath.current = basePath;
    }
  }, [basePath]);

  const handleResult = useCallback((r: unknown) => {
    setStreamText(undefined);
    setResult(r);
  }, []);

  const handleError = useCallback((e: unknown) => {
    setStreamText(undefined);
    setResult(e);
  }, []);

  const handleProviderChange = useCallback((provider: string | undefined) => {
    setSelectedProvider(provider);
    // Switching provider resets the model hint — the new provider
    // may not have the previously selected model.
    setSelectedModel(undefined);
    setResult(null);
    setStreamText(undefined);
  }, []);

  const handleModelChange = useCallback((model: string | undefined) => {
    // ORCH-0038: triggers a re-fetch of the introspect endpoint.
    // We keep the result panel contents so the user can compare
    // runs across models without losing the last output.
    setSelectedModel(model);
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-text-dim text-sm">
        Loading...
      </div>
    );
  }

  if (error || !spec) {
    return (
      <div className="flex items-center justify-center h-full text-red text-sm">
        {error ?? "Failed to load workspace"}
      </div>
    );
  }

  // Skill picker: if primitive has no fields and skills exist
  const hasFields = Object.keys(spec.fields).length > 0;
  const hasSkills = (spec.skills_available?.length ?? 0) > 0;
  const showSkillPicker = !hasFields && hasSkills && !skill;

  const hasResult = result != null || streamText != null;
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
      <div
        className="overflow-hidden shrink-0 transition-all duration-300"
        style={{ flexBasis: formBasis, minWidth: hasResult ? "320px" : undefined }}
      >
        {showSkillPicker ? (
          <SkillPicker
            skills={spec.skills_available ?? []}
            modality={modality!}
            leaf={leaf!}
          />
        ) : (
          <WorkspaceForm
            key={sourceRequest?.id ?? introspectUrl}
            spec={spec}
            sourceRequest={sourceRequest}
            onResult={handleResult}
            onError={handleError}
            onProviderChange={
              spec.routing.providers.length > 1 ? handleProviderChange : undefined
            }
            onModelChange={handleModelChange}
          />
        )}
      </div>

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

function nested(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}
