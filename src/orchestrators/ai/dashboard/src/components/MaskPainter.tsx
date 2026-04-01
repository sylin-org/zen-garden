import { useRef, useState, useCallback, useEffect } from "react";

interface MaskPainterProps {
  /** Base64 data URI of the source image to paint over. */
  sourceImage: string;
  /** Called when the mask changes. Returns a base64 data URI of the mask (white on black). */
  onMaskChange: (maskDataUri: string) => void;
}

/**
 * Canvas-based mask painter for inpainting.
 *
 * Renders the source image with a semi-transparent overlay.
 * User paints with a circular brush (white = inpaint region).
 * Exports a black/white mask image on each stroke.
 */
export function MaskPainter({ sourceImage, onMaskChange }: MaskPainterProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const maskRef = useRef<HTMLCanvasElement>(null);
  const [brushSize, setBrushSize] = useState(30);
  const [erasing, setErasing] = useState(false);
  const [painting, setPainting] = useState(false);
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });

  // Load the source image and set canvas dimensions
  useEffect(() => {
    const img = new Image();
    img.onload = () => {
      // Fit to container width, maintain aspect ratio
      const container = containerRef.current;
      const maxWidth = container ? container.clientWidth : 600;
      const scale = Math.min(1, maxWidth / img.width);
      const w = Math.round(img.width * scale);
      const h = Math.round(img.height * scale);
      setDimensions({ width: w, height: h });

      // Draw source image on the display canvas
      const canvas = canvasRef.current;
      if (canvas) {
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.drawImage(img, 0, 0, w, h);
        }
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
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return { x: 0, y: 0 };
      return {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };
    },
    [],
  );

  const drawStroke = useCallback(
    (x: number, y: number) => {
      // Draw on the visible overlay
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext("2d");
      if (ctx) {
        ctx.globalCompositeOperation = erasing ? "destination-out" : "source-over";
        ctx.fillStyle = "rgba(255, 60, 60, 0.4)";
        ctx.beginPath();
        ctx.arc(x, y, brushSize / 2, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalCompositeOperation = "source-over";
      }

      // Draw on the mask (white = inpaint, black = keep)
      const mask = maskRef.current;
      const mctx = mask?.getContext("2d");
      if (mctx) {
        mctx.fillStyle = erasing ? "#000000" : "#ffffff";
        mctx.beginPath();
        mctx.arc(x, y, brushSize / 2, 0, Math.PI * 2);
        mctx.fill();
      }
    },
    [brushSize, erasing],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      setPainting(true);
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      const { x, y } = getPos(e);
      drawStroke(x, y);
    },
    [getPos, drawStroke],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!painting) return;
      const { x, y } = getPos(e);
      drawStroke(x, y);
    },
    [painting, getPos, drawStroke],
  );

  const handlePointerUp = useCallback(() => {
    setPainting(false);
    // Export mask
    const mask = maskRef.current;
    if (mask) {
      onMaskChange(mask.toDataURL("image/png"));
    }
  }, [onMaskChange]);

  const redrawOverlay = useCallback(() => {
    const canvas = canvasRef.current;
    const mask = maskRef.current;
    if (!canvas || !mask) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Redraw source image
    const img = new Image();
    img.onload = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

      // Overlay the mask as semi-transparent red
      const mctx = mask.getContext("2d");
      if (!mctx) return;
      const maskData = mctx.getImageData(0, 0, mask.width, mask.height);
      const overlay = ctx.createImageData(canvas.width, canvas.height);

      for (let i = 0; i < maskData.data.length; i += 4) {
        const isMasked = maskData.data[i] > 128; // R channel
        overlay.data[i] = isMasked ? 255 : 0;     // R
        overlay.data[i + 1] = isMasked ? 60 : 0;  // G
        overlay.data[i + 2] = isMasked ? 60 : 0;  // B
        overlay.data[i + 3] = isMasked ? 100 : 0;  // A
      }

      ctx.putImageData(overlay, 0, 0);

      // Redraw the source image underneath
      ctx.globalCompositeOperation = "destination-over";
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
      ctx.globalCompositeOperation = "source-over";
    };
    img.src = sourceImage;
  }, [sourceImage]);

  const handleClear = useCallback(() => {
    const mask = maskRef.current;
    if (mask) {
      const mctx = mask.getContext("2d");
      if (mctx) {
        mctx.fillStyle = "#000000";
        mctx.fillRect(0, 0, mask.width, mask.height);
      }
    }
    redrawOverlay();
    onMaskChange("");
  }, [redrawOverlay, onMaskChange]);

  return (
    <div ref={containerRef} className="space-y-2">
      <div className="relative inline-block rounded border border-gray-700 overflow-hidden">
        <canvas
          ref={canvasRef}
          width={dimensions.width}
          height={dimensions.height}
          className="block cursor-crosshair"
          style={{ touchAction: "none" }}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerLeave={handlePointerUp}
        />
        {/* Hidden mask canvas — not displayed, used for export */}
        <canvas ref={maskRef} className="hidden" />
      </div>

      {/* Controls */}
      <div className="flex items-center gap-4 text-[11px]">
        <div className="flex items-center gap-2">
          <label className="text-gray-500 uppercase tracking-wider">Brush</label>
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
          onClick={() => setErasing((e) => !e)}
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
        Paint over the areas you want to replace. White = replace, black = keep.
      </div>
    </div>
  );
}
