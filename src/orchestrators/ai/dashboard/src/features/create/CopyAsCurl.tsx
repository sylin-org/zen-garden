import { useState, useCallback } from "react";

interface Props {
  /** The dispatch URL path, e.g. "/v1/text/translate" */
  url: string;
  /** Current form values as dotted-path keys. */
  values: Record<string, unknown>;
}

export default function CopyAsCurl({ url, values }: Props) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    const payload = buildNested(values);
    const json = JSON.stringify(payload, null, 2);
    const escaped = json.replace(/'/g, "'\\''");
    const curl = `curl -X POST http://localhost:7190${url} \\\n  -H "Content-Type: application/json" \\\n  -d '${escaped}'`;

    navigator.clipboard.writeText(curl).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [url, values]);

  return (
    <button
      onClick={handleCopy}
      className="text-[10px] text-text-dimmer hover:text-accent transition-colors"
      title="Copy as curl command"
    >
      {copied ? "Copied!" : "curl"}
    </button>
  );
}

function buildNested(flat: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [dotted, value] of Object.entries(flat)) {
    if (value === undefined || value === null) continue;
    const parts = dotted.split(".");
    let current: Record<string, unknown> = result;
    for (let i = 0; i < parts.length - 1; i++) {
      const key = parts[i];
      if (typeof current[key] !== "object" || current[key] === null) {
        current[key] = {};
      }
      current = current[key] as Record<string, unknown>;
    }
    current[parts[parts.length - 1]] = value;
  }
  return result;
}
