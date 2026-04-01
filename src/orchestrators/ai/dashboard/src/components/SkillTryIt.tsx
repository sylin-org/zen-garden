import { useState, useRef, useCallback } from "react";
import type { SkillPresentation } from "../types";

interface SkillTryItProps {
  skillName: string;
}

type JobStatus = "idle" | "uploading" | "running" | "completed" | "failed";

interface JobResult {
  status: string;
  content?: Array<{ type: string; url?: string; format?: string }>;
  error?: { code: string; message: string };
  usage?: { duration_ms: number };
}

export function SkillTryIt({ skillName }: SkillTryItProps) {
  const [schema, setSchema] = useState<SkillPresentation | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(true);
  const [imageData, setImageData] = useState<string | null>(null);
  const [imageName, setImageName] = useState<string | null>(null);
  const [params, setParams] = useState<Record<string, unknown>>({});
  const [jobStatus, setJobStatus] = useState<JobStatus>("idle");
  const [result, setResult] = useState<JobResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // Fetch skill form schema
  useState(() => {
    fetch(`/v1/skills/${skillName}/form`)
      .then((r) => (r.ok ? r.json() : null))
      .then((data: SkillPresentation | null) => {
        if (data) {
          setSchema(data);
          // Set defaults from schema
          const props = (data.schema as Record<string, unknown>)?.properties as
            | Record<string, Record<string, unknown>>
            | undefined;
          if (props) {
            const defaults: Record<string, unknown> = {};
            for (const [key, val] of Object.entries(props)) {
              if (val.default !== undefined) defaults[key] = val.default;
            }
            setParams(defaults);
          }
        }
        setSchemaLoading(false);
      })
      .catch(() => setSchemaLoading(false));
  });

  const handleFile = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setImageName(file.name);
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Strip data URI prefix for display, keep full for API
      setImageData(result);
    };
    reader.readAsDataURL(file);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    const file = e.dataTransfer.files[0];
    if (!file) return;
    setImageName(file.name);
    const reader = new FileReader();
    reader.onload = () => setImageData(reader.result as string);
    reader.readAsDataURL(file);
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!imageData) return;
    setJobStatus("uploading");
    setResult(null);
    setErrorMsg(null);

    try {
      const res = await fetch("/v1/workflows/run", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          skill: skillName,
          content: [{ type: "image", role: "source", data: imageData }],
          parameters: params,
        }),
      });

      const job = await res.json();

      if (!res.ok) {
        setJobStatus("failed");
        setErrorMsg(job.error?.message ?? `HTTP ${res.status}`);
        return;
      }

      if (job.status === "completed") {
        setJobStatus("completed");
        setResult(job);
        return;
      }

      // Poll for completion
      setJobStatus("running");
      const jobId = job.id;
      const start = Date.now();
      const maxWait = 300_000; // 5 min

      const poll = async () => {
        if (Date.now() - start > maxWait) {
          setJobStatus("failed");
          setErrorMsg("Timeout waiting for result");
          return;
        }

        const pollRes = await fetch(`/v1/workflows/jobs/${jobId}`);
        const pollJob = await pollRes.json();

        if (pollJob.status === "completed") {
          setJobStatus("completed");
          setResult(pollJob);
        } else if (pollJob.status === "failed") {
          setJobStatus("failed");
          setErrorMsg(pollJob.error?.message ?? "Workflow failed");
        } else {
          setTimeout(poll, 500);
        }
      };

      setTimeout(poll, 500);
    } catch (err: unknown) {
      setJobStatus("failed");
      setErrorMsg(err instanceof Error ? err.message : "Request failed");
    }
  }, [imageData, params, skillName]);

  if (schemaLoading) {
    return <div className="text-xs text-gray-500 py-2">Loading...</div>;
  }

  if (!schema) {
    return <div className="text-xs text-red-400 py-2">Failed to load skill schema</div>;
  }

  const schemaProps = (schema.schema as Record<string, unknown>)?.properties as
    | Record<string, Record<string, unknown>>
    | undefined;

  return (
    <div className="space-y-3">
      {/* Mermaid diagram */}
      {schema.diagram && (
        <div className="bg-[#0d0e14] rounded px-3 py-2 border border-gray-800">
          <pre className="text-[10px] text-gray-500 font-mono whitespace-pre">
            {schema.diagram}
          </pre>
        </div>
      )}

      {/* Image upload dropzone */}
      <div
        className={`border-2 border-dashed rounded-lg p-4 text-center cursor-pointer transition-colors ${
          imageData
            ? "border-emerald-500/40 bg-emerald-500/5"
            : "border-gray-700 hover:border-gray-500 bg-[#0d0e14]"
        }`}
        onClick={() => fileRef.current?.click()}
        onDragOver={(e) => e.preventDefault()}
        onDrop={handleDrop}
      >
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={handleFile}
        />
        {imageData ? (
          <div className="flex items-center gap-3 justify-center">
            <img
              src={imageData}
              alt="Input"
              className="h-16 rounded border border-gray-700"
            />
            <div className="text-left">
              <div className="text-sm text-gray-200">{imageName}</div>
              <div className="text-[10px] text-gray-500">Click or drop to replace</div>
            </div>
          </div>
        ) : (
          <div>
            <div className="text-sm text-gray-400">Drop an image here</div>
            <div className="text-[10px] text-gray-600 mt-1">or click to browse</div>
          </div>
        )}
      </div>

      {/* Parameters */}
      {schemaProps && Object.keys(schemaProps).length > 0 && (
        <div className="flex flex-wrap gap-3">
          {Object.entries(schemaProps).map(([key, prop]) => {
            const enumVals = prop.enum as unknown[] | undefined;
            const uiWidget = (schema.ui_schema as Record<string, Record<string, string>>)?.[key]?.[
              "ui:widget"
            ];

            if (uiWidget === "radio" && enumVals) {
              return (
                <div key={key} className="space-y-1">
                  <label className="text-[10px] text-gray-500 uppercase tracking-wider">
                    {(prop.title as string) ?? key}
                  </label>
                  <div className="flex gap-1">
                    {enumVals.map((v) => (
                      <button
                        key={String(v)}
                        className={`px-3 py-1 text-xs rounded border ${
                          params[key] === v
                            ? "bg-blue-500/20 border-blue-500/50 text-blue-300"
                            : "bg-[#1a1b23] border-gray-700 text-gray-400 hover:border-gray-500"
                        }`}
                        onClick={() =>
                          setParams((p) => ({ ...p, [key]: v }))
                        }
                      >
                        {String(v)}x
                      </button>
                    ))}
                  </div>
                </div>
              );
            }

            if (uiWidget === "select" && enumVals) {
              return (
                <div key={key} className="space-y-1">
                  <label className="text-[10px] text-gray-500 uppercase tracking-wider">
                    {(prop.title as string) ?? key}
                  </label>
                  <select
                    value={String(params[key] ?? "")}
                    onChange={(e) =>
                      setParams((p) => ({ ...p, [key]: e.target.value }))
                    }
                    className="bg-[#1a1b23] border border-gray-700 rounded px-2 py-1 text-xs text-gray-200 w-full max-w-[260px]"
                  >
                    {enumVals.map((v) => (
                      <option key={String(v)} value={String(v)}>
                        {String(v)}
                      </option>
                    ))}
                  </select>
                </div>
              );
            }

            return null;
          })}
        </div>
      )}

      {/* Submit */}
      <button
        onClick={handleSubmit}
        disabled={!imageData || jobStatus === "uploading" || jobStatus === "running"}
        className={`px-4 py-1.5 rounded text-xs font-medium transition-colors ${
          !imageData || jobStatus === "uploading" || jobStatus === "running"
            ? "bg-gray-700 text-gray-500 cursor-not-allowed"
            : "bg-blue-600 text-white hover:bg-blue-500"
        }`}
      >
        {jobStatus === "uploading"
          ? "Uploading..."
          : jobStatus === "running"
            ? "Processing..."
            : "Upscale"}
      </button>

      {/* Error */}
      {errorMsg && (
        <div className="text-xs text-red-400 bg-red-400/5 border border-red-500/30 rounded px-3 py-2">
          {errorMsg}
        </div>
      )}

      {/* Result */}
      {result?.content?.[0]?.url && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-emerald-400 uppercase tracking-wider font-semibold">
              Result
            </span>
            {result.usage && (
              <span className="text-[10px] text-gray-500">
                {(result.usage.duration_ms / 1000).toFixed(1)}s
              </span>
            )}
          </div>
          <img
            src={result.content[0].url}
            alt="Upscaled result"
            className="max-w-full rounded border border-gray-700"
          />
          <a
            href={result.content[0].url}
            download="upscaled.png"
            className="inline-block text-xs text-blue-400 hover:underline"
          >
            Download
          </a>
        </div>
      )}
    </div>
  );
}
