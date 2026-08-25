import { useMemo } from "react";
import { marked } from "marked";

// Configure marked for safe rendering — no HTML passthrough.
marked.setOptions({
  breaks: true,
  gfm: true,
});

interface Props {
  content: string;
  className?: string;
}

/**
 * Renders markdown text as HTML. Used in result panels,
 * dialogue bubbles, and history previews.
 */
export default function Markdown({ content, className }: Props) {
  const html = useMemo(() => {
    try {
      return marked.parse(content) as string;
    } catch {
      return content;
    }
  }, [content]);

  return (
    <div
      className={[
        "prose prose-invert prose-sm max-w-none",
        "prose-p:my-1 prose-headings:my-2 prose-ul:my-1 prose-ol:my-1",
        "prose-pre:bg-surface-3 prose-pre:rounded-lg prose-pre:p-3",
        "prose-code:text-accent prose-code:text-[12px]",
        "prose-a:text-accent prose-a:no-underline hover:prose-a:underline",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
