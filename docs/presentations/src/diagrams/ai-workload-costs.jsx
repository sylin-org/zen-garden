import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'AI Workload Costs',
  description: 'GPU Stone vs Cloud AI — dramatic savings',
  category: 'How Things Work',
  color: 'purple',
  order: 6
};

export default function AIWorkloadCosts() {
  const [year, setYear] = useState(0);
  const [isPlaying, setIsPlaying] = useState(true);
  const [tier, setTier] = useState('capable');

  const tiers = {
    entry: {
      label: 'Entry AI',
      gpu: 'GTX 1070/1080',
      vram: '8GB',
      hardware: 300,
      electricity: 130,
      models: '7B models',
      tokensPerSec: '20-30',
      cloudEquiv: 'g4dn.xlarge',
      cloudAnnual: 4608,
      icon: '🌱',
    },
    capable: {
      label: 'Capable AI',
      gpu: 'RTX 3060 Ti',
      vram: '8GB',
      hardware: 660,
      electricity: 105,
      models: '7B-13B models',
      tokensPerSec: '30-50',
      cloudEquiv: 'g4dn.xlarge',
      cloudAnnual: 4608,
      icon: '🪴',
    },
    serious: {
      label: 'Serious AI',
      gpu: 'RTX 3090',
      vram: '24GB',
      hardware: 1200,
      electricity: 180,
      models: '30B+ models',
      tokensPerSec: '40-60',
      cloudEquiv: 'g5.xlarge',
      cloudAnnual: 8813,
      icon: '🌳',
    },
    maximum: {
      label: 'Maximum AI',
      gpu: 'RTX 4090',
      vram: '24GB',
      hardware: 2500,
      electricity: 250,
      models: '70B models',
      tokensPerSec: '60-80',
      cloudEquiv: 'p3.2xlarge',
      cloudAnnual: 12000,
      icon: '🌲',
    },
  };

  const current = tiers[tier];
  const maxYears = 5;

  useEffect(() => {
    if (!isPlaying) return;
    const timer = setInterval(() => {
      setYear(y => y >= maxYears ? 0 : y + 0.05);
    }, 50);
    return () => clearInterval(timer);
  }, [isPlaying]);

  const getLocalCost = (y) => current.hardware + (current.electricity * y);
  const getCloudCost = (y) => current.cloudAnnual * y;

  const localCost = getLocalCost(year);
  const cloudCost = getCloudCost(year);
  const savings = cloudCost - localCost;
  const savingsPercent = cloudCost > 0 ? ((savings / cloudCost) * 100).toFixed(0) : 0;

  const breakEvenYears = current.hardware / (current.cloudAnnual - current.electricity);
  const breakEvenWeeks = Math.ceil(breakEvenYears * 52);

  const maxCost = current.cloudAnnual * maxYears;
  const graphHeight = 280;
  const graphWidth = 400;

  const getPathPoints = (getCost, progress = 1) => {
    const points = [];
    const steps = Math.floor(50 * progress);
    for (let i = 0; i <= steps; i++) {
      const y = (i / 50) * maxYears;
      const cost = getCost(y);
      const x = (i / 50) * graphWidth;
      const py = graphHeight - (cost / maxCost) * graphHeight;
      points.push(`${x},${py}`);
    }
    return points.join(' ');
  };

  const progress = year / maxYears;

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">AI WORKLOAD COSTS</h2>
      <p className="text-zinc-500 text-sm mb-6">Local GPU Stone vs Cloud — 5 Year Comparison</p>

      {/* Tier selector */}
      <div className="flex gap-2 mb-6">
        {Object.entries(tiers).map(([key, val]) => (
          <button
            key={key}
            onClick={() => { setTier(key); setYear(0); }}
            className={`px-3 py-2 rounded-lg text-xs transition-all ${
              tier === key
                ? 'bg-purple-500/20 text-purple-300 border border-purple-500/50'
                : 'bg-zinc-800 text-zinc-500 border border-zinc-700 hover:border-zinc-600'
            }`}
          >
            <div className="text-lg mb-1">{val.icon}</div>
            <div>{val.label}</div>
          </button>
        ))}
      </div>

      <div className="flex gap-10 items-center">
        
        {/* Left: GPU Info */}
        <div className="w-48">
          <div className="p-4 rounded-xl border border-purple-500/30 bg-purple-500/5 mb-4">
            <div className="text-purple-400 text-xs mb-2">GPU STONE</div>
            <div className="text-purple-300 text-lg font-medium">{current.gpu}</div>
            <div className="text-zinc-500 text-xs mt-1">{current.vram} VRAM</div>
          </div>
          
          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-zinc-500">Models:</span>
              <span className="text-zinc-300">{current.models}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-500">Speed:</span>
              <span className="text-zinc-300">{current.tokensPerSec} tok/s</span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-500">Hardware:</span>
              <span className="text-green-400">${current.hardware}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-zinc-500">Power/yr:</span>
              <span className="text-green-400">${current.electricity}</span>
            </div>
          </div>

          <div className="mt-4 p-3 rounded-lg border border-zinc-700 bg-zinc-800/50">
            <div className="text-zinc-500 text-xs mb-1">CLOUD EQUIVALENT</div>
            <div className="text-red-400 text-sm">{current.cloudEquiv}</div>
            <div className="text-zinc-500 text-xs">${current.cloudAnnual.toLocaleString()}/year</div>
          </div>
        </div>

        {/* Center: Graph */}
        <div className="flex flex-col items-center">
          <svg width={graphWidth + 60} height={graphHeight + 40} className="overflow-visible">
            {/* Grid */}
            {[0, 1, 2, 3, 4, 5].map(y => (
              <g key={y}>
                <line
                  x1={40}
                  y1={graphHeight - (y / 5) * graphHeight}
                  x2={graphWidth + 40}
                  y2={graphHeight - (y / 5) * graphHeight}
                  stroke="#27272a"
                  strokeWidth="1"
                />
                <text
                  x={35}
                  y={graphHeight - (y / 5) * graphHeight + 4}
                  textAnchor="end"
                  fill="#52525b"
                  fontSize="10"
                >
                  ${((y / 5) * maxCost / 1000).toFixed(0)}k
                </text>
              </g>
            ))}

            {/* X axis */}
            {[0, 1, 2, 3, 4, 5].map(x => (
              <text
                key={x}
                x={40 + (x / 5) * graphWidth}
                y={graphHeight + 20}
                textAnchor="middle"
                fill="#52525b"
                fontSize="10"
              >
                Y{x}
              </text>
            ))}

            {/* Ghost lines */}
            <polyline
              points={getPathPoints(getCloudCost)}
              fill="none"
              stroke="#f87171"
              strokeWidth="2"
              opacity="0.2"
              transform="translate(40, 0)"
            />
            <polyline
              points={getPathPoints(getLocalCost)}
              fill="none"
              stroke="#a78bfa"
              strokeWidth="2"
              opacity="0.2"
              transform="translate(40, 0)"
            />

            {/* Animated lines */}
            <polyline
              points={getPathPoints(getCloudCost, progress)}
              fill="none"
              stroke="#f87171"
              strokeWidth="3"
              transform="translate(40, 0)"
            />
            <polyline
              points={getPathPoints(getLocalCost, progress)}
              fill="none"
              stroke="#a78bfa"
              strokeWidth="3"
              transform="translate(40, 0)"
            />

            {/* Break-even line */}
            {breakEvenYears <= maxYears && (
              <g transform={`translate(${40 + (breakEvenYears / maxYears) * graphWidth}, 0)`}>
                <line
                  y1={0}
                  y2={graphHeight}
                  stroke="#22c55e"
                  strokeWidth="2"
                  strokeDasharray="4,4"
                  opacity="0.6"
                />
                <text
                  y={-5}
                  textAnchor="middle"
                  fill="#22c55e"
                  fontSize="10"
                >
                  {breakEvenWeeks}w
                </text>
              </g>
            )}

            {/* Current dots */}
            <circle
              cx={40 + progress * graphWidth}
              cy={graphHeight - (cloudCost / maxCost) * graphHeight}
              r={6}
              fill="#f87171"
            />
            <circle
              cx={40 + progress * graphWidth}
              cy={graphHeight - (localCost / maxCost) * graphHeight}
              r={6}
              fill="#a78bfa"
            />

            {/* Savings area fill */}
            {year > breakEvenYears && (
              <polygon
                points={`
                  ${40 + progress * graphWidth},${graphHeight - (localCost / maxCost) * graphHeight}
                  ${40 + progress * graphWidth},${graphHeight - (cloudCost / maxCost) * graphHeight}
                  ${40 + (breakEvenYears / maxYears) * graphWidth},${graphHeight - (getLocalCost(breakEvenYears) / maxCost) * graphHeight}
                `}
                fill="#22c55e"
                opacity="0.15"
              />
            )}
          </svg>

          {/* Legend */}
          <div className="flex gap-8 mt-4">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-red-400" />
              <span className="text-zinc-400 text-sm">Cloud GPU</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-purple-400" />
              <span className="text-zinc-400 text-sm">GPU Stone</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-4 h-1 bg-green-500" />
              <span className="text-zinc-400 text-sm">Break-even</span>
            </div>
          </div>

          <button
            onClick={() => setIsPlaying(!isPlaying)}
            className="mt-4 px-4 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-zinc-400 text-sm hover:border-zinc-600"
          >
            {isPlaying ? 'pause' : 'play'}
          </button>
        </div>

        {/* Right: Stats */}
        <div className="w-56 space-y-3">
          
          <div className="text-center p-3 bg-zinc-800 rounded-lg">
            <div className="text-zinc-500 text-xs mb-1">YEAR</div>
            <div className="text-purple-400 text-3xl font-light">{year.toFixed(1)}</div>
          </div>

          <div className="p-3 rounded-lg border border-red-500/30 bg-red-500/5">
            <div className="text-red-400 text-xs mb-1">CLOUD GPU</div>
            <div className="text-red-300 text-xl font-light">
              ${cloudCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            </div>
          </div>

          <div className="p-3 rounded-lg border border-purple-500/30 bg-purple-500/5">
            <div className="text-purple-400 text-xs mb-1">GPU STONE</div>
            <div className="text-purple-300 text-xl font-light">
              ${localCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            </div>
          </div>

          <div className="p-4 rounded-lg border border-green-500/30 bg-green-500/5">
            <div className="text-green-400 text-xs mb-1">SAVINGS</div>
            <div className="text-green-300 text-2xl font-light">
              ${Math.max(0, savings).toLocaleString('en-US', { maximumFractionDigits: 0 })}
            </div>
            <div className={`text-sm transition-opacity ${savings > 0 ? 'text-green-400/70' : 'opacity-0'}`}>
              {savingsPercent}% less
            </div>
          </div>

          <div className="text-center text-zinc-500 text-xs">
            Break-even: <span className="text-green-400">{breakEvenWeeks} weeks</span>
          </div>
        </div>
      </div>

      {/* 5-year summary */}
      <div className="mt-6 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <div className="text-zinc-400 text-xs mb-3 tracking-wide">5-YEAR TOTAL</div>
        <div className="flex justify-between items-center">
          <div>
            <div className="text-red-400">Cloud: ${(current.cloudAnnual * 5).toLocaleString()}</div>
            <div className="text-purple-400">GPU Stone: ${(current.hardware + current.electricity * 5).toLocaleString()}</div>
          </div>
          <div className="text-right">
            <div className="text-green-400 text-lg">
              ${((current.cloudAnnual * 5) - (current.hardware + current.electricity * 5)).toLocaleString()} saved
            </div>
            <div className="text-zinc-500 text-xs">
              {(((current.cloudAnnual * 5) - (current.hardware + current.electricity * 5)) / (current.cloudAnnual * 5) * 100).toFixed(0)}% reduction
            </div>
          </div>
        </div>
      </div>

      {/* Key insight */}
      <div className="mt-4 p-3 border border-zinc-800 rounded-lg max-w-lg">
        <p className="text-amber-200/70 text-sm text-center">
          Your app wishes for <code className="text-purple-400">ollama[llama3]</code>. 
          The garden fulfills it. Cloud bill: $0.
        </p>
      </div>

      <button
        onClick={() => setYear(0)}
        className="mt-3 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset animation
      </button>
    </div>
  );
}
