import React, { useState, useEffect } from 'react';

export default function GracefulDegradation() {
  const [stage, setStage] = useState(0);

  useEffect(() => {
    const durations = [3000, 2000, 2500, 2500, 2500, 3000];
    const timer = setTimeout(() => {
      setStage(s => (s + 1) % 6);
    }, durations[stage]);
    return () => clearTimeout(timer);
  }, [stage]);

  // Stages:
  // 0: Healthy garden - 3 stones, services distributed
  // 1: Stone-coral goes offline (click sound, drive dying)
  // 2: Garden detects, offerings orphaned
  // 3: Migration begins - mongodb moves to stone-amber
  // 4: Apps reconnect via discovery
  // 5: Garden stable again, one less stone

  const Stone = ({ name, status, offerings = [], highlight }) => {
    const statusColors = {
      healthy: 'border-green-500/50 bg-green-500/5',
      offline: 'border-red-500/50 bg-red-500/5 opacity-50',
      receiving: 'border-amber-500 bg-amber-500/10',
      busy: 'border-blue-500/50 bg-blue-500/5',
    };

    const ledColors = {
      healthy: 'bg-green-400',
      offline: 'bg-red-400 animate-pulse',
      receiving: 'bg-amber-400 animate-pulse',
      busy: 'bg-blue-400',
    };

    return (
      <div className={`
        p-4 rounded-lg border-2 transition-all duration-500 w-40
        ${highlight ? 'ring-2 ring-amber-400' : ''}
        ${statusColors[status]}
      `}>
        <div className="flex items-center gap-2 mb-2">
          <div className={`w-2 h-2 rounded-full ${ledColors[status]}`} />
          <span className={`text-sm ${status === 'offline' ? 'text-zinc-500 line-through' : 'text-zinc-300'}`}>
            {name}
          </span>
        </div>
        
        <div className="space-y-1">
          {offerings.map((offering, i) => (
            <div 
              key={i}
              className={`text-xs px-2 py-1 rounded ${
                offering.migrating 
                  ? 'bg-amber-500/20 text-amber-400 animate-pulse' 
                  : offering.new
                    ? 'bg-green-500/20 text-green-400'
                    : 'bg-zinc-800 text-zinc-400'
              }`}
            >
              {offering.name}
              {offering.migrating && ' →'}
              {offering.new && ' ✓'}
            </div>
          ))}
          {offerings.length === 0 && (
            <div className="text-xs text-zinc-600 italic">no offerings</div>
          )}
        </div>
        
        {status === 'offline' && (
          <div className="text-red-400 text-xs mt-2">⚠ offline</div>
        )}
      </div>
    );
  };

  const App = ({ name, connected, reconnecting }) => (
    <div className={`
      px-3 py-2 rounded border transition-all duration-500
      ${reconnecting 
        ? 'border-amber-500/50 bg-amber-500/5' 
        : connected 
          ? 'border-green-500/50 bg-green-500/5'
          : 'border-red-500/50 bg-red-500/5'}
    `}>
      <div className="flex items-center gap-2">
        <div className={`w-2 h-2 rounded-full ${
          reconnecting ? 'bg-amber-400 animate-pulse' : connected ? 'bg-green-400' : 'bg-red-400'
        }`} />
        <span className="text-zinc-400 text-sm">{name}</span>
      </div>
      <div className={`text-xs mt-1 ${
        reconnecting ? 'text-amber-400' : connected ? 'text-green-400' : 'text-red-400'
      }`}>
        {reconnecting ? 'reconnecting...' : connected ? 'connected' : 'connection lost'}
      </div>
    </div>
  );

  // Stone states based on stage
  const getStoneStatus = (stoneName) => {
    if (stoneName === 'stone-coral') {
      return stage >= 1 ? 'offline' : 'healthy';
    }
    if (stoneName === 'stone-amber') {
      if (stage === 3) return 'receiving';
      return 'healthy';
    }
    return 'healthy';
  };

  const getOfferings = (stoneName) => {
    if (stoneName === 'stone-coral') {
      if (stage === 0) return [{ name: 'mongodb' }, { name: 'redis' }];
      if (stage === 1 || stage === 2) return [{ name: 'mongodb', migrating: false }, { name: 'redis', migrating: false }];
      return []; // offline, offerings orphaned
    }
    if (stoneName === 'stone-amber') {
      if (stage <= 2) return [{ name: 'postgres' }];
      if (stage === 3) return [{ name: 'postgres' }, { name: 'mongodb', migrating: true }];
      return [{ name: 'postgres' }, { name: 'mongodb', new: true }];
    }
    if (stoneName === 'stone-leaf') {
      if (stage <= 3) return [];
      if (stage === 4) return [{ name: 'redis', migrating: true }];
      return [{ name: 'redis', new: true }];
    }
    return [];
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">GRACEFUL DEGRADATION</h2>
      <p className="text-zinc-500 text-sm mb-8">when hardware fails, the garden adapts</p>

      {/* Main visualization */}
      <div className="flex items-center gap-8 mb-8">
        {/* Apps */}
        <div className="space-y-3">
          <div className="text-zinc-600 text-xs tracking-wider text-center mb-2">APPS</div>
          <App 
            name="my-app" 
            connected={stage === 0 || stage >= 5}
            reconnecting={stage >= 2 && stage < 5}
          />
          <App 
            name="api-server" 
            connected={stage === 0 || stage >= 5}
            reconnecting={stage >= 2 && stage < 5}
          />
        </div>

        {/* Arrow */}
        <div className="flex flex-col items-center">
          <svg width="48" height="24" viewBox="0 0 48 24">
            <path 
              d="M0 12 L40 12 M34 6 L40 12 L34 18" 
              fill="none" 
              stroke={stage >= 4 ? '#4ade80' : stage >= 2 ? '#fbbf24' : '#4ade80'} 
              strokeWidth="2"
            />
          </svg>
          <span className={`text-xs ${
            stage >= 4 ? 'text-green-400' : stage >= 2 ? 'text-amber-400' : 'text-green-400'
          }`}>
            {stage >= 4 ? 'discovery ✓' : stage >= 2 ? 'discovering...' : 'discovery'}
          </span>
        </div>

        {/* Stones */}
        <div className="flex gap-4">
          <Stone 
            name="stone-coral" 
            status={getStoneStatus('stone-coral')}
            offerings={getOfferings('stone-coral')}
          />
          <Stone 
            name="stone-amber" 
            status={getStoneStatus('stone-amber')}
            offerings={getOfferings('stone-amber')}
            highlight={stage === 3}
          />
          <Stone 
            name="stone-leaf" 
            status={getStoneStatus('stone-leaf')}
            offerings={getOfferings('stone-leaf')}
            highlight={stage === 4}
          />
        </div>
      </div>

      {/* Stage description */}
      <div className="p-4 border border-zinc-800 rounded-lg max-w-xl w-full mb-6">
        <div className="text-zinc-300 text-sm text-center">
          {stage === 0 && "Garden healthy. Three stones, services distributed."}
          {stage === 1 && "⚠️ stone-coral's drive starts clicking. The death rattle."}
          {stage === 2 && "Garden detects offline stone. Offerings orphaned. Apps lose connection."}
          {stage === 3 && "Migration begins. mongodb → stone-amber (has capacity)."}
          {stage === 4 && "redis → stone-leaf. Apps rediscover services at new locations."}
          {stage === 5 && "✓ Garden stable. Same services, fewer stones. Apps never restarted."}
        </div>
      </div>

      {/* Timeline */}
      <div className="flex items-center gap-2 mb-6">
        {['Healthy', 'Failure', 'Detect', 'Migrate', 'Reconnect', 'Stable'].map((label, i) => (
          <React.Fragment key={i}>
            <div className="flex flex-col items-center">
              <div className={`
                w-4 h-4 rounded-full border-2 flex items-center justify-center
                ${stage >= i 
                  ? 'border-green-500 bg-green-500' 
                  : 'border-zinc-600 bg-transparent'}
              `}>
                {stage > i && <span className="text-white text-xs">✓</span>}
              </div>
              <span className={`text-xs mt-1 ${stage >= i ? 'text-zinc-400' : 'text-zinc-600'}`}>
                {label}
              </span>
            </div>
            {i < 5 && (
              <div className={`w-8 h-0.5 ${stage > i ? 'bg-green-500' : 'bg-zinc-700'}`} />
            )}
          </React.Fragment>
        ))}
      </div>

      {/* Key insight */}
      <div className="p-4 border border-zinc-800 rounded-lg max-w-lg">
        <p className="text-amber-200/70 text-sm text-center">
          {stage < 5 
            ? "No operator intervention. No 3am pages. The garden heals itself."
            : "Hardware died. Services migrated. Apps reconnected. Life continued."}
        </p>
      </div>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-6">
        {[0,1,2,3,4,5].map(i => (
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
        reset
      </button>
    </div>
  );
}
