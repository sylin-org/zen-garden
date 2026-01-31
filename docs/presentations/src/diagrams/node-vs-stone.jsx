import React from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Node vs Stone',
  description: 'The vocabulary philosophy',
  category: 'Core Concepts',
  color: 'amber',
  order: 2
};


export default function NodeVsStone() {
  return (
    <div className="w-full h-screen bg-zinc-900 flex items-center justify-center p-8">
      <div className="flex gap-16 max-w-4xl">
        
        {/* Node side */}
        <div className="flex-1 flex flex-col items-center">
          <h2 className="text-zinc-500 text-sm tracking-widest mb-8">NODE</h2>
          
          <div className="relative mb-8">
            {/* Generic node boxes - identical, interchangeable */}
            <div className="flex gap-3">
              {[0,1,2,3].map(i => (
                <div 
                  key={i}
                  className="w-12 h-12 rounded border border-zinc-600 bg-zinc-800 flex items-center justify-center"
                >
                  <span className="text-zinc-500 text-xs font-mono">n{i+1}</span>
                </div>
              ))}
            </div>
            {/* Replacement arrow */}
            <div className="absolute -bottom-12 left-1/2 -translate-x-1/2 flex flex-col items-center">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#71717a" strokeWidth="1.5">
                <path d="M12 5v14M5 12l7 7 7-7"/>
              </svg>
            </div>
          </div>
          
          <div className="h-8" />
          
          <div className="w-12 h-12 rounded border border-dashed border-zinc-600 bg-zinc-800/50 flex items-center justify-center mb-6">
            <span className="text-zinc-600 text-xs font-mono">n?</span>
          </div>
          
          <div className="text-center space-y-3 mt-4">
            <p className="text-zinc-400 text-sm">interchangeable</p>
            <p className="text-zinc-500 text-sm">abstract</p>
            <p className="text-zinc-600 text-sm">replaceable</p>
          </div>
        </div>

        {/* Divider */}
        <div className="w-px bg-zinc-800 self-stretch" />

        {/* Stone side */}
        <div className="flex-1 flex flex-col items-center">
          <h2 className="text-amber-400/80 text-sm tracking-widest mb-8">STONE</h2>
          
          <div className="flex gap-4 mb-8">
            {/* Each stone is unique, has character */}
            <div className="flex flex-col items-center gap-2">
              <div className="w-14 h-10 rounded bg-zinc-700 border border-zinc-500 relative">
                <div className="absolute top-1 right-1 w-2 h-2 rounded-full bg-green-400"/>
                <span className="absolute bottom-0 left-1 text-zinc-400 text-xs">WYSE</span>
              </div>
              <span className="text-amber-200/60 text-xs">coral</span>
            </div>
            
            <div className="flex flex-col items-center gap-2">
              <div className="w-14 h-10 rounded bg-zinc-700 border border-zinc-500 relative">
                <div className="absolute top-1 right-1 w-2 h-2 rounded-full bg-blue-400"/>
                <div className="absolute top-1 left-1 w-4 h-1 bg-zinc-600 rounded"/>
              </div>
              <span className="text-amber-200/60 text-xs">amber</span>
            </div>
            
            <div className="flex flex-col items-center gap-2">
              <div className="w-10 h-6 rounded bg-zinc-700 border border-zinc-500 relative">
                <div className="absolute top-1 right-1 w-1.5 h-1.5 rounded-full bg-blue-300"/>
              </div>
              <span className="text-amber-200/60 text-xs">leaf</span>
            </div>
          </div>
          
          <div className="flex items-center gap-2 my-6">
            <div className="w-14 h-10 rounded bg-zinc-700 border border-amber-400/30 relative">
              <div className="absolute -top-1 -right-1 w-3 h-3 rounded-full bg-amber-400 animate-pulse"/>
              <span className="absolute bottom-0 left-1 text-zinc-400 text-xs">3 yrs</span>
            </div>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#fbbf24" strokeWidth="1.5" className="opacity-50">
              <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/>
            </svg>
          </div>
          
          <div className="text-center space-y-3 mt-4">
            <p className="text-amber-200/80 text-sm">has weight</p>
            <p className="text-amber-200/60 text-sm">has history</p>
            <p className="text-amber-200/40 text-sm">remembered</p>
          </div>
        </div>
      </div>
    </div>
  );
}
