import { useState, useEffect, useRef } from "react";
import Form from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type {
  RJSFSchema,
  UiSchema,
  WidgetProps,
  FieldTemplateProps,
  ObjectFieldTemplateProps,
} from "@rjsf/utils";

// ── Types ──────────────────────────────────────────────────────

interface ModelTryItProps {
  model: string;
  capability: string;
}

type ResultData =
  | { type: "stream"; response: Response }
  | { type: "text"; text: string }
  | { type: "audio"; url: string }
  | { type: "embed"; dimensions: number; tokens: number; preview: number[] }
  | { type: "media"; parts: MediaPart[] }
  | { type: "json"; data: unknown };

interface MediaPart {
  type: string;
  text?: string;
  image_url?: { url: string };
}

// ── Tailwind RJSF Widgets ──────────────────────────────────────

function TailwindTextWidget(props: WidgetProps) {
  return (
    <input
      type="text"
      id={props.id}
      value={props.value ?? ""}
      required={props.required}
      disabled={props.disabled}
      readOnly={props.readonly}
      onChange={(e) => props.onChange(e.target.value)}
      placeholder={props.placeholder}
      className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-200 font-mono focus:border-blue-500/50 focus:outline-none"
    />
  );
}

function TailwindTextareaWidget(props: WidgetProps) {
  const rows =
    (props.options as Record<string, unknown>)?.rows ??
    (props.schema as Record<string, unknown>)?.rows ??
    3;
  return (
    <textarea
      id={props.id}
      value={props.value ?? ""}
      required={props.required}
      disabled={props.disabled}
      readOnly={props.readonly}
      onChange={(e) => props.onChange(e.target.value)}
      placeholder={props.placeholder}
      rows={rows as number}
      className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-200 font-mono resize-vertical focus:border-blue-500/50 focus:outline-none"
    />
  );
}

function TailwindSelectWidget(props: WidgetProps) {
  const enumOptions =
    (props.options as Record<string, unknown>)?.enumOptions as
      | { value: string; label: string }[]
      | undefined;
  return (
    <select
      id={props.id}
      value={props.value ?? ""}
      required={props.required}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.target.value)}
      className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-200 font-mono focus:border-blue-500/50 focus:outline-none"
    >
      {enumOptions?.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

function TailwindRangeWidget(props: WidgetProps) {
  const min = (props.schema.minimum as number) ?? 0;
  const max = (props.schema.maximum as number) ?? 1;
  const step = (max - min) / 100 || 0.01;
  const val = (props.value as number) ?? (props.schema.default as number) ?? min;

  return (
    <div className="flex items-center gap-2">
      <input
        type="range"
        id={props.id}
        min={min}
        max={max}
        step={step}
        value={val}
        disabled={props.disabled}
        onChange={(e) => props.onChange(parseFloat(e.target.value))}
        className="flex-1 accent-blue-500"
      />
      <span className="text-xs text-gray-400 font-mono w-12 text-right">
        {val.toFixed(2)}
      </span>
    </div>
  );
}

function TailwindCheckboxWidget(props: WidgetProps) {
  return (
    <input
      type="checkbox"
      id={props.id}
      checked={!!props.value}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.target.checked)}
      className="accent-blue-500"
    />
  );
}

const TAILWIND_WIDGETS = {
  TextWidget: TailwindTextWidget,
  TextareaWidget: TailwindTextareaWidget,
  SelectWidget: TailwindSelectWidget,
  RangeWidget: TailwindRangeWidget,
  CheckboxWidget: TailwindCheckboxWidget,
};

// ── RJSF Templates ────────────────────────────────────────────

