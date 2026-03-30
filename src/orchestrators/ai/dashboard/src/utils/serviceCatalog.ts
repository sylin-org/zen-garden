// Local AI service catalog — all offerings the orchestrator supports.

export interface ServiceCatalogEntry {
  id: string;
  name: string;
  description: string;
  capabilities: readonly string[];
  port: number;
  gpuRequired: boolean;
  cpuVariant: boolean;
  dockerImage: string;
  docsUrl: string;
}

export const SERVICE_CATALOG: readonly ServiceCatalogEntry[] = [
  {
    id: "ollama",
    name: "Ollama",
    description: "LLM inference server — chat, vision, tools, embeddings",
    capabilities: ["chat", "think", "tools", "vision", "embed", "transcribe"],
    port: 11434,
    gpuRequired: false,
    cpuVariant: true,
    dockerImage: "ollama/ollama",
    docsUrl: "https://ollama.com/",
  },
  {
    id: "speaches",
    name: "Speaches",
    description: "STT + TTS server — OpenAI-compatible speech processing (Whisper + Kokoro)",
    capabilities: ["transcribe", "speech"],
    port: 8000,
    gpuRequired: false,
    cpuVariant: true,
    dockerImage: "ghcr.io/speaches-ai/speaches",
    docsUrl: "https://speaches.ai/",
  },
  {
    id: "openedai-speech",
    name: "OpenedAI Speech",
    description: "Text-to-speech server — XTTS v2 and Piper backends",
    capabilities: ["speech"],
    port: 8001,
    gpuRequired: false,
    cpuVariant: false,
    dockerImage: "ghcr.io/matatonic/openedai-speech",
    docsUrl: "https://github.com/matatonic/openedai-speech",
  },
  {
    id: "infinity",
    name: "Infinity",
    description: "High-performance embedding + reranking server",
    capabilities: ["embed", "rerank"],
    port: 7997,
    gpuRequired: false,
    cpuVariant: true,
    dockerImage: "michaelf34/infinity",
    docsUrl: "https://github.com/michaelfeil/infinity",
  },
  {
    id: "comfyui",
    name: "ComfyUI",
    description: "Node-based image generation workflow engine (Stable Diffusion)",
    capabilities: ["image"],
    port: 8188,
    gpuRequired: true,
    cpuVariant: false,
    dockerImage: "comfyanonymous/comfyui",
    docsUrl: "https://github.com/comfyanonymous/ComfyUI",
  },
  {
    id: "libretranslate",
    name: "LibreTranslate",
    description: "Self-hosted machine translation — 50+ languages",
    capabilities: ["translate"],
    port: 5000,
    gpuRequired: false,
    cpuVariant: false,
    dockerImage: "libretranslate/libretranslate",
    docsUrl: "https://libretranslate.com/",
  },
  {
    id: "whispercpp",
    name: "whisper.cpp",
    description: "Lightweight C++ speech-to-text — optimized for CPU and ARM",
    capabilities: ["transcribe"],
    port: 8080,
    gpuRequired: false,
    cpuVariant: false,
    dockerImage: "ghcr.io/ggml-org/whisper.cpp",
    docsUrl: "https://github.com/ggml-org/whisper.cpp",
  },
] as const;

export function findServiceEntry(id: string): ServiceCatalogEntry | undefined {
  return SERVICE_CATALOG.find((s) => s.id === id);
}
