import { useState, useRef, useCallback } from "react";
import type { SkillFormResponse, SkillMapping, ParamOption } from "../types";
import { MermaidDiagram } from "./MermaidDiagram";

interface SkillTryItProps {
  skillName: string;
  disabled?: boolean;
}

type JobStatus = "idle" | "uploading" | "running" | "completed" | "failed";

interface JobResult {
  status: string;
  content?: Array<{ type: string; url?: string; format?: string }>;
  error?: { code: string; message: string };
  usage?: { duration_ms: number };
}

export function SkillTryIt({ skillName, disabled = false }: SkillTryItProps) {
  const [form, setForm] = useState<SkillFormResponse | null>(null);
  const [formLoading, setFormLoading] = useState(true);

  // Content state: keyed by role
  const [contentData, setContentData] = useState<Record<string, string>>({});
  const [contentNames, setContentNames] = useState<Record<string, string>>({});

  // Param state: keyed by field name
  const [params, setParams] = useState<Record<string, unknown>>({});

  const [jobStatus, setJobStatus] = useState<JobStatus>("idle");
  const [result, setResult] = useState<JobResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Fetch form schema on mount
  useState(() => {
    fetch(`/v1/skills/${skillName}/form`)
      .then((r) => (r.ok ? r.json() : null))
      .then((data: SkillFormResponse | null) => {
        if (data) {
          setForm(data);
          // Initialize defaults from param mappings
          const defaults: Record<string, unknown> = {};
          for (const m of data.mappings) {
            if (m.type === "param" && m.default !== undefined && m.default !== null) {
              defaults[m.field] = m.default;
            }
            // Auto params: generate initial value
            if (m.type === "param" && m.param_type === "auto" && m.kind === "random_int") {
              defaults[m.field] = Math.floor(Math.random() * 2 ** 32);
            }
          }
          setParams(defaults);
        }
        setFormLoading(false);
      })
      .catch(() => setFormLoading(false));
  });

  const setContent = useCallback((role: string, data: string, name: string) => {
    setContentData((prev) => ({ ...prev, [role]: data }));
    setContentNames((prev) => ({ ...prev, [role]: name }));
  }, []);

  const setParam = useCallback((field: string, value: unknown) => {
    setParams((prev) => ({ ...prev, [field]: value }));
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!form) return;
    setJobStatus("uploading");
    setResult(null);
    setErrorMsg(null);

    try {
      // Build content array from content mappings
      const content: Array<{ type: string; role: string; data?: string }> = [];
      for (const m of form.mappings) {
        if (m.type !== "content") continue;
        const data = contentData[m.role];
        if (data) {
          content.push({ type: m.content_type, role: m.role, data });
        }
      }

      // Derive capability + moniker from dotted skill name ("image.upscale" → "/v1/image/skill/upscale")
      const dotIdx = skillName.indexOf(".");
      const capability = skillName.substring(0, dotIdx);
      const moniker = skillName.substring(dotIdx + 1);

      const res = await fetch(`/v1/${capability}/skill/${moniker}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ content, parameters: params }),
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

      setJobStatus("running");
      const jobId = job.id;
      const start = Date.now();

      const poll = async () => {
        if (Date.now() - start > 300_000) {
          setJobStatus("failed");
          setErrorMsg("Timeout waiting for result");
          return;
        }
        const pollRes = await fetch(`/v1/jobs/${jobId}`);
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
  }, [form, contentData, params, skillName]);

  if (formLoading) {
    return <div className="text-xs text-gray-500 py-2">Loading...</div>;
  }
  if (!form) {
    return <div className="text-xs text-red-400 py-2">Failed to load skill form</div>;
  }

  const contentMappings = form.mappings.filter((m): m is Extract<SkillMapping, { type: "content" }> => m.type === "content");
  const paramMappings = form.mappings.filter((m): m is Extract<SkillMapping, { type: "param" }> => m.type === "param");

  // Can submit: all required content slots filled + not disabled
  const requiredSlots = form.content_slots.filter((s) => s.required);
  const allFilled = requiredSlots.every((s) => contentData[s.role]);
  const busy = jobStatus === "uploading" || jobStatus === "running";
  const canSubmit = !disabled && allFilled && !busy;

  return (
    <div className="space-y-3">
      {/* Mermaid diagram */}
      {form.diagram && (
        <div className="bg-[#0d0e14] rounded px-3 py-2 border border-gray-800">
          <MermaidDiagram chart={form.diagram} />
        </div>
      )}

      {/* Content inputs — from content mappings */}
      {contentMappings.map((m) =>
        m.content_type === "image" ? (
          <ImageDropzone
            key={m.role}
            role={m.role}
            data={contentData[m.role]}
            name={contentNames[m.role]}
            onSet={setContent}
          />
        ) : (
          <TextInput
            key={m.role}
            role={m.role}
            label={m.role === "prompt" ? "Prompt" : m.role}
            value={contentData[m.role] ?? ""}
            onChange={(val) => setContent(m.role, val, "")}
          />
        ),
      )}

      {/* Parameter inputs — from param mappings */}
      {paramMappings.length > 0 && (
        <div className="flex flex-wrap gap-3">
          {paramMappings.map((m) => (
            <ParamInput key={m.field} mapping={m} value={params[m.field]} onChange={setParam} />
          ))}
        </div>
      )}

      {/* Submit */}
      <button
        onClick={handleSubmit}
        disabled={!canSubmit}
        className={`px-4 py-1.5 rounded text-xs font-medium transition-colors ${
          canSubmit
            ? "bg-blue-600 text-white hover:bg-blue-500"
            : "bg-gray-700 text-gray-500 cursor-not-allowed"
        }`}
      >
        {busy
          ? jobStatus === "uploading" ? "Submitting..." : "Processing..."
          : disabled
            ? "Waiting for instance..."
            : form.display_name}
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
            alt="Result"
            className="max-w-full rounded border border-gray-700"
          />
          <a
            href={result.content[0].url}
            download={`${skillName.replace(".", "-")}-result.png`}
            className="inline-block text-xs text-blue-400 hover:underline"
          >
            Download
          </a>
        </div>
      )}
    </div>
  );
}

// ── Content: Image Dropzone ───────────────────────────────────

function ImageDropzone({
  role,
  data,
  name,
  onSet,
}: {
  role: string;
  data?: string;
  name?: string;
  onSet: (role: string, data: string, name: string) => void;
}) {
  const fileRef = useRef<HTMLInputElement>(null);

  function handleFiles(files: FileList | null) {
    const file = files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => onSet(role, reader.result as string, file.name);
    reader.readAsDataURL(file);
  }

  return (
    <div
      className={`border-2 border-dashed rounded-lg p-4 text-center cursor-pointer transition-colors ${
        data
          ? "border-emerald-500/40 bg-emerald-500/5"
          : "border-gray-700 hover:border-gray-500 bg-[#0d0e14]"
      }`}
      onClick={() => fileRef.current?.click()}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => { e.preventDefault(); handleFiles(e.dataTransfer.files); }}
    >
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(e) => handleFiles(e.target.files)}
      />
      {data ? (
        <div className="flex items-center gap-3 justify-center">
          <img src={data} alt="Input" className="h-16 rounded border border-gray-700" />
          <div className="text-left">
            <div className="text-sm text-gray-200">{name}</div>
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
  );
}

// ── Content: Text Input ───────────────────────────────────────

function TextInput({
  role,
  label,
  value,
  onChange,
}: {
  role: string;
  label: string;
  value: string;
  onChange: (val: string) => void;
}) {
  return (
    <div className="space-y-1">
      <label className="text-[10px] text-gray-500 uppercase tracking-wider">{label}</label>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={`Enter ${label.toLowerCase()}...`}
        rows={3}
        className="w-full bg-[#0d0e14] border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-blue-500/50 resize-y"
        data-role={role}
      />
    </div>
  );
}

// ── Parameter Input (routing by param_type) ───────────────────

type ParamMapping = Extract<SkillMapping, { type: "param" }>;

function ParamInput({
  mapping,
  value,
  onChange,
}: {
  mapping: ParamMapping;
  value: unknown;
  onChange: (field: string, value: unknown) => void;
}) {
  const m = mapping;

  if (m.param_type === "options") {
    const opts = m.options;
    // Radio for ≤4 options, select for more
    if (opts.length <= 4) {
      return (
        <div className="space-y-1">
          <label className="text-[10px] text-gray-500 uppercase tracking-wider">{m.label}</label>
          <div className="flex gap-1">
            {opts.map((opt) => {
              const display = optionLabel(opt);
              const selected = JSON.stringify(value) === JSON.stringify(opt.value);
              return (
                <button
                  key={display}
                  className={`px-3 py-1 text-xs rounded border ${
                    selected
                      ? "bg-blue-500/20 border-blue-500/50 text-blue-300"
                      : "bg-[#1a1b23] border-gray-700 text-gray-400 hover:border-gray-500"
                  }`}
                  onClick={() => onChange(m.field, opt.value)}
                >
                  {display}
                </button>
              );
            })}
          </div>
        </div>
      );
    }

    return (
      <div className="space-y-1">
        <label className="text-[10px] text-gray-500 uppercase tracking-wider">{m.label}</label>
        <select
          value={String(value ?? "")}
          onChange={(e) => {
            // Find the option whose value matches the selected string
            const opt = opts.find((o) => String(o.value) === e.target.value);
            onChange(m.field, opt ? opt.value : e.target.value);
          }}
          className="bg-[#1a1b23] border border-gray-700 rounded px-2 py-1 text-xs text-gray-200 w-full max-w-[280px]"
        >
          {opts.map((opt) => (
            <option key={String(opt.value)} value={String(opt.value)}>
              {optionLabel(opt)}
            </option>
          ))}
        </select>
      </div>
    );
  }

  if (m.param_type === "range") {
    return (
      <div className="space-y-1">
        <label className="text-[10px] text-gray-500 uppercase tracking-wider">
          {m.label}: {String(value ?? m.default ?? m.min)}
        </label>
        <input
          type="range"
          min={m.min}
          max={m.max}
          step={m.step ?? 1}
          value={Number(value ?? m.default ?? m.min)}
          onChange={(e) => onChange(m.field, parseFloat(e.target.value))}
          className="w-40"
        />
      </div>
    );
  }

  if (m.param_type === "auto") {
    return (
      <div className="space-y-1">
        <label className="text-[10px] text-gray-500 uppercase tracking-wider">{m.label}</label>
        <div className="flex items-center gap-1">
          <input
            type="number"
            value={String(value ?? "")}
            onChange={(e) => onChange(m.field, parseInt(e.target.value) || 0)}
            className="bg-[#1a1b23] border border-gray-700 rounded px-2 py-1 text-xs text-gray-200 w-32 font-mono"
          />
          <button
            onClick={() => onChange(m.field, Math.floor(Math.random() * 2 ** 32))}
            className="px-2 py-1 text-[10px] rounded bg-[#1a1b23] border border-gray-700 text-gray-400 hover:border-gray-500"
            title="Generate random seed"
          >
            &#x1f3b2;
          </button>
        </div>
      </div>
    );
  }

  if (m.param_type === "text") {
    return (
      <div className="space-y-1 w-full">
        <label className="text-[10px] text-gray-500 uppercase tracking-wider">{m.label}</label>
        <textarea
          value={String(value ?? "")}
          onChange={(e) => onChange(m.field, e.target.value)}
          rows={2}
          className="w-full bg-[#0d0e14] border border-gray-700 rounded px-3 py-2 text-xs text-gray-200 placeholder-gray-600 focus:outline-none focus:border-blue-500/50 resize-y"
        />
      </div>
    );
  }

  return null;
}

// ── Helpers ───────────────────────────────────────────────────

function optionLabel(opt: ParamOption): string {
  return opt.label ?? String(opt.value);
}