function TailwindFieldTemplate(props: FieldTemplateProps) {
  const { id, label, required, children, description, schema } = props;
  // Hide label for root object and nested objects
  const isObject = schema.type === "object";
  return (
    <div className="space-y-1">
      {label && !isObject && id !== "root" && (
        <label
          htmlFor={id}
          className="block text-[10px] text-gray-500 uppercase tracking-wider"
        >
          {label}
          {required && <span className="text-red-400 ml-0.5">*</span>}
        </label>
      )}
      {children}
      {description && !isObject && (
        <p className="text-[10px] text-gray-600">{description}</p>
      )}
    </div>
  );
}

function TailwindObjectFieldTemplate(props: ObjectFieldTemplateProps) {
  return (
    <div className="space-y-3">
      {props.properties.map((prop) => (
        <div key={prop.name}>{prop.content}</div>
      ))}
    </div>
  );
}

// ── Error parsing ──────────────────────────────────────────────

async function parseApiError(res: Response): Promise<string> {
  try {
    const body = await res.json();
    return body?.error?.message ?? `Error ${res.status}`;
  } catch {
    return `Error ${res.status}`;
  }
}

// ── SSE stream reader ──────────────────────────────────────────

async function readSseStream(
  response: Response,
  onChunk: (content: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("No response body");

  const decoder = new TextDecoder();
  let buffer = "";

  try {
    while (true) {
      if (signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed || !trimmed.startsWith("data: ")) continue;
        const payload = trimmed.slice(6);
        if (payload === "[DONE]") return;

        try {
          const parsed = JSON.parse(payload);
          const delta = parsed.choices?.[0]?.delta?.content;
          if (delta) onChunk(delta);
        } catch {
          // skip non-JSON lines
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

// ── Dispatch functions ─────────────────────────────────────────

async function dispatchChat(
  model: string,
  formData: Record<string, unknown>,
): Promise<ResultData> {
  const messages: { role: string; content: unknown }[] = [];
  if (formData.system) {
    messages.push({ role: "system", content: formData.system as string });
  }
  messages.push({ role: "user", content: formData.message as string });

  const body: Record<string, unknown> = { model, messages, stream: true };
  if (formData.temperature != null) body.temperature = formData.temperature;
  if (formData.max_tokens != null) body.max_tokens = formData.max_tokens;

  const res = await fetch("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) throw new Error(await parseApiError(res));
  return { type: "stream", response: res };
}

async function dispatchEmbed(
  model: string,
  formData: Record<string, unknown>,
): Promise<ResultData> {
  const res = await fetch("/v1/embeddings", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ model, input: formData.input }),
  });

  if (!res.ok) throw new Error(await parseApiError(res));
  const data = await res.json();
  const embedding: number[] = data.data?.[0]?.embedding ?? [];
  return {
    type: "embed",
    dimensions: embedding.length,
    preview: embedding.slice(0, 5),
    tokens: data.usage?.prompt_tokens ?? data.usage?.total_tokens ?? 0,
  };
}

async function dispatchSpeech(
  model: string,
  formData: Record<string, unknown>,
): Promise<ResultData> {
  const body: Record<string, unknown> = {
    model,
    input: formData.input,
  };
  if (formData.voice) body.voice = formData.voice;
  if (formData.speed != null) body.speed = formData.speed;
  if (formData.response_format) body.response_format = formData.response_format;

  const res = await fetch("/v1/audio/speech", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) throw new Error(await parseApiError(res));
  const blob = await res.blob();
  return { type: "audio", url: URL.createObjectURL(blob) };
}

async function dispatchTranscribe(
  model: string,
  _formData: Record<string, unknown>,
  audioFile: File | null,
): Promise<ResultData> {
  if (!audioFile) throw new Error("No audio file selected");

  const form = new FormData();
  form.append("file", audioFile);
  form.append("model", model);

  const res = await fetch("/v1/audio/transcriptions", {
    method: "POST",
    body: form,
  });

  if (!res.ok) throw new Error(await parseApiError(res));
  const data = await res.json();
  return { type: "text", text: data.text ?? JSON.stringify(data, null, 2) };
}

async function dispatchGenerate(
  model: string,
  formData: Record<string, unknown>,
): Promise<ResultData> {
  const res = await fetch("/v1/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model,
      messages: [{ role: "user", content: formData.prompt as string }],
      stream: false,
    }),
  });

  if (!res.ok) throw new Error(await parseApiError(res));
  const data = await res.json();
  const content = data.choices?.[0]?.message?.content;

  if (!content) throw new Error("No content in response");

  if (typeof content === "string") {
    return { type: "text", text: content };
  }

  if (Array.isArray(content)) {
    return { type: "media", parts: content as MediaPart[] };
  }

  return { type: "json", data: content };
}

