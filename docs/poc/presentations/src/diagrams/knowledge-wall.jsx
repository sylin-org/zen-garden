import React, { useState } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Knowledge Wall',
  description: 'Learning AWS buttons vs learning systems',
  category: 'Problem → Insight',
  color: 'green',
  order: 5
};


export default function KnowledgeWall() {
  const [view, setView] = useState('cloud');

  const cloudSkills = [
    { name: 'AWS Console navigation', transferable: false },
    { name: 'IAM policy syntax', transferable: false },
    { name: 'CloudFormation YAML', transferable: false },
    { name: 'ECS task definitions', transferable: false },
    { name: 'VPC networking wizards', transferable: false },
    { name: 'S3 bucket policies', transferable: false },
    { name: 'CloudWatch query syntax', transferable: false },
    { name: 'Cost Explorer analysis', transferable: false },
  ];

  const gardenSkills = [
    { name: 'How containers actually work', transferable: true },
    { name: 'mDNS service discovery', transferable: true },
    { name: 'Volume mounting & persistence', transferable: true },
    { name: 'Health checks & probes', transferable: true },
    { name: 'Network topology', transferable: true },
    { name: 'Backup & restore patterns', transferable: true },
    { name: 'Resource constraints', transferable: true },
    { name: 'Debugging running systems', transferable: true },
  ];

  const SkillBadge = ({ skill, side }) => (
    <div className={`
      px-3 py-2 rounded border text-sm flex items-center justify-between
      ${side === 'cloud' 
        ? 'border-zinc-700 bg-zinc-800/50' 
        : skill.transferable 
          ? 'border-green-500/30 bg-green-500/5'
          : 'border-zinc-700 bg-zinc-800/50'}
    `}>
      <span className={side === 'cloud' ? 'text-zinc-400' : 'text-zinc-300'}>
        {skill.name}
      </span>
      {side === 'cloud' && (
        <span className="text-red-400 text-xs">AWS only</span>
      )}
      {side === 'garden' && skill.transferable && (
        <span className="text-green-400 text-xs">universal</span>
      )}
    </div>
  );

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">THE KNOWLEDGE WALL</h2>
      <p className="text-zinc-500 text-sm mb-6">learning platforms vs learning systems</p>

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
          Cloud Path
        </button>
        <button
          onClick={() => setView('garden')}
          className={`px-4 py-2 rounded-lg border transition-all ${
            view === 'garden'
              ? 'border-green-500 bg-green-500/10 text-green-400'
              : 'border-zinc-700 text-zinc-500 hover:border-zinc-600'
          }`}
        >
          Garden Path
        </button>
      </div>

      <div className="flex gap-12 max-w-5xl w-full">
        
        {/* Cloud learning */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'cloud' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">WHAT YOU LEARN</div>
          
          <div className="bg-zinc-950 rounded-lg p-4 border border-zinc-800">
            <div className="space-y-2">
              {cloudSkills.map((skill, i) => (
                <SkillBadge key={i} skill={skill} side="cloud" />
              ))}
            </div>
          </div>

          {/* The problem */}
          <div className="mt-4 p-4 border border-red-500/30 rounded bg-red-500/5">
            <div className="text-red-400 text-sm mb-2">The trap:</div>
            <div className="text-zinc-500 text-xs space-y-1">
              <p>You learned to click buttons in a specific UI.</p>
              <p>You learned a vendor's DSL for describing infrastructure.</p>
              <p>You learned to read their billing reports.</p>
              <p className="pt-2 text-zinc-400 italic">
                Switch to Azure? Start over. Switch to bare metal? Completely lost.
              </p>
            </div>
          </div>

          {/* Where knowledge goes */}
          <div className="mt-4 flex justify-center">
            <div className="flex flex-col items-center">
              <div className="text-zinc-500 text-xs mb-2">Knowledge transfer to:</div>
              <div className="flex gap-2">
                <span className="px-2 py-1 bg-zinc-800 rounded text-xs text-zinc-500">Azure ❌</span>
                <span className="px-2 py-1 bg-zinc-800 rounded text-xs text-zinc-500">GCP ❌</span>
                <span className="px-2 py-1 bg-zinc-800 rounded text-xs text-zinc-500">On-prem ❌</span>
              </div>
            </div>
          </div>
        </div>

        {/* Garden learning */}
        <div className={`flex-1 transition-opacity duration-500 ${view === 'garden' ? 'opacity-100' : 'opacity-30'}`}>
          <div className="text-zinc-500 text-xs tracking-wider mb-3">WHAT YOU LEARN</div>
          
          <div className="bg-zinc-950 rounded-lg p-4 border border-zinc-800">
            <div className="space-y-2">
              {gardenSkills.map((skill, i) => (
                <SkillBadge key={i} skill={skill} side="garden" />
              ))}
            </div>
          </div>

          {/* The benefit */}
          <div className="mt-4 p-4 border border-green-500/30 rounded bg-green-500/5">
            <div className="text-green-400 text-sm mb-2">The difference:</div>
            <div className="text-zinc-500 text-xs space-y-1">
              <p>You learned how containers actually start and stop.</p>
              <p>You learned how services find each other on a network.</p>
              <p>You learned what happens when hardware fails.</p>
              <p className="pt-2 text-zinc-400 italic">
                This knowledge works everywhere. Forever.
              </p>
            </div>
          </div>

          {/* Where knowledge goes */}
          <div className="mt-4 flex justify-center">
            <div className="flex flex-col items-center">
              <div className="text-zinc-500 text-xs mb-2">Knowledge transfer to:</div>
              <div className="flex gap-2">
                <span className="px-2 py-1 bg-green-500/20 rounded text-xs text-green-400">Any cloud ✓</span>
                <span className="px-2 py-1 bg-green-500/20 rounded text-xs text-green-400">On-prem ✓</span>
                <span className="px-2 py-1 bg-green-500/20 rounded text-xs text-green-400">Your career ✓</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* The scene */}
      <div className="mt-8 max-w-2xl">
        <div className="p-4 border border-zinc-800 rounded-lg">
          <div className="text-zinc-400 text-sm mb-4 text-center">
            {view === 'cloud' ? 'The interview:' : 'The other interview:'}
          </div>
          
          {view === 'cloud' ? (
            <div className="space-y-3 text-sm">
              <div className="text-zinc-500">"How do services discover each other?"</div>
              <div className="text-zinc-400 italic">"We use Route 53 with... um, you set up a hosted zone and then there's records..."</div>
              <div className="text-zinc-500">"No, I mean fundamentally. How does discovery work?"</div>
              <div className="text-red-400 italic">"..."</div>
            </div>
          ) : (
            <div className="space-y-3 text-sm">
              <div className="text-zinc-500">"How do services discover each other?"</div>
              <div className="text-green-400 italic">"Multicast DNS. Each service broadcasts a TXT record with its capabilities. Clients listen for announcements. It's the same protocol your phone uses to find printers."</div>
              <div className="text-zinc-500">"And if you were on AWS?"</div>
              <div className="text-green-400 italic">"Same concept, different implementation. Route 53 service discovery, or a service mesh. The pattern's the same."</div>
            </div>
          )}
        </div>
      </div>

      {/* The insight */}
      <div className="mt-6 p-4 border border-zinc-800 rounded-lg max-w-xl">
        <p className="text-amber-200/70 text-sm text-center">
          {view === 'cloud'
            ? "You're not learning distributed systems. You're learning AWS. Those are very different things."
            : "Understanding beats memorizing. The garden teaches concepts, not consoles."}
        </p>
      </div>
    </div>
  );
}
