import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Connection String',
  description: 'zen-garden:mongodb to actual endpoint',
  category: 'How Things Work',
  color: 'blue',
  order: 3
};


export default function ConnectionStringResolution() {
  const [stage, setStage] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setStage(s => (s + 1) % 6);
    }, 2000);
    return () => clearInterval(timer);
  }, []);

  // Stages:
  // 0: App has connection string
  // 1: Client library parses it
  // 2: Discovery finds the service
  // 3: Topology cache returns location
  // 4: Native string constructed
  // 5: Connected!

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">CONNECTION STRING</h2>
      <p className="text-zinc-500 text-sm mb-8">from abstract request to concrete endpoint</p>

      <div className="w-full max-w-2xl space-y-6">
        
        {/* Application code */}
        <div className={`p-4 rounded-lg border transition-all duration-500 ${
          stage >= 0 ? 'border-blue-500/50 bg-blue-500/5' : 'border-zinc-800'
        }`}>
          <div className="text-blue-400 text-xs mb-2 tracking-wide">YOUR APPLICATION</div>
          <div className="font-mono text-sm">
            <span className="text-zinc-500">const db = connect(</span>
            <span className={`transition-colors ${stage >= 0 ? 'text-amber-400' : 'text-zinc-600'}`}>
              "zen-garden:mongodb/myapp"
            </span>
            <span className="text-zinc-500">)</span>
          </div>
        </div>

        {/* Arrow */}
        <div className="flex justify-center">
          <svg width="24" height="32" viewBox="0 0 24 32">
            <path 
              d="M 12 0 L 12 24 M 6 18 L 12 24 L 18 18" 
              fill="none" 
              stroke={stage >= 1 ? '#fbbf24' : '#3f3f46'} 
              strokeWidth="2"
            />
          </svg>
        </div>

        {/* Client library parsing */}
        <div className={`p-4 rounded-lg border transition-all duration-500 ${
          stage >= 1 ? 'border-amber-500/50 bg-amber-500/5' : 'border-zinc-800 opacity-50'
        }`}>
          <div className="text-amber-400 text-xs mb-2 tracking-wide">ZEN GARDEN CLIENT</div>
          <div className="flex items-center gap-4 font-mono text-sm">
            <div className="flex-1">
              <div className="text-zinc-500 text-xs mb-1">protocol</div>
              <div className={stage >= 1 ? 'text-amber-300' : 'text-zinc-600'}>zen-garden:</div>
            </div>
            <div className="flex-1">
              <div className="text-zinc-500 text-xs mb-1">service</div>
              <div className={stage >= 1 ? 'text-green-300' : 'text-zinc-600'}>mongodb</div>
            </div>
            <div className="flex-1">
              <div className="text-zinc-500 text-xs mb-1">database</div>
              <div className={stage >= 1 ? 'text-blue-300' : 'text-zinc-600'}>myapp</div>
            </div>
          </div>
        </div>

        {/* Arrow */}
        <div className="flex justify-center">
          <svg width="24" height="32" viewBox="0 0 24 32">
            <path 
              d="M 12 0 L 12 24 M 6 18 L 12 24 L 18 18" 
              fill="none" 
              stroke={stage >= 2 ? '#a78bfa' : '#3f3f46'} 
              strokeWidth="2"
            />
          </svg>
        </div>

        {/* Discovery / Topology */}
        <div className={`p-4 rounded-lg border transition-all duration-500 ${
          stage >= 2 ? 'border-purple-500/50 bg-purple-500/5' : 'border-zinc-800 opacity-50'
        }`}>
          <div className="text-purple-400 text-xs mb-3 tracking-wide">GARDEN DISCOVERY</div>
          <div className="flex gap-4">
            {/* Mini topology */}
            <div className="flex-1 space-y-2">
              {[
                { name: 'stone-coral', service: 'mongodb', port: 27017, match: true },
                { name: 'stone-amber', service: 'redis', port: 6379, match: false },
                { name: 'stone-leaf', service: 'postgres', port: 5432, match: false },
              ].map((stone, i) => (
                <div 
                  key={stone.name}
                  className={`flex items-center gap-2 p-2 rounded text-xs transition-all ${
                    stage >= 3 && stone.match 
                      ? 'bg-green-500/20 border border-green-500/50' 
                      : 'bg-zinc-800/50'
                  }`}
                >
                  <div className={`w-2 h-2 rounded-full ${
                    stone.match && stage >= 3 ? 'bg-green-400' : 'bg-zinc-600'
                  }`} />
                  <span className="text-zinc-400 flex-1">{stone.name}</span>
                  <span className={stone.match && stage >= 2 ? 'text-green-400' : 'text-zinc-600'}>
                    {stone.service}
                  </span>
                  <span className="text-zinc-600">:{stone.port}</span>
                </div>
              ))}
            </div>
            
            {/* Query visualization */}
            <div className="flex-1 flex flex-col items-center justify-center">
              {stage === 2 && (
                <div className="text-purple-400 text-xs animate-pulse">
                  querying topology...
                </div>
              )}
              {stage >= 3 && (
                <div className="text-center">
                  <div className="text-green-400 text-xs mb-1">✓ found</div>
                  <div className="text-zinc-300 font-mono text-sm">stone-coral</div>
                  <div className="text-zinc-500 text-xs">192.168.1.45:27017</div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Arrow */}
        <div className="flex justify-center">
          <svg width="24" height="32" viewBox="0 0 24 32">
            <path 
              d="M 12 0 L 12 24 M 6 18 L 12 24 L 18 18" 
              fill="none" 
              stroke={stage >= 4 ? '#4ade80' : '#3f3f46'} 
              strokeWidth="2"
            />
          </svg>
        </div>

        {/* Resolved connection */}
        <div className={`p-4 rounded-lg border transition-all duration-500 ${
          stage >= 4 ? 'border-green-500/50 bg-green-500/5' : 'border-zinc-800 opacity-50'
        }`}>
          <div className="text-green-400 text-xs mb-2 tracking-wide">NATIVE CONNECTION STRING</div>
          <div className="font-mono text-sm">
            <span className={stage >= 4 ? 'text-green-300' : 'text-zinc-600'}>
              mongodb://192.168.1.45:27017/myapp
            </span>
          </div>
          {stage >= 5 && (
            <div className="mt-2 text-green-400 text-xs">
              ✓ Connected to MongoDB on stone-coral
            </div>
          )}
        </div>
      </div>

      {/* Key insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded max-w-md">
        <p className="text-amber-200/70 text-sm text-center">
          Your code never sees IP addresses. If MongoDB moves, discovery returns the new location.
        </p>
      </div>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-6">
        {[0,1,2,3,4,5].map(i => (
          <div 
            key={i}
            className={`w-2 h-2 rounded-full transition-colors ${
              stage === i ? 'bg-amber-400' : 'bg-zinc-700'
            }`}
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
