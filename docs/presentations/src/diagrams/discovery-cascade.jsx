import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Discovery Cascade',
  description: 'How Rake finds stones: --at, env, cache, UDP',
  category: 'How Things Work',
  color: 'blue',
  order: 1
};


export default function DiscoveryCascade() {
  const [stage, setStage] = useState(0);
  const [scenario, setScenario] = useState('full'); // 'full', 'env', 'cache', 'udp'

  useEffect(() => {
    const maxStage = scenario === 'full' ? 1 : 
                     scenario === 'env' ? 2 : 
                     scenario === 'cache' ? 3 : 4;
    
    const timer = setInterval(() => {
      setStage(s => (s + 1) % (maxStage + 2));
    }, 1500);
    return () => clearInterval(timer);
  }, [scenario]);

  const steps = [
    { id: 'at', label: '--at flag', example: '--at stone-coral', color: '#fbbf24' },
    { id: 'env', label: 'GARDEN_STONE', example: 'env var', color: '#fb923c' },
    { id: 'cache', label: 'Tending cache', example: '90s TTL', color: '#f472b6' },
    { id: 'udp', label: 'UDP broadcast', example: 'port 7184', color: '#a78bfa' },
  ];

  const getStepState = (index) => {
    if (scenario === 'full' && index === 0) return stage >= 1 ? 'hit' : 'checking';
    if (scenario === 'env' && index <= 1) return index === 1 && stage >= 2 ? 'hit' : stage > index ? 'miss' : stage === index ? 'checking' : 'waiting';
    if (scenario === 'cache' && index <= 2) return index === 2 && stage >= 3 ? 'hit' : stage > index ? 'miss' : stage === index ? 'checking' : 'waiting';
    if (scenario === 'udp' && index <= 3) return index === 3 && stage >= 4 ? 'hit' : stage > index ? 'miss' : stage === index ? 'checking' : 'waiting';
    return 'waiting';
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">DISCOVERY CASCADE</h2>
      <p className="text-zinc-500 text-sm mb-4">how Rake finds a Stone</p>

      {/* Scenario selector */}
      <div className="flex gap-2 mb-8">
        {[
          { id: 'full', label: 'Explicit' },
          { id: 'env', label: 'Env var' },
          { id: 'cache', label: 'Cached' },
          { id: 'udp', label: 'Discovery' },
        ].map(s => (
          <button
            key={s.id}
            onClick={() => { setScenario(s.id); setStage(0); }}
            className={`px-3 py-1 rounded text-xs transition-colors ${
              scenario === s.id 
                ? 'bg-amber-400/20 text-amber-400 border border-amber-400/50' 
                : 'bg-zinc-800 text-zinc-500 border border-zinc-700 hover:border-zinc-600'
            }`}
          >
            {s.label}
          </button>
        ))}
      </div>

      {/* Command being run */}
      <div className="bg-zinc-800 rounded px-4 py-2 mb-8 font-mono text-sm">
        <span className="text-zinc-500">$ </span>
        <span className="text-zinc-300">garden-rake offer redis </span>
        {scenario === 'full' && <span className="text-amber-400">--at stone-coral</span>}
        {scenario !== 'full' && <span className="text-zinc-500">on stone-coral</span>}
      </div>

      {/* Cascade steps */}
      <div className="flex flex-col gap-4 w-full max-w-md">
        {steps.map((step, i) => {
          const state = getStepState(i);
          const isActive = state === 'checking' || state === 'hit';
          const isHit = state === 'hit';
          const isMiss = state === 'miss';

          return (
            <div
              key={step.id}
              className={`flex items-center gap-4 p-4 rounded-lg border transition-all duration-300 ${
                isHit ? 'border-green-500 bg-green-500/10' :
                isMiss ? 'border-zinc-700 bg-zinc-800/50 opacity-50' :
                isActive ? 'border-amber-400/50 bg-amber-400/5' :
                'border-zinc-800 bg-zinc-800/30'
              }`}
            >
              {/* Priority number */}
              <div className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-mono ${
                isHit ? 'bg-green-500 text-zinc-900' :
                isMiss ? 'bg-zinc-700 text-zinc-500' :
                isActive ? 'bg-amber-400 text-zinc-900' :
                'bg-zinc-800 text-zinc-500'
              }`}>
                {i + 1}
              </div>

              {/* Step info */}
              <div className="flex-1">
                <div className={`font-medium ${
                  isHit ? 'text-green-400' :
                  isMiss ? 'text-zinc-600' :
                  isActive ? 'text-amber-200' :
                  'text-zinc-500'
                }`}>
                  {step.label}
                </div>
                <div className="text-zinc-600 text-xs">{step.example}</div>
              </div>

              {/* Status */}
              <div className="text-sm">
                {isHit && <span className="text-green-400">✓ found</span>}
                {isMiss && <span className="text-zinc-600">✗ skip</span>}
                {state === 'checking' && <span className="text-amber-400 animate-pulse">checking...</span>}
              </div>
            </div>
          );
        })}
      </div>

      {/* Result */}
      <div className="mt-8 p-4 border border-zinc-800 rounded max-w-md w-full">
        <div className="text-zinc-500 text-xs mb-2">RESOLVED ENDPOINT</div>
        <div className="font-mono text-green-400">
          {stage > 0 && scenario === 'full' && 'http://stone-coral.local:7185'}
          {stage > 1 && scenario === 'env' && 'http://192.168.1.45:7185'}
          {stage > 2 && scenario === 'cache' && 'http://192.168.1.45:7185'}
          {stage > 3 && scenario === 'udp' && 'http://192.168.1.45:7185'}
          {((scenario === 'full' && stage <= 0) ||
            (scenario === 'env' && stage <= 1) ||
            (scenario === 'cache' && stage <= 2) ||
            (scenario === 'udp' && stage <= 3)) && 
            <span className="text-zinc-600">resolving...</span>}
        </div>
      </div>

      {/* Explanation */}
      <p className="text-zinc-600 text-xs mt-6 max-w-md text-center">
        First match wins. Explicit flags override environment, environment overrides cache, cache overrides network discovery.
      </p>
    </div>
  );
}
