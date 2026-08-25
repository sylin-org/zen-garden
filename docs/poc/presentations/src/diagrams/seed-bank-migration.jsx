import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Seed-Bank Migration',
  description: 'File journey: save, move storage, retrieve',
  category: 'Core Concepts',
  color: 'amber',
  order: 5
};


export default function SeedBankMigration() {
  const [stage, setStage] = useState(0);
  
  useEffect(() => {
    const durations = [2500, 2000, 2000, 2000, 2000, 2500, 3000];
    const timer = setTimeout(() => {
      setStage(s => (s + 1) % 7);
    }, durations[stage]);
    return () => clearTimeout(timer);
  }, [stage]);

  // Stage descriptions:
  // 0: Initial state - app on leaf, seed-bank on coral
  // 1: App saves report.pdf → file travels to seed-bank
  // 2: File stored in seed-bank (confirmed)
  // 3: Seed-bank unplugged, moving...
  // 4: Seed-bank plugged into amber
  // 5: App requests file → file travels back
  // 6: Success - app has the file

  const stonePositions = {
    coral: { x: 120, y: 100 },
    brook: { x: 300, y: 100 },
    amber: { x: 480, y: 100 },
    mist: { x: 180, y: 220 },
    leaf: { x: 420, y: 220 },
  };

  const seedBankOnCoral = stage <= 2;
  const seedBankMoving = stage === 3;
  const seedBankOnAmber = stage >= 4;
  const fileInSeedBank = stage >= 2 && stage <= 5;

  const Stone = ({ name, x, y, hasStorage, hasApp }) => (
    <g>
      {/* Stone body */}
      <rect
        x={x - 35}
        y={y - 22}
        width={70}
        height={44}
        rx={4}
        fill="#3f3f46"
        stroke={hasStorage ? "#fbbf24" : hasApp ? "#60a5fa" : "#52525b"}
        strokeWidth={hasStorage || hasApp ? 2 : 1}
      />
      {/* LED */}
      <circle
        cx={x + 24}
        cy={y - 12}
        r={4}
        fill={hasStorage ? "#4ade80" : hasApp ? "#60a5fa" : "#6b7280"}
      />
      {/* Name */}
      <text x={x} y={y + 5} textAnchor="middle" fill="#a1a1aa" fontSize="11">
        {name}
      </text>
      
      {/* App indicator */}
      {hasApp && (
        <g>
          <rect
            x={x - 28}
            y={y + 28}
            width={56}
            height={18}
            rx={3}
            fill="#1e3a5f"
            stroke="#3b82f6"
            strokeWidth="1"
          />
          <text x={x} y={y + 40} textAnchor="middle" fill="#93c5fd" fontSize="9">
            my-app
          </text>
        </g>
      )}
      
      {/* Storage indicator */}
      {hasStorage && (
        <g>
          <rect
            x={x - 28}
            y={y + 28}
            width={56}
            height={18}
            rx={3}
            fill="#1c1917"
            stroke="#78716c"
          />
          <text x={x} y={y + 40} textAnchor="middle" fill="#a8a29e" fontSize="9">
            seed-bank
          </text>
          {/* File inside seed-bank */}
          {fileInSeedBank && (
            <g>
              <rect
                x={x + 12}
                y={y + 30}
                width={12}
                height={14}
                rx={1}
                fill="#fbbf24"
                opacity="0.8"
              />
              <text x={x + 18} y={y + 39} textAnchor="middle" fill="#1c1917" fontSize="6">
                📄
              </text>
            </g>
          )}
        </g>
      )}
    </g>
  );

  // File animation path
  const getFilePath = () => {
    if (stage === 1) {
      // File traveling from leaf to coral
      return {
        from: stonePositions.leaf,
        to: stonePositions.coral,
        progress: 'animate',
        label: 'report.pdf'
      };
    }
    if (stage === 5) {
      // File traveling from amber back to leaf
      return {
        from: stonePositions.amber,
        to: stonePositions.leaf,
        progress: 'animate',
        label: 'report.pdf'
      };
    }
    return null;
  };

  const filePath = getFilePath();

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">SEED-BANK MIGRATION</h2>
      <p className="text-zinc-500 text-sm mb-6">storage is singular, access is distributed</p>
      
      <svg viewBox="0 0 600 320" className="w-full max-w-2xl">
        {/* Connection lines between stones */}
        <g stroke="#3f3f46" strokeWidth="1" opacity="0.5">
          <line x1="120" y1="100" x2="300" y2="100" />
          <line x1="300" y1="100" x2="480" y2="100" />
          <line x1="120" y1="100" x2="180" y2="220" />
          <line x1="480" y1="100" x2="420" y2="220" />
          <line x1="180" y1="220" x2="420" y2="220" />
          <line x1="300" y1="100" x2="180" y2="220" />
          <line x1="300" y1="100" x2="420" y2="220" />
        </g>

        {/* Stones */}
        <Stone name="coral" {...stonePositions.coral} hasStorage={seedBankOnCoral} />
        <Stone name="brook" {...stonePositions.brook} />
        <Stone name="amber" {...stonePositions.amber} hasStorage={seedBankOnAmber && !seedBankMoving} />
        <Stone name="mist" {...stonePositions.mist} />
        <Stone name="leaf" {...stonePositions.leaf} hasApp={true} />

        {/* Moving seed-bank */}
        {seedBankMoving && (
          <g>
            <rect
              x={0}
              y={55}
              width={56}
              height={18}
              rx={3}
              fill="#1c1917"
              stroke="#fbbf24"
              strokeWidth="2"
            >
              <animate
                attributeName="x"
                from={stonePositions.coral.x - 28}
                to={stonePositions.amber.x - 28}
                dur="2s"
                fill="freeze"
              />
            </rect>
            <text x={0} y={67} textAnchor="middle" fill="#fbbf24" fontSize="9">
              seed-bank
              <animate
                attributeName="x"
                from={stonePositions.coral.x}
                to={stonePositions.amber.x}
                dur="2s"
                fill="freeze"
              />
            </text>
            {/* File traveling with seed-bank */}
            <rect
              x={0}
              y={57}
              width={12}
              height={14}
              rx={1}
              fill="#fbbf24"
              opacity="0.8"
            >
              <animate
                attributeName="x"
                from={stonePositions.coral.x + 12}
                to={stonePositions.amber.x + 12}
                dur="2s"
                fill="freeze"
              />
            </rect>
          </g>
        )}

        {/* File animation */}
        {filePath && (
          <g>
            {/* Path line */}
            <line
              x1={filePath.from.x}
              y1={filePath.from.y}
              x2={filePath.to.x}
              y2={filePath.to.y}
              stroke="#fbbf24"
              strokeWidth="2"
              strokeDasharray="6,4"
              opacity="0.4"
            />
            {/* Animated file */}
            <g>
              <rect
                x={filePath.from.x - 15}
                y={filePath.from.y - 12}
                width={30}
                height={24}
                rx={3}
                fill="#fbbf24"
              >
                <animate
                  attributeName="x"
                  from={filePath.from.x - 15}
                  to={filePath.to.x - 15}
                  dur="1.5s"
                  fill="freeze"
                />
                <animate
                  attributeName="y"
                  from={filePath.from.y - 12}
                  to={filePath.to.y - 12}
                  dur="1.5s"
                  fill="freeze"
                />
              </rect>
              <text
                x={filePath.from.x}
                y={filePath.from.y + 3}
                textAnchor="middle"
                fill="#1c1917"
                fontSize="8"
                fontWeight="bold"
              >
                📄
                <animate
                  attributeName="x"
                  from={filePath.from.x}
                  to={filePath.to.x}
                  dur="1.5s"
                  fill="freeze"
                />
                <animate
                  attributeName="y"
                  from={filePath.from.y + 3}
                  to={filePath.to.y + 3}
                  dur="1.5s"
                  fill="freeze"
                />
              </text>
            </g>
            {/* Label */}
            <text
              x={(filePath.from.x + filePath.to.x) / 2}
              y={(filePath.from.y + filePath.to.y) / 2 - 20}
              textAnchor="middle"
              fill="#fbbf24"
              fontSize="10"
            >
              {filePath.label}
            </text>
          </g>
        )}

        {/* Stage description */}
        <text x="300" y="290" textAnchor="middle" fill="#71717a" fontSize="12">
          {stage === 0 && "my-app runs on leaf, seed-bank connected to coral"}
          {stage === 1 && "app saves report.pdf → routed to seed-bank"}
          {stage === 2 && "file stored ✓"}
          {stage === 3 && "physically move the seed-bank..."}
          {stage === 4 && "seed-bank now on amber (file still inside)"}
          {stage === 5 && "app requests report.pdf → routed to new location"}
          {stage === 6 && "file retrieved ✓ — app didn't notice the move"}
        </text>

        {/* Code snippet */}
        <g transform="translate(300, 305)">
          <text textAnchor="middle" fill="#52525b" fontSize="10" fontFamily="monospace">
            {stage === 1 && 'await storage.save("report.pdf", data)'}
            {stage === 5 && 'await storage.load("report.pdf")'}
            {stage === 6 && '// same API, different stone, same result'}
          </text>
        </g>
      </svg>

      {/* Key insight */}
      <div className="mt-4 p-4 border border-zinc-800 rounded max-w-md">
        <p className="text-amber-200/70 text-sm text-center">
          {stage < 6 
            ? "The application requests storage. The garden finds it."
            : "The application didn't notice. The garden did."}
        </p>
      </div>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-4">
        {[0,1,2,3,4,5,6].map(i => (
          <div 
            key={i}
            className={`w-2 h-2 rounded-full transition-colors ${stage === i ? 'bg-amber-400' : 'bg-zinc-700'}`}
          />
        ))}
      </div>

      <button 
        onClick={() => setStage(0)}
        className="mt-4 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset animation
      </button>
    </div>
  );
}
