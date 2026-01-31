import React, { useState, useEffect } from 'react';

export default function FeedbackThroughGlass() {
  const [view, setView] = useState('dashboard');
  const [alerting, setAlerting] = useState(false);
  const [pulseFrame, setPulseFrame] = useState(0);

  useEffect(() => {
    // Simulate alert coming in
    const alertTimer = setInterval(() => {
      if (view === 'dashboard') {
        setAlerting(a => !a);
      }
    }, 3000);

    // Firefly animation
    const pulseTimer = setInterval(() => {
      setPulseFrame(f => (f + 1) % 100);
    }, 50);

    return () => {
      clearInterval(alertTimer);
      clearInterval(pulseTimer);
    };
  }, [view]);

  // Dashboard metrics (fake)
  const metrics = [
    { name: 'CPU', value: alerting ? 94 : 23, unit: '%' },
    { name: 'Memory', value: alerting ? 87 : 45, unit: '%' },
    { name: 'Disk', value: 67, unit: '%' },
    { name: 'Network', value: alerting ? 'degraded' : 'healthy', unit: '' },
  ];

  // Firefly colors based on state
  const fireflyColor = alerting 
    ? [255, 100, 80]  // red-ish
    : [255, 180, 100]; // warm white

  const fireflyTempo = alerting ? 0.3 : 0.08;
  const fireflyIntensity = Math.sin(pulseFrame * fireflyTempo) * 0.4 + 0.5;

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">FEEDBACK THROUGH GLASS</h2>
      <p className="text-zinc-500 text-sm mb-6">dashboards demand attention — presence just exists</p>

      {/* Toggle */}
      <div className="flex gap-2 mb-8">
        <button
          onClick={() => { setView('dashboard'); setAlerting(false); }}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'dashboard'
              ? 'border-red-500 bg-red-500/10 text-red-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Traditional
        </button>
        <button
          onClick={() => { setView('ambient'); setAlerting(false); }}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'ambient'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Ambient
        </button>
      </div>

      <div className="flex gap-16 max-w-5xl w-full">
        
        {/* Dashboard view */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'dashboard' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">THE DASHBOARD</div>
          
          {/* Fake dashboard */}
          <div className="bg-zinc-950 rounded-lg p-4 border border-zinc-800">
            {/* Alert banner */}
            {alerting && (
              <div className="bg-red-500/20 border border-red-500 rounded p-2 mb-4 flex items-center gap-2 animate-pulse">
                <span className="text-red-400">🚨</span>
                <span className="text-red-400 text-sm">ALERT: CPU threshold exceeded</span>
              </div>
            )}

            {/* Metrics grid */}
            <div className="grid grid-cols-2 gap-3">
              {metrics.map((metric, i) => (
                <div key={i} className="bg-zinc-800 rounded p-3">
                  <div className="text-zinc-500 text-xs mb-1">{metric.name}</div>
                  <div className={`text-xl font-light ${
                    metric.name === 'CPU' && alerting ? 'text-red-400' :
                    metric.name === 'Memory' && alerting ? 'text-amber-400' :
                    metric.value === 'degraded' ? 'text-red-400' :
                    'text-zinc-300'
                  }`}>
                    {metric.value}{metric.unit}
                  </div>
                </div>
              ))}
            </div>

            {/* Fake chart */}
            <div className="mt-4 h-20 bg-zinc-800 rounded flex items-end p-2 gap-1">
              {Array.from({ length: 20 }).map((_, i) => (
                <div 
                  key={i}
                  className={`flex-1 rounded-t ${alerting && i > 15 ? 'bg-red-400' : 'bg-blue-400/50'}`}
                  style={{ height: `${20 + Math.sin(i * 0.5 + pulseFrame * 0.1) * 30 + (alerting && i > 15 ? 40 : 0)}%` }}
                />
              ))}
            </div>
          </div>

          {/* The problem */}
          <div className="mt-4 p-3 border border-red-500/30 rounded bg-red-500/5 space-y-2">
            <div className="text-red-400 text-sm">The ritual:</div>
            <div className="text-zinc-500 text-xs">1. Phone buzzes</div>
            <div className="text-zinc-500 text-xs">2. Open laptop</div>
            <div className="text-zinc-500 text-xs">3. Log into console</div>
            <div className="text-zinc-500 text-xs">4. Navigate to dashboard</div>
            <div className="text-zinc-500 text-xs">5. Interpret the numbers</div>
            <div className="text-zinc-500 text-xs">6. Decide if it's real</div>
          </div>
        </div>

        {/* Ambient view */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'ambient' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">THE GARDEN</div>
          
          {/* Firefly visualization */}
          <div className="bg-zinc-950 rounded-lg p-8 border border-zinc-800 flex flex-col items-center">
            {/* Firefly grid */}
            <div className="grid grid-cols-5 gap-2 mb-6">
              {Array.from({ length: 25 }).map((_, i) => {
                const isActive = view === 'ambient' && (
                  alerting 
                    ? i % 3 === 0 // More active when alerting
                    : i === 12 || i === 7 // Just a couple when idle
                );
                const pixelIntensity = isActive 
                  ? fireflyIntensity * (0.7 + Math.random() * 0.3)
                  : 0;

                return (
                  <div
                    key={i}
                    className="w-4 h-4 rounded"
                    style={{
                      backgroundColor: isActive
                        ? `rgba(${fireflyColor[0]}, ${fireflyColor[1]}, ${fireflyColor[2]}, ${pixelIntensity})`
                        : 'rgba(63, 63, 70, 0.3)',
                      boxShadow: isActive && pixelIntensity > 0.4
                        ? `0 0 ${pixelIntensity * 10}px rgba(${fireflyColor[0]}, ${fireflyColor[1]}, ${fireflyColor[2]}, ${pixelIntensity * 0.5})`
                        : 'none',
                    }}
                  />
                );
              })}
            </div>

            <div className="text-center">
              <div className={`text-sm ${alerting ? 'text-amber-400' : 'text-zinc-400'}`}>
                {alerting ? 'Rhythm quickened' : 'Slow, meditative pulse'}
              </div>
              <div className="text-zinc-600 text-xs mt-1">
                {alerting ? 'Something needs attention' : 'All is well'}
              </div>
            </div>

            {/* Cricket indicator */}
            <div className="mt-6 flex items-center gap-2">
              <div className={`w-2 h-2 rounded-full ${alerting ? 'bg-amber-400' : 'bg-green-400'}`} />
              <span className="text-zinc-500 text-xs">
                {alerting ? 'Cricket tempo: urgent' : 'Cricket: gentle ambient'}
              </span>
            </div>
          </div>

          {/* The solution */}
          <div className="mt-4 p-3 border border-green-500/30 rounded bg-green-500/5 space-y-2">
            <div className="text-green-400 text-sm">The experience:</div>
            <div className="text-zinc-500 text-xs">You're making coffee.</div>
            <div className="text-zinc-500 text-xs">You notice the lights are faster.</div>
            <div className="text-zinc-500 text-xs">Something changed.</div>
            <div className="text-zinc-500 text-xs mt-2 italic">No phone. No login. No ritual.</div>
          </div>
        </div>
      </div>

      {/* Trigger alert button */}
      <button
        onClick={() => setAlerting(!alerting)}
        className="mt-6 px-4 py-2 bg-zinc-800 border border-zinc-700 rounded text-zinc-400 text-sm hover:border-zinc-600 transition-colors"
      >
        {alerting ? 'Clear alert' : 'Simulate load spike'}
      </button>

      {/* The insight */}
      <div className="mt-6 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {view === 'dashboard'
            ? "Dashboards are for debugging, not awareness. You check them when something's already wrong."
            : "You don't check the garden. You live with it. Changes in rhythm are felt, not read."}
        </p>
      </div>
    </div>
  );
}