async function dispatch(
  model: string,
  capability: string,
  formData: Record<string, unknown>,
  audioFile: File | null,
): Promise<ResultData> {
  switch (capability) {
    case "chat":
    case "think":
    case "tools":
    case "vision":
      return dispatchChat(model, formData);
    case "embed":
      return dispatchEmbed(model, formData);
    case "speech":
      return dispatchSpeech(model, formData);
    case "transcribe":
      return dispatchTranscribe(model, formData, audioFile);
    case "image":
    case "video":
    case "music":
      return dispatchGenerate(model, formData);
    default:
      throw new Error(`Unsupported capability: ${capability}`);
  }
}

// ── Result display components ──────────────────────────────────

function StreamingResult({ response }: { response: Response }) {
  const [text, setText] = useState("");
  const [done, setDone] = useState(false);

  useEffect(() => {
    const abort = new AbortController();
    readSseStream(
      response,
      (chunk) => setText((prev) => prev + chunk),
      abort.signal,
    )
      .catch(() => {})
      .finally(() => setDone(true));

    return () => abort.abort();
  }, [response]);

  return (
    <div className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-300 whitespace-pre-wrap min-h-[60px] max-h-80 overflow-y-auto">
      {text || (
        <span className="text-gray-600 animate-pulse">Generating...</span>
      )}
      {!done && text && (
        <span className="inline-block w-1.5 h-3 bg-blue-500/70 animate-pulse ml-0.5 align-text-bottom" />
      )}
    </div>
  );
}

function MediaDisplay({ parts }: { parts: MediaPart[] }) {
  return (
    <div className="space-y-2">
      {parts.map((part, i) => {
        if (part.type === "text" && part.text) {
          return (
            <div key={i} className="text-xs text-gray-300 whitespace-pre-wrap">
              {part.text}
            </div>
          );
        }
        if (part.type === "image_url" && part.image_url?.url) {
          const url = part.image_url.url;
          if (url.startsWith("data:video/")) {
            return (
              <video
                key={i}
                controls
                src={url}
                className="max-w-full rounded border border-[#2e303a]"
              />
            );
          }
          if (url.startsWith("data:audio/")) {
            return <audio key={i} controls src={url} className="w-full" />;
          }
          return (
            <img
              key={i}
              src={url}
              alt={`Generated ${i + 1}`}
              className="max-w-full rounded border border-[#2e303a]"
            />
          );
        }
        return null;
      })}
    </div>
  );
}

function ResultDisplay({
  capability,
  result,
}: {
  capability: string;
  result: ResultData;
}) {
  switch (result.type) {
    case "stream":
      return <StreamingResult response={result.response} />;
    case "text":
      return (
        <div className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-300 whitespace-pre-wrap max-h-80 overflow-y-auto">
          {result.text}
        </div>
      );
    case "audio":
      return <audio controls src={result.url} className="w-full h-8" />;
    case "embed":
      return (
        <div className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-400 space-y-1">
          <div>
            <span className="text-gray-500">Dimensions:</span>{" "}
            <span className="font-mono">{result.dimensions}</span>
          </div>
          <div>
            <span className="text-gray-500">Preview:</span>{" "}
            <span className="font-mono text-[11px]">
              [{result.preview.map((v) => v.toFixed(6)).join(", ")}
              {result.dimensions > 5 ? ", ..." : ""}]
            </span>
          </div>
          {result.tokens > 0 && (
            <div>
              <span className="text-gray-500">Tokens:</span>{" "}
              <span className="font-mono">{result.tokens}</span>
            </div>
          )}
        </div>
      );
    case "media":
      return <MediaDisplay parts={result.parts} />;
    case "json":
      return (
        <pre className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-400 whitespace-pre-wrap overflow-x-auto">
          {JSON.stringify(result.data, null, 2)}
        </pre>
      );
    default: {
      // Suppress: capability is used for potential future per-capability result rendering
      void capability;
      return null;
    }
  }
}

