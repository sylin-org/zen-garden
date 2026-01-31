import React, { useState, useEffect } from 'react';

export default function CeremonyWorkflow() {
  const [stage, setStage] = useState(0);
  const [scenario, setScenario] = useState('success'); // 'success' or 'rollback'

  useEffect(() => {
    const maxStage = scenario === 'success' ? 6 : 7;
    const durations = scenario === 'success' 
      ? [2000, 2000, 1500, 2000, 1500, 2000, 2500]
      : [2000, 2000, 1500, 2000, 2000, 2000, 2000, 2500];
    
    const timer = setTimeout(() => {
      setStage(s => (s + 1) % (maxStage + 1));
    }, durations[stage] || 2000);
    return () => clearTimeout(timer);
  }, [stage, scenario]);

  // Success: 0=start, 1=quiesce, 2=harvest, 3=resume, 4=update, 5=verify, 6=done
  // Rollback: 0=start, 1=quiesce, 2=harvest, 3=resume, 4=update, 5=verify-fail, 6=rollback, 7=recovered

  const phases = [
    { id: 'collect', label: 'COLLECT', color: '#fbbf24' },
    { id: 'apply', label: 'APPLY', color: '#a78bfa' },
    { id: 'verify', label: 'VERIFY', color: scenario === 'success' ? '#4ade80' : '#f87171' },
  ];

  const getCurrentPhase = () => {
    if (stage <= 3) return 0; // collect
    if (stage <= 4) return 1; // apply
    return 2; // verify
  };

  const Service = ({ state, version }) => (
    <div className={`p-3 rounded border ${
      state === 'running' ? 'border-green-500/50 bg-green-500/10' :
      state === 'paused' ? 'border-amber-500/50 bg-amber-500/10' :
      state === 'updating' ? 'border-purple-500/50 bg-purple-500/10' :
      state === 'failed' ? 'border-red-500/50 bg-red-500/10' :
      'border-zinc-700 bg-zinc-800'
    }`}>
      <div className="flex items-center gap-2 mb-1">
        <div className={`w-2 h-2 rounded-full ${
          state === 'running' ? 'bg-green-400' :
          state === 'paused' ? 'bg-amber-400 animate-pulse' :
          state === 'updating' ? 'bg-purple-400 animate-pulse' :
          state === 'failed' ? 'bg-red-400' :
          'bg-zinc-600'
        }`} />
        <span className="text-zinc-300 text-sm font-medium">mongodb</span>
      </div>
      <div className="text-zinc-500 text-xs">{version}</div>
      <div className={`text-xs mt-1 ${
        state === 'running' ? 'text-green-400' :
        state === 'paused' ? 'text-amber-400' :
        state === 'updating' ? 'text-purple-400' :
        state === 'failed' ? 'text-red-400' :
        'text-zinc-500'
      }`}>{state}</div>
    </div>
  );

  const Harvest = ({ active, restoring }) => (
    <div className={`p-3 rounded border transition-all duration-500 ${
      active ? 'border-amber-500/50 bg-amber-500/10' :
      restoring ? 'border-green-500/50 bg-green-500/10 animate-pulse' :
      'border-zinc-800 bg-zinc-800/30'
    }`}>
      <div className="text-amber-400 text-sm mb-1">📦 harvest</div>
      <div className="text-zinc-500 text-xs">
        {active ? 'capturing...' : restoring ? 'restoring...' : 'v7.0.5 snapshot'}
      </div>
      <div className="text-zinc-600 text-xs mt-1">2.3 GB</div>
    </div>
  );

  const getServiceState = () => {
    if (scenario === 'success') {
      if (stage === 0) return { state: 'running', version: 'v7.0.5' };
      if (stage === 1 || stage === 2) return { state: 'paused', version: 'v7.0.5' };
      if (stage === 3) return { state: 'running', version: 'v7.0.5' };
      if (stage === 4) return { state: 'updating', version: 'v7.0.5 → v7.0.6' };
      return { state: 'running', version: 'v7.0.6' };
    } else {
      if (stage === 0) return { state: 'running', version: 'v7.0.5' };
      if (stage === 1 || stage === 2) return { state: 'paused', version: 'v7.0.5' };
      if (stage === 3) return { state: 'running', version: 'v7.0.5' };
      if (stage === 4) return { state: 'updating', version: 'v7.0.5 → v7.0.6' };
      if (stage === 5) return { state: 'failed', version: 'v7.0.6' };
      if (stage === 6) return { state: 'updating', version: 'rolling back...' };
      return { state: 'running', version: 'v7.0.5' };
    }
  };

  const serviceState = getServiceState();
  const harvestActive = stage === 2;
  const harvestRestoring = scenario === 'rollback' && stage === 6;

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">CEREMONY</h2>
      <p className="text-zinc-500 text-sm mb-4">safe updates with automatic rollback</p>

      {/* Scenario selector */}
      <div className="flex gap-2 mb-6">
        <button
          onClick={() => { setScenario('success'); setStage(0); }}
          className={`px-3 py-1 rounded text-xs transition-colors ${
            scenario === 'success' 
              ? 'bg-green-400/20 text-green-400 border border-green-400/50' 
              : 'bg-zinc-800 text-zinc-500 border border-zinc-700 hover:border-zinc-600'
          }`}
        >
          Successful update
        </button>
        <button
          onClick={() => { setScenario('rollback'); setStage(0); }}
          className={`px-3 py-1 rounded text-xs transition-colors ${
            scenario === 'rollback' 
              ? 'bg-red-400/20 text-red-400 border border-red-400/50' 
              : 'bg-zinc-800 text-zinc-500 border border-zinc-700 hover:border-zinc-600'
          }`}
        >
          Failed → Rollback
        </button>
      </div>

      {/* Command */}
      <div className="bg-zinc-800 rounded px-4 py-2 mb-6 font-mono text-sm">
        <span className="text-zinc-500">$ </span>
        <span className="text-zinc-300">garden-rake nourish mongodb</span>
      </div>

      {/* Phase indicators */}
      <div className="flex gap-8 mb-8">
        {phases.map((phase, i) => (
          <div 
            key={phase.id}
            className={`flex flex-col items-center transition-all duration-300 ${
              getCurrentPhase() === i ? 'opacity-100' : 'opacity-40'
            }`}
          >
            <div 
              className="w-3 h-3 rounded-full mb-2"
              style={{ backgroundColor: getCurrentPhase() >= i ? phase.color : '#3f3f46' }}
            />
            <span className="text-xs tracking-wider" style={{ 
              color: getCurrentPhase() === i ? phase.color : '#71717a' 
            }}>
              {phase.label}
            </span>
          </div>
        ))}
      </div>

      {/* Main visualization */}
      <div className="flex items-center gap-8">
        <Service {...serviceState} />
        
        <div className="flex flex-col items-center gap-2">
          {/* Arrow or rollback indicator */}
          {scenario === 'rollback' && stage >= 6 ? (
            <svg width="60" height="40" viewBox="0 0 60 40">
              <path 
                d="M 50 20 L 10 20 M 20 10 L 10 20 L 20 30" 
                fill="none" 
                stroke="#f87171" 
                strokeWidth="2"
              />
            </svg>
          ) : (
            <svg width="60" height="40" viewBox="0 0 60 40">
              <path 
                d="M 10 20 L 50 20 M 40 10 L 50 20 L 40 30" 
                fill="none" 
                stroke={stage >= 4 ? '#a78bfa' : '#3f3f46'} 
                strokeWidth="2"
              />
            </svg>
          )}
        </div>

        <Harvest active={harvestActive} restoring={harvestRestoring} />
      </div>

      {/* Step description */}
      <div className="mt-8 p-4 border border-zinc-800 rounded max-w-lg w-full min-h-20">
        <div className="text-zinc-300 text-sm">
          {scenario === 'success' && (
            <>
              {stage === 0 && "Starting nourishment ceremony..."}
              {stage === 1 && "Quiescing database (fsyncLock)..."}
              {stage === 2 && "Capturing harvest snapshot..."}
              {stage === 3 && "Resuming database (fsyncUnlock)..."}
              {stage === 4 && "Pulling new image, recreating container..."}
              {stage === 5 && "Running health checks..."}
              {stage === 6 && "✓ Nourished mongodb: 7.0.5 → 7.0.6"}
            </>
          )}
          {scenario === 'rollback' && (
            <>
              {stage === 0 && "Starting nourishment ceremony..."}
              {stage === 1 && "Quiescing database (fsyncLock)..."}
              {stage === 2 && "Capturing harvest snapshot..."}
              {stage === 3 && "Resuming database (fsyncUnlock)..."}
              {stage === 4 && "Pulling new image, recreating container..."}
              {stage === 5 && "✗ Health check failed! Initiating rollback..."}
              {stage === 6 && "Restoring from harvest..."}
              {stage === 7 && "✓ Rolled back to 7.0.5 — data safe"}
            </>
          )}
        </div>
      </div>

      {/* Key insight */}
      <p className="text-zinc-600 text-xs mt-6 max-w-md text-center">
        The harvest is captured before any changes. If anything fails, the ceremony restores from it automatically.
      </p>

      <button 
        onClick={() => setStage(0)}
        className="mt-4 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset animation
      </button>
    </div>
  );
}
