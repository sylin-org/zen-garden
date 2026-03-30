import { useState, useRef, useCallback } from "react";
import type { ModelStatus } from "../types";

// ── Props ───────────────────────────────────────────────────────

interface TryItProps {
  capability: string;
  models: ModelStatus[];
}

// ── Shared state shape ──────────────────────────────────────────

interface RequestState {
  loading: boolean;
  error: string | null;
}

// ── Speak helper (reused by chat, translate, transcribe) ────────

function SpeakButton({ text }: { text: string }) {
  const [speaking, setSpeaking] = useState(false);
  const audioRef = useRef<HTMLAudioElement>(null);

  async function handleSpeak() {
    setSpeaking(true);
    try {
      const res = await fetch("/v1/audio/speech", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "tts-1",
          input: text,
          voice: "alloy",
          response_format: "wav",
        }),
      });
      if (!res.ok) throw new Error(await parseApiError(res));
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      if (audioRef.current) {
        audioRef.current.src = url;
        audioRef.current.play();
      }
    } catch {
      // Silently fail — the main panel shows errors
    } finally {
      setSpeaking(false);
    }
  }

  return (
    <span className="inline-flex items-center gap-2">
      <button
        onClick={handleSpeak}
        disabled={speaking}
        className="text-[11px] px-2 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-purple-500/20 hover:text-purple-300 disabled:opacity-40"
      >
        {speaking ? "Speaking..." : "Speak this"}
      </button>
      <audio ref={audioRef} className="hidden" />
    </span>
  );
}

// ── Model selector ──────────────────────────────────────────────

function ModelSelector({
  models,
  selected,
  onSelect,
}: {
  models: ModelStatus[];
  selected: string;
  onSelect: (name: string) => void;
}) {
  if (models.length === 0) return null;
  return (
    <select
      value={selected}
      onChange={(e) => onSelect(e.target.value)}
      className="bg-[#1a1b23] border border-[#2e303a] rounded px-2 py-1.5 text-[12px] text-gray-300 focus:outline-none focus:border-blue-500/50"
    >
      {models.map((m) => (
        <option key={m.model} value={m.model}>
          {m.model}
        </option>
      ))}
    </select>
  );
}

// ── Error parsing ──────────────────────────────────────────────

/**
 * Parse a structured error response from the unified API.
 *
 * The backend always returns: `{ error: { code, message, status } }`
 * This function extracts the human-readable message.
 */
async function parseApiError(res: Response): Promise<string> {
  try {
    const body = await res.json();
    return body?.error?.message ?? `Error ${res.status}`;
  } catch {
    return `Error ${res.status}`;
  }
}

// ── Error / Loading display ─────────────────────────────────────

function StatusBar({ state }: { state: RequestState }) {
  if (state.loading) {
    return (
      <div className="flex items-center gap-2 text-[12px] text-gray-500">
        <span className="inline-block w-3 h-3 border-2 border-gray-500 border-t-transparent rounded-full animate-spin" />
        Running...
      </div>
    );
  }
  if (state.error) {
    return (
      <div className="text-[12px] text-red-400 bg-red-400/5 border border-red-400/20 rounded px-3 py-2">
        {state.error}
      </div>
    );
  }
  return null;
}

// ── SSE stream reader helper ────────────────────────────────────

async function readSseStream(
  response: Response,
  onChunk: (content: string) => void,
): Promise<void> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("No response body");

  const decoder = new TextDecoder();
  let buffer = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      // Keep the last incomplete line in the buffer
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
          // Skip non-JSON lines
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

// ── Chat / Tools / Think panel ──────────────────────────────────

function ChatPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [prompt, setPrompt] = useState("");
  const [response, setResponse] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });
  const abortRef = useRef<AbortController | null>(null);

  async function send() {
    if (!prompt.trim() || !model) return;
    setState({ loading: true, error: null });
    setResponse("");
    setStreaming(true);

    abortRef.current = new AbortController();

    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [{ role: "user", content: prompt }],
          stream: true,
        }),
        signal: abortRef.current.signal,
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }

      await readSseStream(res, (chunk) => {
        setResponse((prev) => prev + chunk);
      });

      setState({ loading: false, error: null });
    } catch (e: unknown) {
      if (e instanceof DOMException && e.name === "AbortError") {
        setState({ loading: false, error: null });
      } else {
        setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
      }
    } finally {
      setStreaming(false);
      abortRef.current = null;
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <button
          onClick={send}
          disabled={state.loading || !prompt.trim()}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Send
        </button>
      </div>
      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder="Enter a message..."
        rows={3}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      {streaming && (
        <div className="flex items-center gap-2 text-[12px] text-gray-500">
          <span className="inline-block w-3 h-3 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
          Streaming...
        </div>
      )}
      {!streaming && <StatusBar state={state} />}
      {response && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap max-h-80 overflow-y-auto">
            {response}
          </pre>
          {!streaming && <SpeakButton text={response} />}
        </div>
      )}
    </div>
  );
}

// ── Embed panel ─────────────────────────────────────────────────

interface EmbedResult {
  dimensions: number;
  preview: number[];
  usage: { prompt_tokens?: number; total_tokens?: number } | null;
}

function EmbedPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [text, setText] = useState("");
  const [result, setResult] = useState<EmbedResult | null>(null);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function embed() {
    if (!text.trim() || !model) return;
    setState({ loading: true, error: null });
    setResult(null);
    try {
      const res = await fetch("/v1/embeddings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ model, input: text }),
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }
      const data = await res.json();
      const embedding: number[] = data.data?.[0]?.embedding ?? [];
      setResult({
        dimensions: embedding.length,
        preview: embedding.slice(0, 5),
        usage: data.usage ?? null,
      });
      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <button
          onClick={embed}
          disabled={state.loading || !text.trim()}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Embed
        </button>
      </div>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Enter text to embed..."
        rows={2}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      <StatusBar state={state} />
      {result && (
        <div className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 space-y-1">
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
          {result.usage && (
            <div>
              <span className="text-gray-500">Tokens:</span>{" "}
              <span className="font-mono text-[11px]">
                {result.usage.prompt_tokens ?? result.usage.total_tokens ?? "-"}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Vision panel ────────────────────────────────────────────────

function VisionPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [prompt, setPrompt] = useState("");
  const [imageUrl, setImageUrl] = useState("");
  const [response, setResponse] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function send() {
    if (!prompt.trim() || !model || !imageUrl.trim()) return;
    setState({ loading: true, error: null });
    setResponse("");
    setStreaming(true);
    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [
            {
              role: "user",
              content: [
                { type: "text", text: prompt },
                { type: "image_url", image_url: { url: imageUrl } },
              ],
            },
          ],
          stream: true,
        }),
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }

      await readSseStream(res, (chunk) => {
        setResponse((prev) => prev + chunk);
      });

      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    } finally {
      setStreaming(false);
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <button
          onClick={send}
          disabled={state.loading || !prompt.trim() || !imageUrl.trim()}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Send
        </button>
      </div>
      <input
        value={imageUrl}
        onChange={(e) => setImageUrl(e.target.value)}
        placeholder="Image URL (https://...)"
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-1.5 text-[12px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500/50"
      />
      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder="Describe what you want to know about the image..."
        rows={2}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      {streaming && (
        <div className="flex items-center gap-2 text-[12px] text-gray-500">
          <span className="inline-block w-3 h-3 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
          Streaming...
        </div>
      )}
      {!streaming && <StatusBar state={state} />}
      {response && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap max-h-80 overflow-y-auto">
            {response}
          </pre>
          {!streaming && <SpeakButton text={response} />}
        </div>
      )}
    </div>
  );
}

// ── Speak panel ─────────────────────────────────────────────────

const VOICES = ["alloy", "echo", "fable", "onyx", "nova", "shimmer"] as const;
const FORMATS = ["wav", "mp3"] as const;

function SpeakPanel() {
  const [text, setText] = useState("");
  const [voice, setVoice] = useState<string>("alloy");
  const [format, setFormat] = useState<string>("wav");
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function speak() {
    if (!text.trim()) return;
    setState({ loading: true, error: null });
    setAudioUrl(null);
    try {
      const res = await fetch("/v1/audio/speech", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "tts-1",
          input: text,
          voice,
          response_format: format,
        }),
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }
      const blob = await res.blob();
      setAudioUrl(URL.createObjectURL(blob));
      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end flex-wrap">
        <select
          value={voice}
          onChange={(e) => setVoice(e.target.value)}
          className="bg-[#1a1b23] border border-[#2e303a] rounded px-2 py-1.5 text-[12px] text-gray-300 focus:outline-none focus:border-blue-500/50"
        >
          {VOICES.map((v) => (
            <option key={v} value={v}>{v}</option>
          ))}
        </select>
        <select
          value={format}
          onChange={(e) => setFormat(e.target.value)}
          className="bg-[#1a1b23] border border-[#2e303a] rounded px-2 py-1.5 text-[12px] text-gray-300 focus:outline-none focus:border-blue-500/50"
        >
          {FORMATS.map((f) => (
            <option key={f} value={f}>{f}</option>
          ))}
        </select>
        <button
          onClick={speak}
          disabled={state.loading || !text.trim()}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Speak
        </button>
      </div>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Enter text to speak..."
        rows={3}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      <StatusBar state={state} />
      {audioUrl && (
        <audio controls src={audioUrl} className="w-full h-8" />
      )}
    </div>
  );
}

// ── Transcribe panel ────────────────────────────────────────────

function TranscribePanel() {
  const [file, setFile] = useState<File | null>(null);
  const [result, setResult] = useState("");
  const [state, setState] = useState<RequestState>({ loading: false, error: null });
  const inputRef = useRef<HTMLInputElement>(null);

  async function transcribe() {
    if (!file) return;
    setState({ loading: true, error: null });
    setResult("");
    try {
      const formData = new FormData();
      formData.append("file", file);
      formData.append("model", "whisper-1");
      const res = await fetch("/v1/audio/transcriptions", {
        method: "POST",
        body: formData,
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }
      const data = await res.json();
      setResult(data.text ?? JSON.stringify(data, null, 2));
      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end">
        <input
          ref={inputRef}
          type="file"
          accept="audio/*"
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          className="text-[12px] text-gray-400 file:mr-2 file:py-1 file:px-3 file:rounded file:border-0 file:text-[12px] file:bg-[#2e303a] file:text-gray-300 file:cursor-pointer"
        />
        <button
          onClick={transcribe}
          disabled={state.loading || !file}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Transcribe
        </button>
      </div>
      <StatusBar state={state} />
      {result && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap">
            {result}
          </pre>
          <SpeakButton text={result} />
        </div>
      )}
    </div>
  );
}

// ── Translate panel ─────────────────────────────────────────────

const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "es", name: "Spanish" },
  { code: "fr", name: "French" },
  { code: "de", name: "German" },
  { code: "it", name: "Italian" },
  { code: "pt", name: "Portuguese" },
  { code: "ja", name: "Japanese" },
  { code: "ko", name: "Korean" },
  { code: "zh", name: "Chinese" },
  { code: "ru", name: "Russian" },
  { code: "ar", name: "Arabic" },
  { code: "hi", name: "Hindi" },
] as const;

function TranslatePanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [text, setText] = useState("");
  const [source, setSource] = useState("en");
  const [target, setTarget] = useState("es");
  const [result, setResult] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  const sourceName = LANGUAGES.find((l) => l.code === source)?.name ?? source;
  const targetName = LANGUAGES.find((l) => l.code === target)?.name ?? target;

  async function translate() {
    if (!text.trim()) return;
    setState({ loading: true, error: null });
    setResult("");
    setStreaming(true);
    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: model || undefined,
          messages: [
            {
              role: "system",
              content: `You are a professional translator. Translate the following text from ${sourceName} to ${targetName}. Output only the translation, no explanations.`,
            },
            { role: "user", content: text },
          ],
          stream: true,
        }),
      });
      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }

      await readSseStream(res, (chunk) => {
        setResult((prev) => prev + chunk);
      });

      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    } finally {
      setStreaming(false);
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end flex-wrap">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <select
          value={source}
          onChange={(e) => setSource(e.target.value)}
          className="bg-[#1a1b23] border border-[#2e303a] rounded px-2 py-1.5 text-[12px] text-gray-300 focus:outline-none focus:border-blue-500/50"
        >
          {LANGUAGES.map((l) => (
            <option key={l.code} value={l.code}>{l.name}</option>
          ))}
        </select>
        <span className="text-gray-600 text-[12px]">to</span>
        <select
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          className="bg-[#1a1b23] border border-[#2e303a] rounded px-2 py-1.5 text-[12px] text-gray-300 focus:outline-none focus:border-blue-500/50"
        >
          {LANGUAGES.map((l) => (
            <option key={l.code} value={l.code}>{l.name}</option>
          ))}
        </select>
        <button
          onClick={translate}
          disabled={state.loading || !text.trim()}
          className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
        >
          Translate
        </button>
      </div>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Enter text to translate..."
        rows={3}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      {streaming && (
        <div className="flex items-center gap-2 text-[12px] text-gray-500">
          <span className="inline-block w-3 h-3 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
          Translating...
        </div>
      )}
      {!streaming && <StatusBar state={state} />}
      {result && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap">
            {result}
          </pre>
          {!streaming && <SpeakButton text={result} />}
        </div>
      )}
    </div>
  );
}

// ── Image generation panel ──────────────────────────────────────

function ImagePanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [prompt, setPrompt] = useState("");
  const [images, setImages] = useState<string[]>([]);
  const [text, setText] = useState("");
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function generate() {
    if (!prompt.trim() || !model) return;
    setState({ loading: true, error: null });
    setImages([]);
    setText("");

    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [{ role: "user", content: prompt }],
          stream: false,
        }),
      });

      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }

      const data = await res.json();
      const content = data.choices?.[0]?.message?.content;

      if (!content) {
        throw new Error("No content in response");
      }

      // Content may be a string (text only) or array of parts (text + images)
      if (typeof content === "string") {
        setText(content);
      } else if (Array.isArray(content)) {
        const newImages: string[] = [];
        const textParts: string[] = [];

        for (const part of content) {
          if (part.type === "text" && part.text) {
            textParts.push(part.text);
          } else if (part.type === "image_url" && part.image_url?.url) {
            newImages.push(part.image_url.url);
          }
        }

        setImages(newImages);
        if (textParts.length > 0) setText(textParts.join("\n"));
      }

      setState({ loading: false, error: null });
    } catch (e) {
      setState({
        loading: false,
        error: e instanceof Error ? e.message : "Unknown error",
      });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <button
          onClick={generate}
          disabled={state.loading || !prompt.trim()}
          className="px-4 py-1.5 text-[12px] rounded bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {state.loading ? "Generating..." : "Generate"}
        </button>
      </div>

      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder="Describe the image you want to generate..."
        rows={3}
        className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-200 resize-vertical focus:border-blue-500/50 focus:outline-none"
      />

      <StatusBar state={state} />

      {text && (
        <div className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap">
          {text}
        </div>
      )}

      {images.length > 0 && (
        <div className="grid grid-cols-1 gap-3">
          {images.map((src, idx) => (
            <div
              key={idx}
              className="border border-[#2e303a] rounded overflow-hidden bg-[#0f1117]"
            >
              <img
                src={src}
                alt={`Generated image ${idx + 1}`}
                className="max-w-full h-auto"
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Rerank panel ────────────────────────────────────────────────

function RerankPanel() {
  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-6 text-center">
      <p className="text-[12px] text-gray-500">
        Reranking is not available via the unified API.
      </p>
      <p className="text-[11px] text-gray-600 mt-1">
        Use the provider-specific endpoint directly, or leverage embedding similarity.
      </p>
    </div>
  );
}

// ── Placeholder for ComfyUI capabilities ────────────────────────

// ── Media generation panel (video, music) ──────────────────────
// Uses /v1/chat/completions — the provider handles responseModalities
// and returns media as content parts or audio data.

function MediaPanel({ models, mediaType }: { models: ModelStatus[]; mediaType: "video" | "music" }) {
  const [model, setModel] = useState(models[0]?.model ?? "");
  const [prompt, setPrompt] = useState("");
  const [result, setResult] = useState<string | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  const placeholder = mediaType === "video"
    ? "Describe the video you want to generate..."
    : "Describe the music you want to generate (genre, mood, instruments)...";

  async function generate() {
    if (!prompt.trim() || !model) return;
    setState({ loading: true, error: null });
    setResult(null);
    setAudioUrl(null);

    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [{ role: "user", content: prompt }],
          stream: false,
        }),
      });

      if (!res.ok) {
        throw new Error(await parseApiError(res));
      }

      const data = await res.json();
      const content = data.choices?.[0]?.message?.content;

      if (!content) throw new Error("No content in response");

      if (typeof content === "string") {
        setResult(content);
      } else if (Array.isArray(content)) {
        const textParts: string[] = [];
        for (const part of content) {
          if (part.type === "text" && part.text) {
            textParts.push(part.text);
          } else if (part.type === "image_url" && part.image_url?.url) {
            // Audio/video may come as data URIs
            const url = part.image_url.url;
            if (url.startsWith("data:audio/") || url.startsWith("data:video/")) {
              setAudioUrl(url);
            }
          }
        }
        if (textParts.length > 0) setResult(textParts.join("\n"));
      }

      setState({ loading: false, error: null });
    } catch (e) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Unknown error" });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-3">
        <ModelSelector models={models} selected={model} onSelect={setModel} />
        <button
          onClick={generate}
          disabled={state.loading || !prompt.trim()}
          className="px-4 py-1.5 text-[12px] rounded bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {state.loading ? "Generating..." : "Generate"}
        </button>
      </div>

      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder={placeholder}
        rows={3}
        className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-200 resize-vertical focus:border-blue-500/50 focus:outline-none"
      />

      <StatusBar state={state} />

      {result && (
        <div className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap">
          {result}
        </div>
      )}

      {audioUrl && (
        <div className="bg-[#0f1117] border border-[#2e303a] rounded p-3">
          {mediaType === "video" ? (
            <video controls src={audioUrl} className="max-w-full h-auto rounded" />
          ) : (
            <audio controls src={audioUrl} className="w-full" />
          )}
        </div>
      )}
    </div>
  );
}

