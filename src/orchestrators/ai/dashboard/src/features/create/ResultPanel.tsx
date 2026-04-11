import type { DispatchResponse, Meta } from "../../api/types";
import Markdown from "../../components/Markdown";

interface Props {
  result: unknown;
  streaming?: string;
}

export default function ResultPanel({ result, streaming }: Props) {
  if (streaming !== undefined) {
    return (
      <ResultFrame title="Streaming...">
        <Markdown content={streaming} />
      </ResultFrame>
    );
  }

  if (!result) {
    return (
      <ResultFrame title="Result">
        <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
          Send a request to see the result
        </div>
      </ResultFrame>
    );
  }

  const data = result as DispatchResponse;

  // Error response
  if (data.error) {
    return (
      <ResultFrame title="Error">
        <div className="p-4 rounded-lg bg-red-dim border border-red/20">
          <div className="text-sm font-medium text-red">{data.error.code}</div>
          <div className="text-[12px] text-text mt-1">{data.error.message}</div>
          {data.error.details != null ? (
            <pre className="text-[10px] text-text-dim mt-2 overflow-x-auto">
              {JSON.stringify(data.error.details, null, 2)}
            </pre>
          ) : null}
        </div>
        {data._meta && <MetaFooter meta={data._meta} />}
      </ResultFrame>
    );
  }

  // Success response
  const output = data.output ?? {};

  // Text response (chat, translate)
  const textResponse =
    nested(output, "text.response") ??
    nested(output, "text.translated");
  if (typeof textResponse === "string") {
    const reasoning = nested(output, "text.reasoning.content");
    return (
      <ResultFrame title="Response">
        {typeof reasoning === "string" && reasoning.length > 0 && (
          <ReasoningBlock content={reasoning} />
        )}
        <Markdown content={textResponse} />
        {data._meta && <MetaFooter meta={data._meta} />}
      </ResultFrame>
    );
  }

  // Image response
  const imageData = nested(output, "image.data") as string | undefined;
  const imageMediaId = nested(output, "image.media_id") as string | undefined;
  if (imageData || imageMediaId) {
    const src = imageData
      ? `data:image/png;base64,${imageData}`
      : `/v1/media/${imageMediaId}`;
    return (
      <ResultFrame title="Image">
        <img src={src} alt="Generated" className="w-full rounded-lg" />
        {data._meta && <MetaFooter meta={data._meta} />}
      </ResultFrame>
    );
  }

  // Audio response
  const audioData = nested(output, "audio.data") as string | undefined;
  const audioMediaId = nested(output, "audio.media_id") as string | undefined;
  if (audioData || audioMediaId) {
    const src = audioData
      ? `data:audio/wav;base64,${audioData}`
      : `/v1/media/${audioMediaId}`;
    return (
      <ResultFrame title="Audio">
        <audio controls src={src} className="w-full" />
        {data._meta && <MetaFooter meta={data._meta} />}
      </ResultFrame>
    );
  }

  // Embedding response
  const embedDimensions = nested(output, "text.dimensions") as number | undefined;
  if (embedDimensions !== undefined) {
    return (
      <ResultFrame title="Embedding">
        <div className="text-sm text-text-dim">{embedDimensions} dimensions</div>
        {data._meta && <MetaFooter meta={data._meta} />}
      </ResultFrame>
    );
  }

  // Fallback: raw JSON
  return (
    <ResultFrame title="Result">
      <pre className="text-[11px] text-text-dim overflow-auto whitespace-pre-wrap">
        {JSON.stringify(output, null, 2)}
      </pre>
      {data._meta && <MetaFooter meta={data._meta} />}
    </ResultFrame>
  );
}

function ResultFrame({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-3.5 border-b border-border text-[11px] uppercase tracking-wider text-text-dimmer font-semibold">
        {title}
      </div>
      <div className="flex-1 p-5 overflow-y-auto">{children}</div>
    </div>
  );
}

function MetaFooter({ meta }: { meta: Meta }) {
  const latency =
    meta.received_at && meta.completed_at
      ? `${new Date(meta.completed_at).getTime() - new Date(meta.received_at).getTime()}ms`
      : null;

  return (
    <div className="flex gap-2 flex-wrap mt-4 pt-3 border-t border-border">
      {meta.provider && <MetaBadge text={meta.provider} />}
      {meta.model && <MetaBadge text={meta.model} />}
      {latency && <MetaBadge text={latency} />}
      <MetaBadge text={meta.mode} />
    </div>
  );
}

function MetaBadge({ text }: { text: string }) {
  return (
    <span className="text-[10px] text-text-dimmer bg-surface-2 px-2 py-0.5 rounded">
      {text}
    </span>
  );
}

/**
 * Collapsible reasoning block — renders the reasoning model's
 * chain-of-thought (from `text.reasoning.content`) above the final
 * answer. Closed by default so the answer stays in focus; click to
 * expand. Styled as subtle context, not a primary answer.
 */
function ReasoningBlock({ content }: { content: string }) {
  return (
    <details className="mb-4 border-l-2 border-accent/40 pl-3 text-text-dim">
      <summary className="cursor-pointer text-[10px] uppercase tracking-wider text-text-dimmer font-semibold select-none">
        Reasoning
      </summary>
      <div className="text-[12px] mt-2 whitespace-pre-wrap font-mono">
        {content}
      </div>
    </details>
  );
}

/** Access a nested value via dotted path. */
function nested(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}
