import { useState } from "react";
import type { DashboardStatus } from "../types";

interface CloudProps {
  status: DashboardStatus | null;
}

// Known cloud provider catalog — what's available to set up
const CLOUD_CATALOG = [
  {
    id: "openai",
    name: "OpenAI",
    baseUrl: "https://api.openai.com",
    capabilities: ["chat", "embed", "vision", "imagine", "speak", "transcribe"],
    description: "GPT-4o, DALL-E, Whisper, TTS — full-spectrum AI",
    keyPrefix: "sk-",
  },
  {
    id: "anthropic",
    name: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    capabilities: ["chat", "vision", "tools", "think"],
    description: "Claude Sonnet, Opus, Haiku — advanced reasoning",
    keyPrefix: "sk-ant-",
  },
  {
    id: "google",
    name: "Google AI",
    baseUrl: "https://generativelanguage.googleapis.com",
    capabilities: ["chat", "embed", "vision", "speak"],
    description: "Gemini models — multimodal AI",
    keyPrefix: "AI",
  },
  {
    id: "cohere",
    name: "Cohere",
    baseUrl: "https://api.cohere.com",
    capabilities: ["chat", "embed", "rerank"],
    description: "Command, Embed, Rerank — enterprise NLP",
    keyPrefix: "",
  },
  {
    id: "deepgram",
    name: "Deepgram",
    baseUrl: "https://api.deepgram.com",
    capabilities: ["transcribe", "speak"],
    description: "Speech-to-text and text-to-speech",
    keyPrefix: "",
  },
  {
    id: "stability-ai",
    name: "Stability AI",
    baseUrl: "https://api.stability.ai",
    capabilities: ["imagine"],
    description: "Stable Diffusion — image generation",
    keyPrefix: "sk-",
  },
  {
    id: "elevenlabs",
    name: "ElevenLabs",
    baseUrl: "https://api.elevenlabs.io",
    capabilities: ["speak"],
    description: "Premium voice synthesis",
    keyPrefix: "",
  },
] as const;

const CAP_COLORS: Record<string, string> = {
  chat: "bg-emerald-500/20 text-emerald-400",
  embed: "bg-blue-500/20 text-blue-400",
  vision: "bg-amber-500/20 text-amber-400",
  tools: "bg-cyan-500/20 text-cyan-400",
  think: "bg-purple-500/20 text-purple-400",
  imagine: "bg-pink-500/20 text-pink-400",
  speak: "bg-orange-500/20 text-orange-400",
  transcribe: "bg-teal-500/20 text-teal-400",
  rerank: "bg-indigo-500/20 text-indigo-400",
};

interface ConfiguredProvider {
  name: string;
  kind: string;
  base_url: string;
  masked_key: string;
  enabled: boolean;
  priority: number;
  capabilities: string[];
  model_count: number;
}

