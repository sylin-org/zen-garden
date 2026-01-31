import React, { useState, useEffect } from 'react';

export default function StoneHealthStates() {
  const [activeState, setActiveState] = useState('thriving');
  const [pulseFrame, setPulseFrame] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setPulseFrame(f => (f + 1) % 100);
    }, 50);
    return () => clearInterval(timer);
  }, []);

  const states = [
    {
      id: 'thriving',
      label: 'THRIVING',
      color: '#4ade80',
      fireflyColor: [255, 180, 100], // warm white
      description: 'All systems healthy',
      metrics: { cpu: '12%', memory: '34%', disk: '45%' },
      tempo: 'slow, meditative',
      sound: 'gentle ambient',
    },
    {
      id: 'withering',
      label: 'WITHERING',
      color: '#fbbf24',
      fireflyColor: [255, 180, 50], // amber shift
      description: 'Resources under pressure',
      metrics: { cpu: '78%', memory: '82%', disk: '89%' },
      tempo: 'quickening',
      sound: 'subtle tension',
    },
    {
      id: 'wilting',
      label: 'WILTING',
      color: '#f87171',
      fireflyColor: [255, 100, 80], // red shift
      description: 'Critical state',
      metrics: { cpu: '94%', memory: '96%', disk: '98%' },
      tempo: 'urgent pulse',
      sound: 'attention needed',
    },
  ];

  const currentState = states.find(s => s.id === activeState);

  // Firefly pulse based on state
  const getPulseIntensity = () => {
    const base = Math.sin(pulseFrame * 0.1) * 0.5 + 0.5;
    if (activeState === 'thriving') return base * 0.3 + 0.2; // gentle
    if (activeState === 'withering') return base * 0.5 + 0.3; // moderate
    return base * 0.8 + 0.2; // urgent
  };

  const getPulseSpeed = () => {
    if (activeState === 'thriving') return 'slow';
    if (activeState === 'withering') return 'medium';
    return 'fast';
  };

  const intensity = getPulseIntensity();

  // Mini firefly grid
  const FireflyPreview = () => {
    const color = currentState.fireflyColor;
    const count = activeState === 'thriving' ? 2 : activeState === 'withering' ? 3 : 5;
    
    return (
      <div className="grid grid-cols-5 gap-1">
        {Array.from({ length: 25 }).map((_, i) => {
          const isActive = i < count && Math.random() > 0.3;
          const pixelIntensity = isActive ? intensity * (0.7 + Math.random() * 0.3) : 0;
          
          return (
            <div
              key={i}
              className="w-3 h-3 rounded-sm"
              style={{
                backgroundColor: isActive
                  ? `rgba(${color[0]}, ${color[1]}, ${color[2]}, ${pixelIntensity})`
                  : 'rgba(63, 63, 70, 0.3)',
                boxShadow: isActive && pixelIntensity > 0.4
                  ? `0 0 ${pixelIntensity * 8}px rgba(${color[0]}, ${color[1]}, ${color[2]}, ${pixelIntensity * 0.5})`
                  : 'none',
              }}
            />
          );
        })}
      </div>
    );
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">STONE HEALTH</h2>
      <p className="text-zinc-500 text-sm mb-8">the garden tells you how it feels</p>

      {/* State selector */}
      <div className="flex gap-4 mb-8">
        {states.map(state => (
          <button
            key={state.id}
            onClick={() => setActiveState(state.id)}
            className={`px-4 py-2 rounded-lg border transition-all ${
              activeState === state.id
                ? `border-2 bg-opacity-20`
                : 'border-zinc-700 hover:border-zinc-600'
            }`}
            style={{
              borderColor: activeState === state.id ? state.color : undefined,
              backgroundColor: activeState === state.id ? `${state.color}15` : undefined,
            }}
          >
            <div 
              className="text-sm font-medium tracking-wide"
              style={{ color: activeState === state.id ? state.color : '#a1a1aa' }}
            >
              {state.label}
            </div>
          </button>
        ))}
      </div>

      {/* Main display */}
      <div className="flex gap-12 items-start">
        
        {/* Firefly preview */}
        <div className="flex flex-col items-center">
          <div className="text-zinc-500 text-xs mb-4 tracking-wide">FIREFLY RESPONSE</div>
          <div className="p-4 bg-zinc-950 rounded-lg border border-zinc-800">
            <FireflyPreview />
          </div>
          <div className="mt-3 text-zinc-500 text-xs">
            tempo: <span style={{ color: currentState.color }}>{currentState.tempo}</span>
          </div>
        </div>

        {/* Stone visualization */}
        <div className="flex flex-col items-center">
          <div className="text-zinc-500 text-xs mb-4 tracking-wide">STONE STATE</div>
          <div 
            className="w-32 h-24 rounded-lg border-2 flex flex-col items-center justify-center transition-all"
            style={{ 
              borderColor: currentState.color,
              boxShadow: `0 0 20px ${currentState.color}30`,
            }}
          >
            {/* LED indicator */}
            <div 
              className="w-4 h-4 rounded-full mb-2"
              style={{ 
                backgroundColor: currentState.color,
                boxShadow: `0 0 ${intensity * 15}px ${currentState.color}`,
                opacity: intensity,
              }}
            />
            <div className="text-zinc-400 text-xs">stone-coral</div>
          </div>
          <div className="mt-3 text-zinc-500 text-xs">
            audio: <span style={{ color: currentState.color }}>{currentState.sound}</span>
          </div>
        </div>

        {/* Metrics */}
        <div className="flex flex-col">
          <div className="text-zinc-500 text-xs mb-4 tracking-wide">METRICS</div>
          <div className="space-y-3">
            {Object.entries(currentState.metrics).map(([key, value]) => {
              const numValue = parseInt(value);
              const barColor = numValue > 90 ? '#f87171' : numValue > 70 ? '#fbbf24' : '#4ade80';
              
              return (
                <div key={key} className="flex items-center gap-3">
                  <div className="text-zinc-500 text-xs w-14">{key}</div>
                  <div className="w-24 h-2 bg-zinc-800 rounded-full overflow-hidden">
                    <div 
                      className="h-full rounded-full transition-all duration-500"
                      style={{ 
                        width: value,
                        backgroundColor: barColor,
                      }}
                    />
                  </div>
                  <div className="text-zinc-400 text-xs w-10">{value}</div>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Description */}
      <div 
        className="mt-8 p-4 rounded-lg border max-w-md text-center"
        style={{ borderColor: `${currentState.color}50` }}
      >
        <p style={{ color: currentState.color }} className="text-sm">
          {currentState.description}
        </p>
      </div>

      {/* Key insight */}
      <p className="text-zinc-600 text-xs mt-6 max-w-md text-center">
        You don't check metrics. You notice the room. The lights shift. The sound changes. The garden tells you.
      </p>
    </div>
  );
}
