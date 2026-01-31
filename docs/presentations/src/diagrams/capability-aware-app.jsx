import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Capability-Aware App',
  description: 'Features light up as hardware arrives',
  category: 'How Things Work',
  color: 'blue',
  order: 6
};


export default function CapabilityAwareApp() {
  const [stage, setStage] = useState(0);

  useEffect(() => {
    const durations = [3000, 2500, 3000, 2500, 2000, 2000, 3500];
    const timer = setTimeout(() => {
      setStage(s => (s + 1) % 7);
    }, durations[stage]);
    return () => clearTimeout(timer);
  }, [stage]);

  // Stages:
  // 0: App starts, declares requirements
  // 1: Garden responds - mongodb found, ollama not available (gpu-stone offline)
  // 2: App runs with yearning, waiting...
  // 3: User plugs in GPU stone - it comes online
  // 4: Capability appears in ledger, event flows to driver
  // 5: Driver pushes event to app
  // 6: Semantic search lights up!

  const gpuOnline = stage >= 3;
  const ollamaInLedger = stage >= 4;
  const driverNotified = stage >= 4;
  const appNotified = stage >= 5;
  const featureEnabled = stage >= 6;
  const yearning = stage >= 1 && stage < 4;

  const Stone = ({ name, capabilities, online = true, highlight = false }) => (
    <div 
      className={`
        flex items-center gap-3 px-3 py-2 rounded-lg border transition-all duration-500
        ${!online ? 'opacity-40 border-dashed border-zinc-700 bg-zinc-900' : 
          highlight ? 'border-green-500 bg-green-500/10' : 
          'border-zinc-700 bg-zinc-800'}
      `}
    >
      <div className={`w-2 h-2 rounded-full flex-shrink-0 ${
        !online ? 'bg-zinc-600' : 
        highlight ? 'bg-green-400' : 
        'bg-blue-400'
      }`} />
      <div className="flex-1 min-w-0">
        <div className={`text-sm truncate ${!online ? 'text-zinc-600' : 'text-zinc-300'}`}>
          {name}
        </div>
        {capabilities.length > 0 && (
          <div className={`text-xs truncate ${!online ? 'text-zinc-700' : highlight ? 'text-green-400' : 'text-zinc-500'}`}>
            {capabilities.join(', ')}
          </div>
        )}
      </div>
      {!online && (
        <div className="text-xs text-zinc-600 flex-shrink-0">offline</div>
      )}
    </div>
  );

  const LedgerEntry = ({ name, status, subscribed = false }) => {
    // status: 'available', 'yearning', 'offline'
    const baseStyles = "w-full px-3 py-2 rounded border text-sm flex items-center justify-between transition-all duration-500";
    
    const styles = {
      available: subscribed 
        ? 'border-green-500/50 bg-green-500/10 text-green-400' 
        : 'border-zinc-700 bg-zinc-800/50 text-zinc-500',
      yearning: 'border-pink-500/50 bg-pink-500/10 text-pink-400 border-dashed animate-pulse',
      offline: 'border-zinc-800 bg-zinc-900 text-zinc-600',
    };
    
    return (
      <div className={`${baseStyles} ${styles[status]}`}>
        <span>{name}</span>
        <span className="text-xs">
          {status === 'available' && subscribed && '✓ subscribed'}
          {status === 'available' && !subscribed && 'available'}
          {status === 'yearning' && 'yearning...'}
          {status === 'offline' && 'not in use'}
        </span>
      </div>
    );
  };

  // Arrow component
  const Arrow = ({ direction = 'right', active = false, label = '' }) => (
    <div className={`flex items-center gap-1 ${active ? 'text-green-400' : 'text-zinc-700'}`}>
      {direction === 'left' && (
        <svg width="32" height="16" viewBox="0 0 32 16">
          <path 
            d="M28 8 L4 8 M10 2 L4 8 L10 14" 
            fill="none" 
            stroke="currentColor" 
            strokeWidth="2"
            strokeDasharray={active ? "none" : "4,2"}
          />
          {active && (
            <circle r="3" fill="currentColor">
              <animateMotion dur="0.5s" repeatCount="indefinite" path="M28 8 L4 8" />
            </circle>
          )}
        </svg>
      )}
      {direction === 'right' && (
        <svg width="32" height="16" viewBox="0 0 32 16">
          <path 
            d="M4 8 L28 8 M22 2 L28 8 L22 14" 
            fill="none" 
            stroke="currentColor" 
            strokeWidth="2"
            strokeDasharray={active ? "none" : "4,2"}
          />
          {active && (
            <circle r="3" fill="currentColor">
              <animateMotion dur="0.5s" repeatCount="indefinite" path="M4 8 L28 8" />
            </circle>
          )}
        </svg>
      )}
      {label && <span className="text-xs">{label}</span>}
    </div>
  );

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">CAPABILITY-AWARE APPLICATION</h2>
      <p className="text-zinc-500 text-sm mb-8">features light up as the garden grows</p>

      <div className="flex items-center gap-4 max-w-5xl w-full">
        
        {/* LEFT: Application (with driver inside) */}
        <div className="w-56 flex-shrink-0">
          <div className="border-2 border-blue-500/50 rounded-xl p-4 bg-zinc-900">
            <div className="text-blue-400 text-sm font-medium mb-3">my-app</div>
            
            {/* App UI */}
            <div className="space-y-2 mb-3">
              <button className="w-full py-1.5 px-3 bg-blue-600 text-white rounded text-xs">
                Search
              </button>
              <button className="w-full py-1.5 px-3 bg-blue-600 text-white rounded text-xs">
                Save Note
              </button>
              <button 
                className={`w-full py-1.5 px-3 rounded text-xs transition-all duration-500 flex items-center justify-center gap-1 ${
                  featureEnabled 
                    ? 'bg-purple-600 text-white' 
                    : yearning
                      ? 'bg-zinc-800 text-zinc-500 border border-dashed border-pink-500/50 animate-pulse'
                      : 'bg-zinc-800 text-zinc-600'
                }`}
              >
                <span>🔮 Semantic Search</span>
                {featureEnabled && <span>✨</span>}
              </button>
            </div>

            {/* Driver (inside app) */}
            <div className={`
              border rounded-lg p-2 transition-all duration-300
              ${driverNotified ? 'border-green-500/50 bg-green-500/5' : 
                yearning ? 'border-amber-500/50 bg-amber-500/5' : 
                'border-zinc-700 bg-zinc-800/50'}
            `}>
              <div className="flex items-center gap-2 mb-1">
                <div className={`w-1.5 h-1.5 rounded-full ${
                  driverNotified ? 'bg-green-400' : 
                  yearning ? 'bg-amber-400 animate-pulse' : 
                  'bg-zinc-600'
                }`} />
                <span className={`text-xs ${
                  driverNotified ? 'text-green-400' : 
                  yearning ? 'text-amber-400' : 
                  'text-zinc-500'
                }`}>
                  garden-driver
                </span>
              </div>
              <div className="text-xs text-zinc-500 font-mono">
                {stage === 0 && 'connecting...'}
                {stage === 1 && 'mongodb ✓'}
                {stage === 2 && 'listening...'}
                {stage === 3 && 'listening...'}
                {stage === 4 && '→ event!'}
                {stage === 5 && '→ notify app'}
                {stage === 6 && 'ollama ✓'}
              </div>
            </div>
          </div>
        </div>

        {/* Arrow: Ledger → App */}
        <div className="flex flex-col items-center">
          <Arrow direction="left" active={stage === 5} />
          {stage === 5 && <span className="text-green-400 text-xs mt-1">event</span>}
        </div>

        {/* CENTER: Capability Ledger */}
        <div className="w-48 flex-shrink-0">
          <div className="border border-zinc-700 rounded-xl p-4 bg-zinc-800/30">
            <div className="text-xs text-zinc-500 tracking-wider mb-3 text-center">CAPABILITY LEDGER</div>
            
            <div className="space-y-2">
              <LedgerEntry 
                name="mongodb" 
                status={stage >= 1 ? 'available' : 'offline'}
                subscribed={stage >= 1}
              />
              <LedgerEntry 
                name="redis" 
                status="available"
                subscribed={false}
              />
              <LedgerEntry 
                name="ollama" 
                status={ollamaInLedger ? 'available' : yearning ? 'yearning' : 'offline'}
                subscribed={ollamaInLedger}
              />
            </div>
          </div>
        </div>

        {/* Arrow: Garden → Ledger */}
        <div className="flex flex-col items-center">
          <Arrow direction="left" active={stage === 4} />
          {stage === 4 && <span className="text-green-400 text-xs mt-1">announce</span>}
        </div>

        {/* RIGHT: Garden (stones in column) */}
        <div className="w-48 flex-shrink-0">
          <div className="text-xs text-zinc-500 tracking-wider mb-3 text-center">GARDEN</div>
          
          <div className="space-y-2">
            <Stone 
              name="stone-coral" 
              capabilities={['mongodb']} 
            />
            <Stone 
              name="stone-amber" 
              capabilities={['redis']} 
            />
            <Stone 
              name="stone-leaf" 
              capabilities={[]} 
            />
            
            {/* GPU Stone */}
            <Stone 
              name="stone-gpu" 
              capabilities={['ollama', 'cuda']} 
              online={gpuOnline}
              highlight={stage === 3 || stage === 4}
            />
          </div>
          
          {stage === 3 && (
            <div className="mt-3 text-center text-green-400 text-xs animate-pulse">
              ⚡ plugged in!
            </div>
          )}
        </div>
      </div>

      {/* Stage description */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-2xl w-full">
        <div className="text-zinc-300 text-sm text-center">
          {stage === 0 && "App starts up, tells the garden what it needs and wants..."}
          {stage === 1 && "MongoDB found ✓ — ollama not available. Redis exists but app doesn't need it."}
          {stage === 2 && "App works fine, but Semantic Search is disabled. The driver listens for changes..."}
          {stage === 3 && "User plugs in the GPU machine. It announces itself to the garden."}
          {stage === 4 && "Ollama appears in the ledger → driver receives the announcement"}
          {stage === 5 && "Driver pushes capability event to the application"}
          {stage === 6 && "✨ Semantic Search lights up — no restart, no redeploy"}
        </div>
      </div>

      {/* Key insight */}
      <p className="mt-4 text-amber-200/60 text-sm text-center max-w-md">
        {stage < 6 
          ? "The app doesn't crash without ollama. It yearns. And keeps listening."
          : "Features emerge as hardware arrives. The garden grows, the app evolves."}
      </p>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-6">
        {[0,1,2,3,4,5,6].map(i => (
          <button
            key={i}
            onClick={() => setStage(i)}
            className={`w-2 h-2 rounded-full transition-colors ${
              stage === i ? 'bg-amber-400' : 'bg-zinc-700 hover:bg-zinc-600'
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
