import React, { useState, useEffect } from 'react';

export default function PriceBreakdown() {
  const [visibleItems, setVisibleItems] = useState(0);
  
  useEffect(() => {
    const timer = setInterval(() => {
      setVisibleItems(v => v < 6 ? v + 1 : v);
    }, 600);
    return () => clearInterval(timer);
  }, []);

  const items = [
    { name: 'Wyse 5070', qty: 3, unit: 35, icon: '▮', desc: 'thin clients' },
    { name: 'Wyse dx0q', qty: 1, unit: 25, icon: '▮', desc: 'workstation' },
    { name: 'Kangaroo MD2B', qty: 1, unit: 40, icon: '▬', desc: 'pocket pc' },
    { name: 'RP2040-Matrix', qty: 3, unit: 7.50, icon: '◫', desc: 'fireflies' },
    { name: 'SanDisk 1TB', qty: 1, unit: 0, icon: '▭', desc: 'seed-bank' },
  ];

  const total = items.reduce((sum, item) => sum + (item.qty * item.unit), 0);
  const showTotal = visibleItems > items.length;

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-8 tracking-wide">TOTAL COST</h2>
      
      <div className="w-full max-w-lg space-y-4">
        {items.map((item, i) => (
          <div 
            key={item.name}
            className="flex items-center justify-between py-3 border-b border-zinc-800 transition-all duration-500"
            style={{
              opacity: visibleItems > i ? 1 : 0,
              transform: visibleItems > i ? 'translateX(0)' : 'translateX(-20px)'
            }}
          >
            <div className="flex items-center gap-4">
              <span className="text-amber-400/60 text-lg w-8">{item.icon}</span>
              <div>
                <div className="text-zinc-300 text-sm">
                  {item.qty > 1 && <span className="text-zinc-500">{item.qty}× </span>}
                  {item.name}
                </div>
                <div className="text-zinc-600 text-xs">{item.desc}</div>
              </div>
            </div>
            <div className="text-right">
              {item.unit === 0 ? (
                <span className="text-zinc-500 text-sm">drawer find</span>
              ) : (
                <>
                  <span className="text-amber-200/80 text-lg font-light">
                    ${(item.qty * item.unit).toFixed(2)}
                  </span>
                  {item.qty > 1 && (
                    <span className="text-zinc-600 text-xs block">
                      ${item.unit} each
                    </span>
                  )}
                </>
              )}
            </div>
          </div>
        ))}
        
        {/* Total line */}
        <div 
          className="flex items-center justify-between pt-6 mt-4 border-t-2 border-zinc-700 transition-all duration-700"
          style={{
            opacity: showTotal ? 1 : 0,
            transform: showTotal ? 'translateY(0)' : 'translateY(10px)'
          }}
        >
          <div className="text-zinc-400 tracking-wide">TOTAL</div>
          <div className="text-amber-400 text-3xl font-light">
            ${total.toFixed(2)}
          </div>
        </div>
      </div>
      
      {/* Context */}
      <div 
        className="mt-12 text-center transition-all duration-700 delay-300"
        style={{
          opacity: showTotal ? 1 : 0,
        }}
      >
        <p className="text-zinc-500 text-sm">
          5 stones · 1TB storage · distributed compute
        </p>
        <p className="text-zinc-600 text-xs mt-2">
          less than one month of a modest EC2 instance
        </p>
      </div>

      {/* Reset button (for recording multiple takes) */}
      <button 
        onClick={() => setVisibleItems(0)}
        className="mt-8 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
      >
        reset animation
      </button>
    </div>
  );
}
