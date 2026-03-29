import { useState, useRef, useCallback } from "react";
import type { ModelStatus } from "../types";

// ── Port mapping ────────────────────────────────────────────────

const PROXY_PORTS: Record<string, number> = {
  chat: 21434,
  embed: 21434,
  vision: 21434,
  tools: 21434,
  think: 21434,
  speak: 21437,
  transcribe: 21436,
  translate: 21439,
  rerank: 21438,
  imagine: 21435,
  edit: 21435,
  render: 21435,
};

function proxyUrl(capability: string, path: string): string {
  const port = PROXY_PORTS[capability] ?? 21434;
  return `http://${window.location.hostname}:${port}${path}`;
}

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
      const res = await fetch(proxyUrl("speak", "/v1/audio/speech"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "tts-1",
          input: text,
          voice: "alloy",
          response_format: "wav",
        }),
      });
      if (!res.ok) throw new Error(`Speech failed: ${res.status}`);
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
        <option key={m.name} value={m.name}>
          {m.name}
        </option>
      ))}
    </select>
  );
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

// ── Chat / Tools / Think panel ──────────────────────────────────

function ChatPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.name ?? "");
  const [prompt, setPrompt] = useState("");
  const [response, setResponse] = useState("");
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function send() {
    if (!prompt.trim() || !model) return;
    setState({ loading: true, error: null });
    setResponse("");
    try {
      const res = await fetch(proxyUrl("chat", "/api/chat"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [{ role: "user", content: prompt }],
          stream: false,
        }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`${res.status}: ${text.slice(0, 300)}`);
      }
      const data = await res.json();
      setResponse(data.message?.content ?? JSON.stringify(data, null, 2));
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
      <StatusBar state={state} />
      {response && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap max-h-80 overflow-y-auto">
            {response}
          </pre>
          <SpeakButton text={response} />
        </div>
      )}
    </div>
  );
}

// ── Embed panel ─────────────────────────────────────────────────

function EmbedPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.name ?? "");
  const [text, setText] = useState("");
  const [result, setResult] = useState<{ dimensions: number; preview: number[] } | null>(null);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function embed() {
    if (!text.trim() || !model) return;
    setState({ loading: true, error: null });
    setResult(null);
    try {
      const res = await fetch(proxyUrl("embed", "/api/embed"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ model, input: text }),
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`${res.status}: ${body.slice(0, 300)}`);
      }
      const data = await res.json();
      const embeddings: number[] = data.embeddings?.[0] ?? data.embedding ?? [];
      setResult({ dimensions: embeddings.length, preview: embeddings.slice(0, 5) });
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
        </div>
      )}
    </div>
  );
}

// ── Vision panel ────────────────────────────────────────────────

function VisionPanel({ models }: { models: ModelStatus[] }) {
  const [model, setModel] = useState(models[0]?.name ?? "");
  const [prompt, setPrompt] = useState("");
  const [imageUrl, setImageUrl] = useState("");
  const [response, setResponse] = useState("");
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function send() {
    if (!prompt.trim() || !model || !imageUrl.trim()) return;
    setState({ loading: true, error: null });
    setResponse("");
    try {
      const res = await fetch(proxyUrl("vision", "/api/chat"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages: [
            {
              role: "user",
              content: prompt,
              images: [imageUrl],
            },
          ],
          stream: false,
        }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`${res.status}: ${text.slice(0, 300)}`);
      }
      const data = await res.json();
      setResponse(data.message?.content ?? JSON.stringify(data, null, 2));
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
      <StatusBar state={state} />
      {response && (
        <div className="space-y-2">
          <pre className="bg-[#0d0e14] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 whitespace-pre-wrap max-h-80 overflow-y-auto">
            {response}
          </pre>
          <SpeakButton text={response} />
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
      const res = await fetch(proxyUrl("speak", "/v1/audio/speech"), {
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
        const body = await res.text();
        throw new Error(`${res.status}: ${body.slice(0, 300)}`);
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
      const res = await fetch(proxyUrl("transcribe", "/inference"), {
        method: "POST",
        body: formData,
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`${res.status}: ${body.slice(0, 300)}`);
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

function TranslatePanel() {
  const [text, setText] = useState("");
  const [source, setSource] = useState("en");
  const [target, setTarget] = useState("es");
  const [result, setResult] = useState("");
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function translate() {
    if (!text.trim()) return;
    setState({ loading: true, error: null });
    setResult("");
    try {
      const res = await fetch(proxyUrl("translate", "/translate"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ q: text, source, target }),
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`${res.status}: ${body.slice(0, 300)}`);
      }
      const data = await res.json();
      setResult(data.translatedText ?? JSON.stringify(data, null, 2));
      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-2 items-end flex-wrap">
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

// ── Rerank panel ────────────────────────────────────────────────

function RerankPanel() {
  const [query, setQuery] = useState("");
  const [docs, setDocs] = useState("");
  const [results, setResults] = useState<{ index: number; score: number; text: string }[]>([]);
  const [state, setState] = useState<RequestState>({ loading: false, error: null });

  async function rerank() {
    if (!query.trim() || !docs.trim()) return;
    setState({ loading: true, error: null });
    setResults([]);
    try {
      const documents = docs.split("\n").filter((d) => d.trim());
      const res = await fetch(proxyUrl("rerank", "/rerank"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ query, documents }),
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`${res.status}: ${body.slice(0, 300)}`);
      }
      const data = await res.json();
      const ranked = (data.results ?? data ?? []) as { index: number; relevance_score?: number; score?: number }[];
      setResults(
        ranked.map((r) => ({
          index: r.index,
          score: r.relevance_score ?? r.score ?? 0,
          text: documents[r.index] ?? "",
        })),
      );
      setState({ loading: false, error: null });
    } catch (e: unknown) {
      setState({ loading: false, error: e instanceof Error ? e.message : "Request failed" });
    }
  }

  return (
    <div className="space-y-3">
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Query..."
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-1.5 text-[12px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500/50"
      />
      <textarea
        value={docs}
        onChange={(e) => setDocs(e.target.value)}
        placeholder="Documents (one per line)..."
        rows={4}
        className="w-full bg-[#1a1b23] border border-[#2e303a] rounded px-3 py-2 text-[12px] text-gray-300 placeholder-gray-600 resize-y focus:outline-none focus:border-blue-500/50"
      />
      <button
        onClick={rerank}
        disabled={state.loading || !query.trim() || !docs.trim()}
        className="px-3 py-1.5 rounded bg-blue-600 text-white text-[12px] hover:bg-blue-500 disabled:opacity-40"
      >
        Rerank
      </button>
      <StatusBar state={state} />
      {results.length > 0 && (
        <div className="bg-[#0d0e14] border border-[#2e303a] rounded divide-y divide-[#2e303a]/50">
          {results.map((r, i) => (
            <div key={i} className="px-3 py-2 flex items-start gap-3">
              <span className="text-[11px] font-mono text-gray-500 shrink-0 w-12">
                {r.score.toFixed(4)}
              </span>
              <span className="text-[12px] text-gray-300">{r.text}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Placeholder for ComfyUI capabilities ────────────────────────

function ComingSoonPanel() {
  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-6 text-center">
      <p className="text-[12px] text-gray-500">
        Coming soon — requires ComfyUI
      </p>
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
    case "speak":
      return <SpeakPanel />;
    case "transcribe":
      return <TranscribePanel />;
    case "translate":
      return <TranslatePanel />;
    case "rerank":
      return <RerankPanel />;
    case "imagine":
    case "edit":
    case "render":
      return <ComingSoonPanel />;
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