// ── File upload for transcribe ─────────────────────────────────

function FileUpload({ onFile }: { onFile: (file: File | null) => void }) {
  return (
    <div>
      <label className="block text-[10px] text-gray-500 uppercase tracking-wider mb-1">
        Audio File <span className="text-red-400">*</span>
      </label>
      <input
        type="file"
        accept="audio/*"
        onChange={(e) => onFile(e.target.files?.[0] ?? null)}
        className="text-xs text-gray-400 file:mr-2 file:py-1 file:px-3 file:rounded file:border-0 file:text-xs file:bg-[#2e303a] file:text-gray-300 file:cursor-pointer"
      />
    </div>
  );
}

// ── Fallback schema ────────────────────────────────────────────

function fallbackSchema(capability: string): {
  schema: RJSFSchema;
  uiSchema: UiSchema;
} {
  switch (capability) {
    case "chat":
    case "think":
    case "tools":
      return {
        schema: {
          type: "object",
          required: ["message"],
          properties: {
            message: { type: "string", title: "Message" },
            system: { type: "string", title: "System Prompt" },
            temperature: {
              type: "number",
              title: "Temperature",
              minimum: 0,
              maximum: 2,
              default: 0.7,
            },
            max_tokens: {
              type: "integer",
              title: "Max Tokens",
              minimum: 1,
              maximum: 131072,
              default: 2048,
            },
          },
        },
        uiSchema: {
          message: { "ui:widget": "textarea", "ui:options": { rows: 3 } },
          system: { "ui:widget": "textarea", "ui:options": { rows: 2 } },
          temperature: { "ui:widget": "range" },
        },
      };
    case "vision":
      return {
        schema: {
          type: "object",
          required: ["message"],
          properties: {
            message: { type: "string", title: "Message" },
            system: { type: "string", title: "System Prompt" },
            temperature: {
              type: "number",
              title: "Temperature",
              minimum: 0,
              maximum: 2,
              default: 0.7,
            },
          },
        },
        uiSchema: {
          message: { "ui:widget": "textarea", "ui:options": { rows: 3 } },
          system: { "ui:widget": "textarea", "ui:options": { rows: 2 } },
          temperature: { "ui:widget": "range" },
        },
      };
    case "embed":
      return {
        schema: {
          type: "object",
          required: ["input"],
          properties: {
            input: { type: "string", title: "Text" },
          },
        },
        uiSchema: {
          input: { "ui:widget": "textarea", "ui:options": { rows: 2 } },
        },
      };
    case "speech":
      return {
        schema: {
          type: "object",
          required: ["input"],
          properties: {
            input: { type: "string", title: "Text" },
            voice: {
              type: "string",
              title: "Voice",
              enum: ["alloy", "echo", "fable", "onyx", "nova", "shimmer"],
              default: "alloy",
            },
            speed: {
              type: "number",
              title: "Speed",
              minimum: 0.25,
              maximum: 4.0,
              default: 1.0,
            },
            response_format: {
              type: "string",
              title: "Format",
              enum: ["wav", "mp3", "opus", "flac"],
              default: "wav",
            },
          },
        },
        uiSchema: {
          input: { "ui:widget": "textarea", "ui:options": { rows: 3 } },
          speed: { "ui:widget": "range" },
        },
      };
    case "transcribe":
      return {
        schema: {
          type: "object",
          properties: {
            language: {
              type: "string",
              title: "Language",
              description: "ISO 639-1 code (optional, e.g. en, es, fr)",
            },
          },
        },
        uiSchema: {},
      };
    case "image":
    case "video":
    case "music":
      return {
        schema: {
          type: "object",
          required: ["prompt"],
          properties: {
            prompt: { type: "string", title: "Prompt" },
          },
        },
        uiSchema: {
          prompt: { "ui:widget": "textarea", "ui:options": { rows: 3 } },
        },
      };
    default:
      return {
        schema: { type: "object", properties: {} },
        uiSchema: {},
      };
  }
}

