import { useState, useCallback } from "react";

interface Props {
  url: string;
  /** The payload object — already nested, ready to serialize. */
  values: Record<string, unknown>;
}

export default function CopyAsCurl({ url, values }: Props) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    const json = JSON.stringify(values, null, 2);
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
