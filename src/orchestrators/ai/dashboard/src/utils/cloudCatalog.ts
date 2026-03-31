// Cloud provider catalog — shared across CloudList, CloudDetail, CloudEdit.

export interface CloudCatalogEntry {
  id: string;
  name: string;
  baseUrl: string;
  capabilities: readonly string[];
  description: string;
  keyPrefix: string;
}

export const CLOUD_CATALOG: readonly CloudCatalogEntry[] = [
  {
    id: "openai",
    name: "OpenAI",
    baseUrl: "https://api.openai.com",
    capabilities: ["chat", "embed", "vision", "image", "speech", "transcribe"],
    description: "GPT-4o, DALL-E, Whisper, TTS",
    keyPrefix: "sk-",
  },
  {
    id: "anthropic",
    name: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    capabilities: ["chat", "vision", "tools", "think"],
    description: "Claude Sonnet, Opus, Haiku",
    keyPrefix: "sk-ant-",
  },
  {
    id: "google",
    name: "Google AI",
    baseUrl: "https://generativelanguage.googleapis.com",
    capabilities: [
      "chat",
      "think",
      "tools",
      "vision",
      "embed",
      "image",
      "speech",
      "transcribe",
      "video",
      "music",
    ],
    description: "Gemini, Nano Banana, Veo, Lyria",
    keyPrefix: "AI",
  },
  {
    id: "cohere",
    name: "Cohere",
    baseUrl: "https://api.cohere.com",
    capabilities: ["chat", "embed", "rerank"],
    description: "Command, Embed, Rerank",
    keyPrefix: "",
  },
  {
    id: "deepgram",
    name: "Deepgram",
    baseUrl: "https://api.deepgram.com",
    capabilities: ["transcribe", "speech"],
    description: "Speech-to-text and text-to-speech",
    keyPrefix: "",
  },
  {
    id: "stability-ai",
    name: "Stability AI",
    baseUrl: "https://api.stability.ai",
    capabilities: ["image"],
    description: "Stable Diffusion",
    keyPrefix: "sk-",
  },
  {
    id: "elevenlabs",
    name: "ElevenLabs",
    baseUrl: "https://api.elevenlabs.io",
    capabilities: ["speech"],
    description: "Premium voice synthesis",
    keyPrefix: "",
  },
] as const;

export const CLOUD_PROVIDER_IDS = CLOUD_CATALOG.map((c) => c.id);

export function isCloudOffering(offering: string): boolean {
  return CLOUD_PROVIDER_IDS.some(
    (c) => offering.toLowerCase() === c || offering.startsWith("cloud:"),
  );
}

export function findCatalogEntry(id: string): CloudCatalogEntry | undefined {
  return CLOUD_CATALOG.find((c) => c.id === id);
}

export const CAP_COLORS: Record<string, string> = {
  chat: "bg-emerald-500/20 text-emerald-400",
  think: "bg-purple-500/20 text-purple-400",
  tools: "bg-cyan-500/20 text-cyan-400",
  translate: "bg-sky-500/20 text-sky-400",
  vision: "bg-amber-500/20 text-amber-400",
  ocr: "bg-lime-500/20 text-lime-400",
  transcribe: "bg-teal-500/20 text-teal-400",
  embed: "bg-blue-500/20 text-blue-400",
  rerank: "bg-indigo-500/20 text-indigo-400",
  image: "bg-pink-500/20 text-pink-400",
  video: "bg-fuchsia-500/20 text-fuchsia-400",
  speech: "bg-orange-500/20 text-orange-400",
  music: "bg-rose-500/20 text-rose-400",
};