export function Cloud({ status: _status }: CloudProps) {
  const [providers, setProviders] = useState<ConfiguredProvider[]>([]);
  const [configuring, setConfiguring] = useState<string | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [priority, setPriority] = useState(-10);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    valid: boolean;
    message: string;
  } | null>(null);
  const [loaded, setLoaded] = useState(false);

  // Fetch configured providers
  if (!loaded) {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((data) => {
        setProviders(data);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }

  const getConfigured = (id: string) =>
    providers.find((p) => p.name === id);

  async function saveProvider(catalogEntry: typeof CLOUD_CATALOG[number]) {
    if (!apiKey.trim()) return;
    setSaving(true);
    try {
      await fetch("/api/providers", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          kind: catalogEntry.id,
          name: catalogEntry.id,
          api_key: apiKey.trim(),
          base_url: catalogEntry.baseUrl,
          enabled: true,
          priority,
          capabilities: catalogEntry.capabilities,
          models: [],
        }),
      });
      setConfiguring(null);
      setApiKey("");
      setPriority(-10);
      setLoaded(false); // re-fetch
    } finally {
      setSaving(false);
    }
  }

  async function removeProvider(name: string) {
    await fetch(`/api/providers/${name}`, { method: "DELETE" });
    setLoaded(false); // re-fetch
  }

  async function testKey(providerId: string, baseUrl: string) {
    if (!apiKey.trim()) return;
    setTesting(true);
    setTestResult(null);
    try {
      const resp = await fetch("/api/providers/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: providerId,
          api_key: apiKey.trim(),
          base_url: baseUrl,
        }),
      });
      const data = await resp.json();
      setTestResult({ valid: data.valid, message: data.message });
    } catch (e) {
      setTestResult({
        valid: false,
        message: `request failed: ${e instanceof Error ? e.message : "unknown"}`,
      });
    } finally {
      setTesting(false);
    }
  }

  return (
    <div className="p-6 max-w-5xl">
      <h2 className="text-xl font-semibold text-gray-100 mb-1">
        Cloud Providers
      </h2>
      <p className="text-sm text-gray-400 mb-6">
        Add cloud AI providers as fallback or supplementary capability sources.
        Cloud providers are used at priority -10 by default — local instances
        are always preferred when available.
      </p>

      <div className="space-y-3">
        {CLOUD_CATALOG.map((provider) => {
          const configured = getConfigured(provider.id);
          const isActive = configuring === provider.id;

          return (
            <div
              key={provider.id}
              className={`rounded-lg border p-4 ${
                configured
                  ? "border-purple-500/40 bg-[#1a1b23]"
                  : "border-[#2e303a] bg-[#1a1b23]/50"
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <div className="flex items-center gap-3 mb-1">
                    <h3 className="text-sm font-semibold text-gray-100">
                      {provider.name}
                    </h3>
                    {configured ? (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 font-mono">
                        configured
                      </span>
                    ) : (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-400">
                        not configured
                      </span>
                    )}
                    {configured && (
                      <span className="text-[10px] text-gray-500 font-mono">
                        key: {configured.masked_key} &middot; priority:{" "}
                        {configured.priority}
                      </span>
                    )}
                  </div>

                  <p className="text-xs text-gray-500 mb-2">
                    {provider.description}
                  </p>

                  <div className="flex flex-wrap gap-1">
                    {provider.capabilities.map((cap) => (
                      <span
                        key={cap}
                        className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                          CAP_COLORS[cap] ?? "bg-gray-700 text-gray-400"
                        }`}
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>

                <div className="flex gap-2 ml-4">
                  {configured ? (
                    <>
                      <button
                        onClick={() => {
                          setConfiguring(provider.id);
                          setApiKey("");
                          setPriority(configured.priority);
                        }}
                        className="text-xs px-2 py-1 rounded bg-[#2e303a] text-gray-300 hover:bg-[#3e404a]"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => removeProvider(provider.id)}
                        className="text-xs px-2 py-1 rounded bg-red-500/10 text-red-400 hover:bg-red-500/20"
                      >
                        Remove
                      </button>
                    </>
                  ) : (
                    <button
                      onClick={() => {
                        setConfiguring(provider.id);
                        setApiKey("");
                        setPriority(-10);
                      }}
                      className="text-xs px-3 py-1 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30"
                    >
                      Add API Key
                    </button>
                  )}
                </div>
              </div>

              {isActive && (
                <div className="mt-3 pt-3 border-t border-[#2e303a]">
                  <div className="flex gap-3 items-end">
                    <div className="flex-1">
                      <label className="block text-[10px] text-gray-500 mb-1 uppercase tracking-wider">
                        API Key
                      </label>
                      <input
                        type="password"
                        value={apiKey}
                        onChange={(e) => setApiKey(e.target.value)}
                        placeholder={`${provider.keyPrefix}...`}
                        className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1.5 text-xs text-gray-200 font-mono focus:border-purple-500 focus:outline-none"
                      />
                    </div>
                    <div className="w-24">
                      <label className="block text-[10px] text-gray-500 mb-1 uppercase tracking-wider">
                        Priority
                      </label>
                      <input
                        type="number"
                        value={priority}
                        onChange={(e) =>
                          setPriority(parseInt(e.target.value) || -10)
                        }
                        className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1.5 text-xs text-gray-200 font-mono focus:border-purple-500 focus:outline-none"
                      />
                    </div>
                    <button
                      onClick={() => testKey(provider.id, provider.baseUrl)}
                      disabled={testing || !apiKey.trim()}
                      className="px-3 py-1.5 text-xs rounded bg-[#2e303a] text-gray-300 hover:bg-[#3e404a] disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {testing ? "Testing..." : "Test Key"}
                    </button>
                    <button
                      onClick={() => saveProvider(provider)}
                      disabled={saving || !apiKey.trim()}
                      className="px-3 py-1.5 text-xs rounded bg-purple-600 text-white hover:bg-purple-500 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {saving ? "Saving..." : "Save"}
                    </button>
                    <button
                      onClick={() => {
                        setConfiguring(null);
                        setTestResult(null);
                      }}
                      className="px-3 py-1.5 text-xs rounded bg-[#2e303a] text-gray-400 hover:bg-[#3e404a]"
                    >
                      Cancel
                    </button>
                  </div>
                  {testResult && (
                    <div
                      className={`mt-2 px-3 py-1.5 rounded text-xs font-mono ${
                        testResult.valid
                          ? "bg-emerald-500/10 border border-emerald-500/30 text-emerald-400"
                          : "bg-red-500/10 border border-red-500/30 text-red-400"
                      }`}
                    >
                      {testResult.valid ? "✓" : "✗"} {testResult.message}
                    </div>
                  )}
                  <p className="text-[10px] text-gray-600 mt-2">
                    Priority -10 = cloud fallback (only used when no local
                    instance serves the capability). Set to 0 for equal priority
                    with local instances, or +10 to prefer this provider.
                  </p>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
