import React, { useState } from 'react';

export default function Tending() {
  const [tendedStone, setTendedStone] = useState(null);
  const [commandHistory, setCommandHistory] = useState([]);

  const stones = [
    { name: 'stone-coral', offerings: ['mongodb', 'redis'] },
    { name: 'stone-amber', offerings: ['postgres'] },
    { name: 'stone-leaf', offerings: [] },
  ];

  const runCommand = (cmd) => {
    const newHistory = [...commandHistory, { cmd, tended: tendedStone }];
    
    if (cmd.startsWith('tend ')) {
      const stoneName = cmd.replace('tend ', '');
      const stone = stones.find(s => s.name === stoneName);
      if (stone) {
        setTendedStone(stoneName);
        newHistory[newHistory.length - 1].output = `Now tending ${stoneName}`;
      }
    } else if (cmd === 'untend') {
      setTendedStone(null);
      newHistory[newHistory.length - 1].output = 'No longer tending any stone';
    } else if (cmd === 'status') {
      if (tendedStone) {
        const stone = stones.find(s => s.name === tendedStone);
        newHistory[newHistory.length - 1].output = `${tendedStone}: ${stone.offerings.join(', ') || 'no offerings'}`;
      } else {
        newHistory[newHistory.length - 1].output = 'Tip: tend a stone first, or use --at';
      }
    } else if (cmd === 'offer redis') {
      if (tendedStone) {
        newHistory[newHistory.length - 1].output = `Planting redis on ${tendedStone}...`;
      } else {
        newHistory[newHistory.length - 1].output = 'Which stone? Use: tend <stone> first';
      }
    }
    
    setCommandHistory(newHistory.slice(-6));
  };

  const presetCommands = [
    'tend stone-coral',
    'status',
    'offer redis',
    'tend stone-amber',
    'status',
    'untend',
  ];

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">TENDING</h2>
      <p className="text-zinc-500 text-sm mb-8">context, not flags — like cd for infrastructure</p>

      <div className="flex gap-12 max-w-4xl w-full">
        
        {/* Left: The Problem */}
        <div className="flex-1">
          <div className="text-zinc-500 text-xs tracking-wider mb-3">WITHOUT TENDING</div>
          
          <div className="bg-zinc-800/50 rounded-lg p-4 font-mono text-sm space-y-2">
            <div className="text-zinc-500">$ garden-rake status <span className="text-red-400">--at stone-coral</span></div>
            <div className="text-zinc-500">$ garden-rake offer redis <span className="text-red-400">--at stone-coral</span></div>
            <div className="text-zinc-500">$ garden-rake logs mongodb <span className="text-red-400">--at stone-coral</span></div>
            <div className="text-zinc-500">$ garden-rake health <span className="text-red-400">--at stone-coral</span></div>
          </div>
          
          <div className="mt-4 p-3 border border-red-500/30 rounded bg-red-500/5">
            <div className="text-red-400 text-sm">Repetitive. Error-prone.</div>
            <div className="text-zinc-500 text-xs mt-1">Copy-paste the wrong flag, hit the wrong stone.</div>
          </div>
        </div>

        {/* Right: The Solution */}
        <div className="flex-1">
          <div className="text-zinc-500 text-xs tracking-wider mb-3">WITH TENDING</div>
          
          <div className="bg-zinc-800/50 rounded-lg p-4 font-mono text-sm space-y-2">
            <div><span className="text-green-400">$ garden-rake tend stone-coral</span></div>
            <div className="text-zinc-400">$ garden-rake status</div>
            <div className="text-zinc-400">$ garden-rake offer redis</div>
            <div className="text-zinc-400">$ garden-rake logs mongodb</div>
            <div className="text-zinc-400">$ garden-rake health</div>
          </div>
          
          <div className="mt-4 p-3 border border-green-500/30 rounded bg-green-500/5">
            <div className="text-green-400 text-sm">Set context once. Work naturally.</div>
            <div className="text-zinc-500 text-xs mt-1">Like cd, but for infrastructure.</div>
          </div>
        </div>
      </div>

      {/* Interactive demo */}
      <div className="mt-10 w-full max-w-2xl">
        <div className="text-zinc-500 text-xs tracking-wider mb-3">TRY IT</div>
        
        <div className="flex gap-6">
          {/* Terminal */}
          <div className="flex-1 bg-zinc-950 rounded-lg p-4 font-mono text-sm">
            <div className="text-zinc-600 text-xs mb-3">
              {tendedStone 
                ? <span>tending: <span className="text-green-400">{tendedStone}</span></span>
                : <span>tending: <span className="text-zinc-500">none</span></span>
              }
            </div>
            
            <div className="space-y-1 h-36 overflow-hidden">
              {commandHistory.map((entry, i) => (
                <div key={i}>
                  <div className="text-zinc-400">
                    <span className="text-zinc-600">$</span> {entry.cmd}
                  </div>
                  {entry.output && (
                    <div className="text-zinc-500 text-xs ml-2">{entry.output}</div>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Stones visualization */}
          <div className="w-48 space-y-2">
            {stones.map(stone => (
              <div 
                key={stone.name}
                className={`
                  p-3 rounded-lg border transition-all cursor-pointer
                  ${tendedStone === stone.name 
                    ? 'border-green-500 bg-green-500/10' 
                    : 'border-zinc-700 bg-zinc-800/50 hover:border-zinc-600'}
                `}
                onClick={() => runCommand(`tend ${stone.name}`)}
              >
                <div className="flex items-center gap-2">
                  <div className={`w-2 h-2 rounded-full ${
                    tendedStone === stone.name ? 'bg-green-400' : 'bg-zinc-600'
                  }`} />
                  <span className={`text-sm ${
                    tendedStone === stone.name ? 'text-green-400' : 'text-zinc-400'
                  }`}>
                    {stone.name}
                  </span>
                </div>
                {stone.offerings.length > 0 && (
                  <div className="text-zinc-600 text-xs mt-1 ml-4">
                    {stone.offerings.join(', ')}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Command buttons */}
        <div className="flex gap-2 mt-4 flex-wrap">
          {presetCommands.map((cmd, i) => (
            <button
              key={i}
              onClick={() => runCommand(cmd)}
              className="px-3 py-1 bg-zinc-800 border border-zinc-700 rounded text-xs text-zinc-400 hover:border-zinc-600 hover:text-zinc-300 transition-colors"
            >
              {cmd}
            </button>
          ))}
        </div>
      </div>

      {/* Key insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-lg">
        <p className="text-amber-200/70 text-sm text-center">
          Tending lasts 90 seconds, then fades. Long enough to work, short enough to not forget where you are.
        </p>
      </div>
    </div>
  );
}
