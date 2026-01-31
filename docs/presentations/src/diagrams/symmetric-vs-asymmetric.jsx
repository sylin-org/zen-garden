import React, { useState, useEffect } from 'react';

export default function SymmetricVsAsymmetric() {
  const [stage, setStage] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => {
      setStage(s => (s + 1) % 4);
    }, 3000);
    return () => clearInterval(timer);
  }, []);

  // Stages:
  // 0: Show cloud - identical boxes
  // 1: Show problem - paying for uniformity you don't need
  // 2: Show garden - diverse hardware
  // 3: Show insight - each stone does what it's good at

  const CloudBox = ({ index, highlight }) => (
    <div className={`
      w-24 h-20 rounded border-2 flex flex-col items-center justify-center
      transition-all duration-500
      ${highlight ? 'border-amber-500 bg-amber-500/10' : 'border-zinc-600 bg-zinc-800'}
    `}>
      <div className="text-zinc-400 text-xs mb-1">t3.large</div>
      <div className="text-zinc-500 text-xs">2 vCPU</div>
      <div className="text-zinc-500 text-xs">8 GB</div>
      {highlight && (
        <div className="text-amber-400 text-xs mt-1">$60/mo</div>
      )}
    </div>
  );

  const GardenStone = ({ name, specs, strength, color = 'blue' }) => {
    const colors = {
      blue: 'border-blue-500 bg-blue-500/10',
      green: 'border-green-500 bg-green-500/10',
      purple: 'border-purple-500 bg-purple-500/10',
      amber: 'border-amber-500 bg-amber-500/10',
    };
    
    return (
      <div className={`
        rounded border-2 p-3 transition-all duration-500 ${colors[color]}
      `} style={{ width: specs.width || 'auto' }}>
        <div className="text-zinc-300 text-sm mb-1">{name}</div>
        <div className="text-zinc-500 text-xs">{specs.cpu}</div>
        <div className="text-zinc-500 text-xs">{specs.ram}</div>
        {specs.special && (
          <div className={`text-xs mt-1 ${
            color === 'blue' ? 'text-blue-400' :
            color === 'green' ? 'text-green-400' :
            color === 'purple' ? 'text-purple-400' :
            'text-amber-400'
          }`}>
            {specs.special}
          </div>
        )}
        {strength && (
          <div className="text-zinc-400 text-xs mt-2 italic">
            → {strength}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">SYMMETRIC vs ASYMMETRIC</h2>
      <p className="text-zinc-500 text-sm mb-8">cloud assumes uniformity — your hardware doesn't</p>

      <div className="flex gap-16 items-start max-w-5xl">
        
        {/* Cloud side */}
        <div className="flex-1">
          <div className="text-center mb-4">
            <span className={`text-sm tracking-wider ${stage < 2 ? 'text-red-400' : 'text-zinc-600'}`}>
              CLOUD
            </span>
          </div>
          
          <div className={`transition-opacity duration-500 ${stage >= 2 ? 'opacity-30' : 'opacity-100'}`}>
            <div className="flex gap-3 justify-center mb-4">
              <CloudBox index={0} highlight={stage === 1} />
              <CloudBox index={1} highlight={stage === 1} />
              <CloudBox index={2} highlight={stage === 1} />
            </div>
            
            {stage === 0 && (
              <div className="text-center text-zinc-500 text-sm">
                Three identical instances
              </div>
            )}
            
            {stage === 1 && (
              <div className="text-center space-y-2">
                <div className="text-amber-400 text-sm">$180/month</div>
                <div className="text-zinc-500 text-xs max-w-xs mx-auto">
                  Same specs. Same cost. Even if one just runs cron jobs 
                  and another needs a GPU you can't have.
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Divider */}
        <div className="flex flex-col items-center gap-2 py-8">
          <div className="w-px h-16 bg-zinc-700" />
          <div className="text-zinc-600 text-xs">vs</div>
          <div className="w-px h-16 bg-zinc-700" />
        </div>

        {/* Garden side */}
        <div className="flex-1">
          <div className="text-center mb-4">
            <span className={`text-sm tracking-wider ${stage >= 2 ? 'text-green-400' : 'text-zinc-600'}`}>
              GARDEN
            </span>
          </div>
          
          <div className={`transition-opacity duration-500 ${stage < 2 ? 'opacity-30' : 'opacity-100'}`}>
            <div className="flex gap-3 items-end justify-center mb-4">
              <GardenStone 
                name="stone-gpu"
                specs={{ cpu: '4 cores', ram: '16 GB', special: 'RTX 3060', width: '100px' }}
                strength={stage === 3 ? "ML inference" : null}
                color="purple"
              />
              <GardenStone 
                name="stone-coral"
                specs={{ cpu: '4 cores', ram: '8 GB', special: 'NVMe', width: '90px' }}
                strength={stage === 3 ? "databases" : null}
                color="blue"
              />
              <GardenStone 
                name="stone-tiny"
                specs={{ cpu: '2 cores', ram: '2 GB', special: '6W idle', width: '80px' }}
                strength={stage === 3 ? "always-on" : null}
                color="green"
              />
            </div>
            
            {stage === 2 && (
              <div className="text-center space-y-2">
                <div className="text-green-400 text-sm">~$8/month electricity</div>
                <div className="text-zinc-500 text-xs max-w-xs mx-auto">
                  Different shapes. Different strengths. 
                  Hardware you already have.
                </div>
              </div>
            )}
            
            {stage === 3 && (
              <div className="text-center space-y-2">
                <div className="text-zinc-400 text-sm">Each does what it's good at</div>
                <div className="text-zinc-500 text-xs max-w-xs mx-auto">
                  The garden knows. Placement adapts to capability.
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Key insight */}
      <div className="mt-12 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {stage < 2 
            ? "Cloud providers sell uniformity. You pay the same whether you need it or not."
            : "Your hardware is already asymmetric. The garden embraces that."}
        </p>
      </div>

      {/* Stage indicators */}
      <div className="flex gap-2 mt-6">
        {[0,1,2,3].map(i => (
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
