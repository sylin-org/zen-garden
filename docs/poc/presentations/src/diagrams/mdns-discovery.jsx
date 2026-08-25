import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'mDNS Discovery',
  description: 'Stones finding each other via broadcast',
  category: 'Core Concepts',
  color: 'amber',
  order: 1
};


export default function MDNSDiscovery() {
  const [stage, setStage] = useState(0);
  
  useEffect(() => {
    const timer = setInterval(() => {
      setStage(s => (s + 1) % 6);
    }, 1500);
    return () => clearInterval(timer);
  }, []);

  const stones = [
    { id: 'coral', name: 'stone-coral-prairie', x: 200, y: 150 },
    { id: 'amber', name: 'stone-amber-falls', x: 500, y: 100 },
    { id: 'brook', name: 'stone-quiet-brook', x: 450, y: 280 },
    { id: 'mist', name: 'stone-morning-mist', x: 150, y: 320 },
    { id: 'leaf', name: 'stone-silver-leaf', x: 350, y: 200 },
  ];

  const getStoneOpacity = (index) => {
    if (stage === 0) return index === 0 ? 1 : 0;
    if (stage === 1) return index <= 1 ? 1 : 0;
    if (stage === 2) return index <= 2 ? 1 : 0;
    if (stage === 3) return index <= 3 ? 1 : 0;
    return 1;
  };

  const getLineOpacity = (fromIndex, toIndex) => {
    const minRequired = Math.max(fromIndex, toIndex);
    if (stage <= minRequired) return 0;
    return 0.4;
  };

  const connections = [
    [0, 1], [0, 3], [0, 4],
    [1, 2], [1, 4],
    [2, 3], [2, 4],
    [3, 4]
  ];

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">DISCOVERY</h2>
      <p className="text-zinc-500 text-sm mb-8">mDNS multicast — stones find each other automatically</p>
      
      <svg viewBox="0 0 700 420" className="w-full max-w-2xl">
        {/* Connection lines */}
        {connections.map(([from, to], i) => (
          <line
            key={i}
            x1={stones[from].x}
            y1={stones[from].y}
            x2={stones[to].x}
            y2={stones[to].y}
            stroke="#a78bfa"
            strokeWidth="1"
            opacity={getLineOpacity(from, to)}
            style={{ transition: 'opacity 0.5s ease' }}
          />
        ))}

        {/* Broadcast rings */}
        {stones.map((stone, i) => (
          <circle
            key={`ring-${i}`}
            cx={stone.x}
            cy={stone.y}
            r={stage > i && stage <= i + 1 ? 60 : 0}
            fill="none"
            stroke="#fbbf24"
            strokeWidth="2"
            opacity={stage > i && stage <= i + 1 ? 0.5 : 0}
            style={{ transition: 'all 0.5s ease' }}
          />
        ))}

        {/* Stones */}
        {stones.map((stone, i) => (
          <g 
            key={stone.id} 
            opacity={getStoneOpacity(i)}
            style={{ transition: 'opacity 0.5s ease' }}
          >
            {/* Stone body */}
            <rect
              x={stone.x - 30}
              y={stone.y - 20}
              width={60}
              height={40}
              rx={4}
              fill="#3f3f46"
              stroke="#71717a"
              strokeWidth="1"
            />
            {/* LED indicator */}
            <circle
              cx={stone.x + 20}
              cy={stone.y - 10}
              r={4}
              fill={stage > i ? '#4ade80' : '#27272a'}
              style={{ transition: 'fill 0.3s ease' }}
            />
            {/* Name label */}
            <text
              x={stone.x}
              y={stone.y + 50}
              textAnchor="middle"
              className="text-xs"
              fill="#a1a1aa"
            >
              {stone.name.replace('stone-', '')}
            </text>
          </g>
        ))}

        {/* Stage indicator */}
        <text x="350" y="400" textAnchor="middle" fill="#71717a" className="text-sm">
          {stage === 0 && "first stone powers on..."}
          {stage === 1 && "broadcasts presence..."}
          {stage === 2 && "others respond..."}
          {stage === 3 && "mesh forms..."}
          {stage >= 4 && "garden discovered"}
        </text>
      </svg>

      <div className="flex gap-2 mt-8">
        {[0,1,2,3,4,5].map(i => (
          <div 
            key={i}
            className={`w-2 h-2 rounded-full transition-colors ${stage === i ? 'bg-amber-400' : 'bg-zinc-700'}`}
          />
        ))}
      </div>
    </div>
  );
}
