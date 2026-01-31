import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Cost Comparison',
  description: '5-year TCO: Zen Garden vs Cloud',
  category: 'How Things Work',
  color: 'blue',
  order: 5
};


export default function CostComparison() {
  const [year, setYear] = useState(0);
  const [isPlaying, setIsPlaying] = useState(true);
  const [scenario, setScenario] = useState('team'); // 'solo', 'team', 'business'

  const scenarios = {
    solo: {
      label: 'Solo Developer',
      stones: 1,
      hardware: 75,
      electricity: 22,
      cloudAnnual: 3600,
      cloudLabel: 't3.medium + RDS + ElastiCache',
    },
    team: {
      label: 'Small Team (3 devs)',
      stones: 3,
      hardware: 225,
      electricity: 65,
      cloudAnnual: 4500,
      cloudLabel: '3× t3.large + shared DB',
    },
    business: {
      label: 'Small Business',
      stones: 5,
      hardware: 400,
      electricity: 100,
      cloudAnnual: 8000,
      cloudLabel: 'Production setup',
    },
  };

  const current = scenarios[scenario];
  const maxYears = 5;

  useEffect(() => {
    if (!isPlaying) return;
    const timer = setInterval(() => {
      setYear(y => y >= maxYears ? 0 : y + 0.1);
    }, 100);
    return () => clearInterval(timer);
  }, [isPlaying]);

  const getZenGardenCost = (y) => {
    return current.hardware + (current.electricity * y);
  };

  const getCloudCost = (y) => {
    return current.cloudAnnual * y;
  };

  const zenCost = getZenGardenCost(year);
  const cloudCost = getCloudCost(year);
  const savings = cloudCost - zenCost;
  const savingsPercent = cloudCost > 0 ? ((savings / cloudCost) * 100).toFixed(0) : 0;

  // Find break-even point
  const breakEvenYears = current.hardware / (current.cloudAnnual - current.electricity);
  const breakEvenMonths = Math.ceil(breakEvenYears * 12);

  // Scale for graph
  const maxCost = current.cloudAnnual * maxYears;
  const graphHeight = 200;
  const graphWidth = 400;

  const zenY = graphHeight - (zenCost / maxCost) * graphHeight;
  const cloudY = graphHeight - (cloudCost / maxCost) * graphHeight;

  // Generate path points
  const getPathPoints = (getCost, steps = 50) => {
    const points = [];
    for (let i = 0; i <= steps; i++) {
      const y = (i / steps) * maxYears;
      const cost = getCost(y);
      const x = (i / steps) * graphWidth;
      const py = graphHeight - (cost / maxCost) * graphHeight;
      points.push(`${x},${py}`);
    }
    return points.join(' ');
  };

  // Current position on graph
  const currentX = (year / maxYears) * graphWidth;

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">COST COMPARISON</h2>
      <p className="text-zinc-500 text-sm mb-6">Zen Garden vs Cloud — 5 Year Total Cost</p>

      {/* Scenario selector */}
      <div className="flex gap-2 mb-6">
        {Object.entries(scenarios).map(([key, val]) => (
          <button
            key={key}
            onClick={() => { setScenario(key); setYear(0); }}
            className={`px-3 py-1 rounded text-xs transition-colors ${
              scenario === key
                ? 'bg-amber-400/20 text-amber-400 border border-amber-400/50'
                : 'bg-zinc-800 text-zinc-500 border border-zinc-700 hover:border-zinc-600'
            }`}
          >
            {val.label}
          </button>
        ))}
      </div>

      {/* Main content */}
      <div className="flex gap-12 items-start">
        
        {/* Graph */}
        <div className="flex flex-col items-center">
          <svg width={graphWidth + 60} height={graphHeight + 40} className="overflow-visible">
            {/* Grid lines */}
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

            {/* X axis labels */}
            {[0, 1, 2, 3, 4, 5].map(x => (
              <text
                key={x}
                x={40 + (x / 5) * graphWidth}
                y={graphHeight + 15}
                textAnchor="middle"
                fill="#52525b"
                fontSize="10"
              >
                Y{x}
              </text>
            ))}

            {/* Cloud line (full) */}
            <polyline
              points={getPathPoints(getCloudCost)}
              fill="none"
              stroke="#f87171"
              strokeWidth="2"
              opacity="0.3"
              transform="translate(40, 0)"
            />

            {/* Zen Garden line (full) */}
            <polyline
              points={getPathPoints(getZenGardenCost)}
              fill="none"
              stroke="#4ade80"
              strokeWidth="2"
              opacity="0.3"
              transform="translate(40, 0)"
            />

            {/* Animated cloud line */}
            <polyline
              points={getPathPoints(getCloudCost, Math.floor((year / maxYears) * 50))}
              fill="none"
              stroke="#f87171"
              strokeWidth="3"
              transform="translate(40, 0)"
            />

            {/* Animated zen garden line */}
            <polyline
              points={getPathPoints(getZenGardenCost, Math.floor((year / maxYears) * 50))}
              fill="none"
              stroke="#4ade80"
              strokeWidth="3"
              transform="translate(40, 0)"
            />

            {/* Break-even marker */}
            {breakEvenYears <= maxYears && (
              <g transform={`translate(${40 + (breakEvenYears / maxYears) * graphWidth}, 0)`}>
                <line
                  y1={0}
                  y2={graphHeight}
                  stroke="#fbbf24"
                  strokeWidth="1"
                  strokeDasharray="4,4"
                  opacity="0.5"
                />
                <text
                  y={-5}
                  textAnchor="middle"
                  fill="#fbbf24"
                  fontSize="9"
                >
                  break-even
                </text>
              </g>
            )}

            {/* Current position markers */}
            <circle
              cx={40 + currentX}
              cy={cloudY}
              r={5}
              fill="#f87171"
            />
            <circle
              cx={40 + currentX}
              cy={zenY}
              r={5}
              fill="#4ade80"
            />

            {/* Savings area */}
            {year > 0 && (
              <polygon
                points={`
                  ${40 + currentX},${zenY}
                  ${40 + currentX},${cloudY}
                  40,${graphHeight - (current.hardware / maxCost) * graphHeight}
                  40,0
                `}
                fill="#4ade80"
                opacity="0.1"
              />
            )}
          </svg>

          {/* Legend */}
          <div className="flex gap-6 mt-4">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-red-400" />
              <span className="text-zinc-400 text-xs">Cloud</span>
            </div>
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-green-400" />
              <span className="text-zinc-400 text-xs">Zen Garden</span>
            </div>
          </div>

          {/* Play/pause */}
          <button
            onClick={() => setIsPlaying(!isPlaying)}
            className="mt-4 px-3 py-1 bg-zinc-800 border border-zinc-700 rounded text-zinc-400 text-xs hover:border-zinc-600"
          >
            {isPlaying ? 'pause' : 'play'}
          </button>
        </div>

        {/* Stats panel */}
        <div className="flex flex-col gap-4 min-w-64">
          
          {/* Year indicator */}
          <div className="text-center p-3 bg-zinc-800 rounded-lg">
            <div className="text-zinc-500 text-xs mb-1">YEAR</div>
            <div className="text-amber-400 text-3xl font-light">{year.toFixed(1)}</div>
          </div>

          {/* Cost comparison */}
          <div className="space-y-3">
            <div className="p-3 rounded-lg border border-red-500/30 bg-red-500/5">
              <div className="text-red-400 text-xs mb-1">CLOUD ({current.cloudLabel})</div>
              <div className="text-red-300 text-xl font-light">
                ${cloudCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
              </div>
              <div className="text-zinc-500 text-xs">${current.cloudAnnual.toLocaleString()}/year</div>
            </div>

            <div className="p-3 rounded-lg border border-green-500/30 bg-green-500/5">
              <div className="text-green-400 text-xs mb-1">ZEN GARDEN ({current.stones} stones)</div>
              <div className="text-green-300 text-xl font-light">
                ${zenCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
              </div>
              <div className="text-zinc-500 text-xs">
                ${current.hardware} hardware + ${current.electricity}/yr electricity
              </div>
            </div>
          </div>

          {/* Savings */}
          <div className="p-4 rounded-lg border border-amber-500/30 bg-amber-500/5">
            <div className="text-amber-400 text-xs mb-1">SAVINGS</div>
            <div className="text-amber-300 text-2xl font-light">
              ${savings.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            </div>
            <div className="text-amber-400/70 text-sm">
              {savingsPercent}% less
            </div>
          </div>

          {/* Break-even */}
          <div className="text-center text-zinc-500 text-xs">
            Break-even: <span className="text-amber-400">{breakEvenMonths} months</span>
          </div>
        </div>
      </div>

      {/* 5-year summary */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <div className="text-zinc-400 text-xs mb-3 tracking-wide">5-YEAR TOTAL</div>
        <div className="flex justify-between items-center">
          <div>
            <div className="text-red-400">Cloud: ${(current.cloudAnnual * 5).toLocaleString()}</div>
            <div className="text-green-400">Zen Garden: ${(current.hardware + current.electricity * 5).toLocaleString()}</div>
          </div>
          <div className="text-right">
            <div className="text-amber-400 text-lg">
              ${((current.cloudAnnual * 5) - (current.hardware + current.electricity * 5)).toLocaleString()} saved
            </div>
            <div className="text-zinc-500 text-xs">
              {(((current.cloudAnnual * 5) - (current.hardware + current.electricity * 5)) / (current.cloudAnnual * 5) * 100).toFixed(0)}% reduction
            </div>
          </div>
        </div>
      </div>

      <button
        onClick={() => setYear(0)}
        className="mt-4 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset animation
      </button>
    </div>
  );
}
