import React, { useState, useEffect } from 'react';

export default function AbstractionTax() {
  const [view, setView] = useState('cloud');
  const [highlightLayer, setHighlightLayer] = useState(null);

  useEffect(() => {
    if (view === 'cloud') {
      const timer = setInterval(() => {
        setHighlightLayer(h => h === null ? 0 : h >= 6 ? null : h + 1);
      }, 800);
      return () => clearInterval(timer);
    } else {
      setHighlightLayer(null);
    }
  }, [view]);

  const cloudLayers = [
    { name: 'You', color: 'blue', desc: 'Click a button' },
    { name: 'Console', color: 'zinc', desc: 'AWS/Azure/GCP UI' },
    { name: 'API Gateway', color: 'zinc', desc: 'REST calls, auth tokens' },
    { name: 'Control Plane', color: 'zinc', desc: 'Orchestration layer' },
    { name: 'Hypervisor', color: 'zinc', desc: 'VM management' },
    { name: 'Virtual Machine', color: 'zinc', desc: 'Your "server"' },
    { name: 'Container Runtime', color: 'zinc', desc: 'Docker/containerd' },
    { name: 'Your App', color: 'green', desc: 'Finally!' },
  ];

  const gardenLayers = [
    { name: 'You', color: 'blue', desc: 'Type a command' },
    { name: 'Your App', color: 'green', desc: 'Running on the stone' },
  ];

  const Layer = ({ layer, index, total, highlighted, side }) => {
    const width = side === 'cloud' 
      ? 200 - (index * 15) 
      : 200;
    
    const colors = {
      blue: 'bg-blue-500/20 border-blue-500',
      green: 'bg-green-500/20 border-green-500',
      zinc: 'bg-zinc-800 border-zinc-700',
    };

    return (
      <div 
        className={`
          flex items-center justify-center border-2 rounded transition-all duration-300
          ${colors[layer.color]}
          ${highlighted ? 'ring-2 ring-amber-400' : ''}
        `}
        style={{ 
          width: `${width}px`, 
          height: '36px',
          opacity: highlighted === false ? 0.3 : 1,
        }}
      >
        <span className={`text-xs ${
          layer.color === 'zinc' ? 'text-zinc-400' : 
          layer.color === 'blue' ? 'text-blue-400' : 'text-green-400'
        }`}>
          {layer.name}
        </span>
      </div>
    );
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">THE ABSTRACTION TAX</h2>
      <p className="text-zinc-500 text-sm mb-6">every layer between you and your app costs something</p>

      {/* Toggle */}
      <div className="flex gap-2 mb-8">
        <button
          onClick={() => setView('cloud')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'cloud'
              ? 'border-red-500 bg-red-500/10 text-red-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Cloud
        </button>
        <button
          onClick={() => setView('garden')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'garden'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Zen Garden
        </button>
      </div>

      <div className="flex gap-16 items-start">
        
        {/* Cloud side */}
        <div className={`transition-opacity duration-500 ${view === 'cloud' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-4 text-center">CLOUD PATH</div>
          
          <div className="flex flex-col items-center gap-1">
            {cloudLayers.map((layer, i) => (
              <React.Fragment key={i}>
                <Layer 
                  layer={layer} 
                  index={i} 
                  total={cloudLayers.length}
                  highlighted={highlightLayer === null ? null : highlightLayer === i}
                  side="cloud"
                />
                {i < cloudLayers.length - 1 && (
                  <div className={`text-xs transition-colors ${
                    highlightLayer === i ? 'text-amber-400' : 'text-zinc-700'
                  }`}>
                    ↓
                  </div>
                )}
              </React.Fragment>
            ))}
          </div>

          <div className="mt-6 text-center">
            <div className="text-red-400 text-2xl font-light">8 layers</div>
            <div className="text-zinc-500 text-xs mt-1">
              Latency, complexity, cost at every step
            </div>
          </div>
        </div>

        {/* Divider */}
        <div className="flex flex-col items-center py-20">
          <div className="w-px h-32 bg-zinc-700" />
          <div className="text-zinc-600 text-xs py-2">vs</div>
          <div className="w-px h-32 bg-zinc-700" />
        </div>

        {/* Garden side */}
        <div className={`transition-opacity duration-500 ${view === 'garden' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-4 text-center">GARDEN PATH</div>
          
          <div className="flex flex-col items-center gap-1">
            {gardenLayers.map((layer, i) => (
              <React.Fragment key={i}>
                <Layer 
                  layer={layer} 
                  index={0} 
                  total={2}
                  highlighted={view === 'garden' ? true : null}
                  side="garden"
                />
                {i < gardenLayers.length - 1 && (
                  <div className="text-green-400 text-xs">↓</div>
                )}
              </React.Fragment>
            ))}
          </div>

          <div className="mt-6 text-center">
            <div className="text-green-400 text-2xl font-light">2 layers</div>
            <div className="text-zinc-500 text-xs mt-1">
              You and your app. That's it.
            </div>
          </div>

          {/* What's missing */}
          <div className="mt-8 p-4 bg-zinc-800/50 rounded-lg">
            <div className="text-zinc-500 text-xs mb-2">What about...</div>
            <div className="space-y-1 text-xs">
              <div className="flex justify-between">
                <span className="text-zinc-400">Orchestration?</span>
                <span className="text-green-400">Moss handles it</span>
              </div>
              <div className="flex justify-between">
                <span className="text-zinc-400">Networking?</span>
                <span className="text-green-400">mDNS, already there</span>
              </div>
              <div className="flex justify-between">
                <span className="text-zinc-400">Scaling?</span>
                <span className="text-green-400">Add a stone</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* The insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {view === 'cloud'
            ? "Every abstraction layer adds latency, failure modes, and concepts you need to understand. You're paying the tax whether you need the abstraction or not."
            : "The stone is a computer. Your app runs on it. Everything else is handled, not hidden behind layers you can't see or control."}
        </p>
      </div>
    </div>
  );
}
