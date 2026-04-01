import { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";

mermaid.initialize({
  startOnLoad: false,
  theme: "dark",
  themeVariables: {
    primaryColor: "#2563eb",
    primaryTextColor: "#e5e7eb",
    primaryBorderColor: "#3b82f6",
    lineColor: "#6b7280",
    secondaryColor: "#1e293b",
    tertiaryColor: "#0f172a",
    background: "#0d0e14",
    mainBkg: "#1a1b23",
    nodeBorder: "#3b82f6",
    clusterBkg: "#1a1b23",
    titleColor: "#e5e7eb",
    edgeLabelBackground: "#0d0e14",
    fontSize: "12px",
  },
  flowchart: {
    htmlLabels: true,
    curve: "monotoneX",
    padding: 12,
  },
});

interface MermaidDiagramProps {
  chart: string;
  className?: string;
}

export function MermaidDiagram({ chart, className = "" }: MermaidDiagramProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!chart || !containerRef.current) return;

    const id = `mermaid-${Date.now()}`;

    mermaid
      .render(id, chart)
      .then(({ svg: renderedSvg }) => {
        setSvg(renderedSvg);
        setError(null);
      })
      .catch((err: unknown) => {
        const message = err instanceof Error ? err.message : "Render failed";
        setError(message);
        setSvg("");
      });
  }, [chart]);

  if (error) {
    return (
      <pre className="text-[10px] text-gray-600 font-mono whitespace-pre px-3 py-2">
        {chart}
      </pre>
    );
  }

  return (
    <div
      ref={containerRef}
      className={`flex justify-center ${className}`}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