// ── Panel router ────────────────────────────────────────────────

function PanelForCapability({ capability, models }: TryItProps) {
  switch (capability) {
    case "chat":
    case "tools":
    case "think":
      return <ChatPanel models={models} />;
    case "embed":
      return <EmbedPanel models={models} />;
    case "vision":
      return <VisionPanel models={models} />;
    case "speech":
      return <SpeakPanel />;
    case "transcribe":
      return <TranscribePanel />;
    case "translate":
      return <TranslatePanel models={models} />;
    case "rerank":
      return <RerankPanel />;
    case "image":
      return <ImagePanel models={models} />;
    case "video":
      return <MediaPanel models={models} mediaType="video" />;
    case "music":
      return <MediaPanel models={models} mediaType="music" />;
    default:
      return (
        <p className="text-[12px] text-gray-500">
          No test panel available for this capability.
        </p>
      );
  }
}

// ── Main component ──────────────────────────────────────────────

export function TryIt({ capability, models }: TryItProps) {
  const [expanded, setExpanded] = useState(false);

  const toggle = useCallback(() => setExpanded((prev) => !prev), []);

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
      <button
        onClick={toggle}
        className="w-full flex items-center justify-between px-4 py-2.5 text-left hover:bg-[#22232d] transition-colors"
      >
        <span className="text-[13px] font-medium text-gray-300">Try It</span>
        <svg
          className={`w-4 h-4 text-gray-500 transition-transform ${expanded ? "rotate-180" : ""}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {expanded && (
        <div className="px-4 pb-4 pt-1 border-t border-[#2e303a]/50">
          <PanelForCapability capability={capability} models={models} />
        </div>
      )}
    </div>
  );
}