// ── Main Component ─────────────────────────────────────────────

export function ModelTryIt({ model, capability }: ModelTryItProps) {
  const [schema, setSchema] = useState<RJSFSchema | null>(null);
  const [uiSchema, setUiSchema] = useState<UiSchema>({});
  const [formData, setFormData] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<ResultData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [audioFile, setAudioFile] = useState<File | null>(null);
  const formRef = useRef<Form<Record<string, unknown>> | null>(null);

  // Fetch schema from backend, fall back to hardcoded defaults
  useEffect(() => {
    let cancelled = false;

    fetch(
      `/v1/models/${encodeURIComponent(model)}/form?capability=${encodeURIComponent(capability)}`,
    )
      .then((r) => {
        if (!r.ok) throw new Error("schema not available");
        return r.json();
      })
      .then((d) => {
        if (cancelled) return;
        setSchema(d.schema ?? d);
        setUiSchema(d.uiSchema ?? d.ui_schema ?? {});
      })
      .catch(() => {
        if (cancelled) return;
        const fb = fallbackSchema(capability);
        setSchema(fb.schema);
        setUiSchema(fb.uiSchema);
      });

    return () => {
      cancelled = true;
    };
  }, [model, capability]);

  // Reset state when model/capability changes
  useEffect(() => {
    setFormData({});
    setResult(null);
    setError(null);
    setAudioFile(null);
  }, [model, capability]);

  async function handleSubmit(data: { formData?: Record<string, unknown> }) {
    const submitted = data.formData ?? {};
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const data = await dispatch(model, capability, submitted, audioFile);
      setResult(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Request failed");
    } finally {
      setLoading(false);
    }
  }

  if (!schema) {
    return (
      <div className="text-xs text-gray-500 py-2">Loading form...</div>
    );
  }

  return (
    <div className="space-y-3 mt-3">
      <div className="text-[10px] text-gray-500 uppercase tracking-wider font-semibold">
        Try It
      </div>

      {/* File upload for transcribe (above form) */}
      {capability === "transcribe" && <FileUpload onFile={setAudioFile} />}

      {/* RJSF form */}
      <Form
        ref={formRef}
        schema={schema}
        uiSchema={uiSchema}
        formData={formData}
        onChange={({ formData: fd }) =>
          setFormData(fd as Record<string, unknown>)
        }
        onSubmit={handleSubmit}
        validator={validator}
        widgets={TAILWIND_WIDGETS}
        templates={{
          FieldTemplate: TailwindFieldTemplate,
          ObjectFieldTemplate: TailwindObjectFieldTemplate,
        }}
      >
        <button
          type="submit"
          disabled={loading}
          className="px-4 py-1.5 text-xs rounded bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
        >
          {loading ? "Running..." : "Send"}
        </button>
      </Form>

      {/* Error display */}
      {error && (
        <div className="text-xs text-red-400 bg-red-400/5 border border-red-400/20 rounded px-3 py-2">
          {error}
        </div>
      )}

      {/* Result display */}
      {result && <ResultDisplay capability={capability} result={result} />}
    </div>
  );
}
