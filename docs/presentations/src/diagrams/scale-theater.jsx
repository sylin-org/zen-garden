import React, { useState, useEffect } from 'react';

export default function ScaleTheater() {
  const [userCount, setUserCount] = useState(12);
  const [view, setView] = useState('theater');

  const cloudComponents = [
    { name: 'Load Balancer', cost: 20, icon: '⚖️', needed: 1000 },
    { name: 'Auto-scaling Group', cost: 0, icon: '📈', needed: 100 },
    { name: 'Multi-AZ RDS', cost: 150, icon: '🗄️', needed: 10000 },
    { name: 'ElastiCache Cluster', cost: 50, icon: '⚡', needed: 5000 },
    { name: 'CloudFront CDN', cost: 30, icon: '🌐', needed: 50000 },
    { name: 'Route 53', cost: 5, icon: '🧭', needed: 1 },
    { name: 'WAF', cost: 20, icon: '🛡️', needed: 1000 },
    { name: 'CloudWatch', cost: 15, icon: '📊', needed: 1 },
  ];

  const gardenComponents = [
    { name: 'stone-coral', desc: 'MongoDB + API', icon: '🪨' },
    { name: 'stone-amber', desc: 'Redis cache', icon: '🪨' },
  ];

  const totalCloudCost = cloudComponents.reduce((sum, c) => sum + c.cost, 0);

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">SCALE THEATER</h2>
      <p className="text-zinc-500 text-sm mb-6">architectures designed for users you don't have</p>

      {/* User count slider */}
      <div className="mb-6 text-center">
        <div className="text-zinc-400 text-sm mb-2">Your actual users:</div>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min="1"
            max="100"
            value={userCount}
            onChange={(e) => setUserCount(parseInt(e.target.value))}
            className="w-48"
          />
          <span className="text-amber-400 text-2xl font-light w-16">{userCount}</span>
        </div>
      </div>

      {/* Toggle */}
      <div className="flex gap-2 mb-8">
        <button
          onClick={() => setView('theater')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'theater'
              ? 'border-red-500 bg-red-500/10 text-red-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          "Production Ready"
        </button>
        <button
          onClick={() => setView('garden')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'garden'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Actually Needed
        </button>
      </div>

      <div className="flex gap-12 max-w-5xl w-full">
        
        {/* Cloud "production" architecture */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'theater' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">THE "BEST PRACTICES" STACK</div>
          
          <div className="bg-zinc-950 rounded-lg p-4 border border-zinc-800">
            <div className="grid grid-cols-2 gap-2">
              {cloudComponents.map((comp, i) => {
                const isOverkill = userCount < comp.needed;
                return (
                  <div 
                    key={i}
                    className={`
                      p-3 rounded border transition-all
                      ${isOverkill 
                        ? 'border-red-500/30 bg-red-500/5' 
                        : 'border-green-500/30 bg-green-500/5'}
                    `}
                  >
                    <div className="flex items-center gap-2 mb-1">
                      <span>{comp.icon}</span>
                      <span className={`text-xs ${isOverkill ? 'text-red-400' : 'text-green-400'}`}>
                        {comp.name}
                      </span>
                    </div>
                    <div className="flex justify-between text-xs">
                      <span className="text-zinc-600">
                        {isOverkill ? `Need ${comp.needed.toLocaleString()}+ users` : 'Reasonable'}
                      </span>
                      <span className="text-zinc-500">${comp.cost}/mo</span>
                    </div>
                  </div>
                );
              })}
            </div>

            <div className="mt-4 pt-4 border-t border-zinc-800 flex justify-between items-center">
              <div>
                <div className="text-zinc-500 text-xs">Monthly cost</div>
                <div className="text-red-400 text-xl font-light">${totalCloudCost}</div>
              </div>
              <div className="text-right">
                <div className="text-zinc-500 text-xs">Cost per user</div>
                <div className="text-red-400 text-xl font-light">
                  ${(totalCloudCost / userCount).toFixed(2)}
                </div>
              </div>
            </div>
          </div>

          {/* The absurdity */}
          <div className="mt-4 p-3 border border-red-500/30 rounded bg-red-500/5">
            <div className="text-red-400 text-sm mb-2">You're paying for:</div>
            <div className="text-zinc-500 text-xs">
              • Multi-region failover for {userCount} users<br/>
              • Auto-scaling that will never trigger<br/>
              • CDN for content nobody's requesting<br/>
              • Database redundancy for data that fits on a USB stick
            </div>
          </div>
        </div>

        {/* Garden reality */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'garden' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">WHAT {userCount} USERS ACTUALLY NEED</div>
          
          <div className="bg-zinc-950 rounded-lg p-4 border border-zinc-800">
            <div className="space-y-3">
              {gardenComponents.map((comp, i) => (
                <div 
                  key={i}
                  className="p-4 rounded border border-green-500/30 bg-green-500/5"
                >
                  <div className="flex items-center gap-2 mb-1">
                    <span>{comp.icon}</span>
                    <span className="text-green-400">{comp.name}</span>
                  </div>
                  <div className="text-zinc-500 text-sm">{comp.desc}</div>
                </div>
              ))}
            </div>

            {/* Capacity visualization */}
            <div className="mt-6 p-4 bg-zinc-800/50 rounded">
              <div className="text-zinc-500 text-xs mb-2">Actual capacity needed:</div>
              <div className="h-4 bg-zinc-700 rounded overflow-hidden">
                <div 
                  className="h-full bg-green-500 transition-all duration-500"
                  style={{ width: `${Math.min(userCount, 100)}%` }}
                />
              </div>
              <div className="flex justify-between text-xs mt-1">
                <span className="text-zinc-500">{userCount} users</span>
                <span className="text-zinc-500">Wyse 5070 can handle 1000+</span>
              </div>
            </div>

            <div className="mt-4 pt-4 border-t border-zinc-800 flex justify-between items-center">
              <div>
                <div className="text-zinc-500 text-xs">Monthly cost</div>
                <div className="text-green-400 text-xl font-light">~$8</div>
              </div>
              <div className="text-right">
                <div className="text-zinc-500 text-xs">Cost per user</div>
                <div className="text-green-400 text-xl font-light">
                  ${(8 / userCount).toFixed(2)}
                </div>
              </div>
            </div>
          </div>

          {/* The reality */}
          <div className="mt-4 p-3 border border-green-500/30 rounded bg-green-500/5">
            <div className="text-green-400 text-sm mb-2">What you get:</div>
            <div className="text-zinc-500 text-xs">
              • Enough capacity for 100x your users<br/>
              • Same reliability (it's just MongoDB)<br/>
              • Same functionality<br/>
              • And you can see it from your desk
            </div>
          </div>
        </div>
      </div>

      {/* The insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {view === 'theater'
            ? `You're paying $${(totalCloudCost / userCount).toFixed(2)}/user for infrastructure designed for millions. Your ${userCount} users don't need multi-region failover.`
            : `Right-sized infrastructure. A $35 thin client handles more concurrent users than most apps will ever see.`}
        </p>
      </div>
    </div>
  );
}
