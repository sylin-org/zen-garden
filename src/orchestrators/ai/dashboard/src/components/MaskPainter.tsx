import { useRef, useState, useCallback, useEffect } from "react";

interface MaskPainterProps {
  sourceImage: string;
  onMaskChange: (maskDataUri: string) => void;
}

/**
 * Canvas-based mask painter for inpainting.
 *
 * The source image is rendered as a CSS background — always visible.
 * The canvas only contains the semi-transparent red overlay.
 * A hidden canvas tracks the black/white mask for export.
 */
export function MaskPainter({ sourceImage, onMaskChange }: MaskPainterProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const maskRef = useRef<HTMLCanvasElement>(null);
  const [brushSize, setBrushSize] = useState(30);
  const [erasing, setErasing] = useState(false);
  const [painting, setPainting] = useState(false);
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });

  // Load source image to get dimensions, then set canvas size
  useEffect(() => {
    const img = new Image();
    img.onload = () => {
      const container = containerRef.current;
      const maxWidth = container ? container.clientWidth : 600;
      const scale = Math.min(1, maxWidth / img.width);
      const w = Math.round(img.width * scale);
      const h = Math.round(img.height * scale);
      setDimensions({ width: w, height: h });

      // Clear overlay canvas
      const overlay = overlayRef.current;
      if (overlay) {
        overlay.width = w;
        overlay.height = h;
      }

      // Initialize mask canvas (black = keep)
      const mask = maskRef.current;
      if (mask) {
        mask.width = w;
        mask.height = h;
        const mctx = mask.getContext("2d");
        if (mctx) {
          mctx.fillStyle = "#000000";
          mctx.fillRect(0, 0, w, h);
        }
      }
    };
    img.src = sourceImage;
  }, [sourceImage]);

  const getPos = useCallback(
    (e: React.PointerEvent): { x: number; y: number } => {
      const rect = overlayRef.current?.getBoundingClientRect();
      if (!rect) return { x: 0, y: 0 };
      return { x: e.clientX - rect.left, y: e.clientY - rect.top };
    },
    [],
  );

  const drawStroke = useCallback(
    (x: number, y: number) => {
      const radius = brushSize / 2;

      // Red overlay (visible to user)
      const overlay = overlayRef.current;
      const ctx = overlay?.getContext("2d");
      if (ctx) {
        if (erasing) {
          ctx.globalCompositeOperation = "destination-out";
          ctx.beginPath();
          ctx.arc(x, y, radius, 0, Math.PI * 2);
          ctx.fill();
          ctx.globalCompositeOperation = "source-over";
        } else {
          ctx.fillStyle = "rgba(255, 50, 50, 0.45)";
          ctx.beginPath();
          ctx.arc(x, y, radius, 0, Math.PI * 2);
          ctx.fill();
        }
      }

      // Black/white mask (hidden, for export)
      const mask = maskRef.current;
      const mctx = mask?.getContext("2d");
      if (mctx) {
        mctx.fillStyle = erasing ? "#000000" : "#ffffff";
        mctx.beginPath();
        mctx.arc(x, y, radius, 0, Math.PI * 2);
        mctx.fill();
      }
    },
    [brushSize, erasing],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      setPainting(true);
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      const pos = getPos(e);
      drawStroke(pos.x, pos.y);
    },
    [getPos, drawStroke],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!painting) return;
      const pos = getPos(e);
      drawStroke(pos.x, pos.y);
    },
    [painting, getPos, drawStroke],
  );

  const handlePointerUp = useCallback(() => {
    setPainting(false);
    const mask = maskRef.current;
    if (mask) {
      onMaskChange(mask.toDataURL("image/png"));
    }
  }, [onMaskChange]);

  const handleClear = useCallback(() => {
    // Clear overlay
    const overlay = overlayRef.current;
    if (overlay) {
      const ctx = overlay.getContext("2d");
      if (ctx) ctx.clearRect(0, 0, overlay.width, overlay.height);
    }
    // Reset mask to black
    const mask = maskRef.current;
    if (mask) {
      const mctx = mask.getContext("2d");
      if (mctx) {
        mctx.fillStyle = "#000000";
        mctx.fillRect(0, 0, mask.width, mask.height);
      }
    }
    onMaskChange("");
  }, [onMaskChange]);

  return (
    <div ref={containerRef} className="space-y-2">
      {/* Canvas with source image as CSS background */}
      <div
        className="relative inline-block rounded border border-gray-700 overflow-hidden"
        style={{
          width: dimensions.width || undefined,
          height: dimensions.height || undefined,
        }}
      >
        <canvas
          ref={overlayRef}
          width={dimensions.width}
          height={dimensions.height}
          className="block cursor-crosshair"
          style={{
            touchAction: "none",
            backgroundImage: `url(${sourceImage})`,
            backgroundSize: "100% 100%",
          }}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerLeave={handlePointerUp}
        />
        {/* Hidden mask canvas */}
        <canvas ref={maskRef} className="hidden" />
      </div>

      {/* Controls */}
      <div className="flex items-center gap-4 text-[11px]">
        <div className="flex items-center gap-2">
          <label className="text-gray-500 uppercase tracking-wider text-[10px]">Brush</label>
          <input
            type="range"
            min={5}
            max={100}
            value={brushSize}
            onChange={(e) => setBrushSize(parseInt(e.target.value))}
            className="w-24"
          />
          <span className="text-gray-400 font-mono w-8">{brushSize}px</span>
        </div>

        <button
          onClick={() => setErasing((prev) => !prev)}
          className={`px-2 py-0.5 rounded border text-[10px] ${
            erasing
              ? "bg-amber-500/20 border-amber-500/50 text-amber-300"
              : "bg-[#1a1b23] border-gray-700 text-gray-400 hover:border-gray-500"
          }`}
        >
          {erasing ? "Erasing" : "Eraser"}
        </button>

        <button
          onClick={handleClear}
          className="px-2 py-0.5 rounded border border-gray-700 text-gray-400 hover:border-gray-500 text-[10px]"
        >
          Clear
        </button>
      </div>

      <div className="text-[10px] text-gray-600">
        Paint over the areas you want to replace.
      </div>
    </div>
  );
}
