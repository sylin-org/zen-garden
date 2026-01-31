import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Service Origins',
  description: 'Planted / Adopted / Borrowed services',
  category: 'Architecture',
  color: 'purple',
  order: 2
};


export default function ServiceOrigins() {
  const [activeOrigin, setActiveOrigin] = useState('planted');

  const origins = {
    planted: {
      color: 'green',
      title: 'PLANTED',
      subtitle: 'You asked for it',
      description: 'You ran a command. The garden created it. Full lifecycle control.',
      command: 'garden-rake offer mongodb',
      example: 'MongoDB container created, configured, health-checked',
      icon: '🌱',
      features: ['Created by you', 'Full control', 'Can uproot', 'Versioned updates'],
    },
    adopted: {
      color: 'amber',
      title: 'ADOPTED',
      subtitle: 'Already running, now recognized',
      description: 'That Docker container you started manually? The garden found it.',
      command: 'garden-rake adopt my-postgres',
      example: 'Existing postgres container joins the garden',
      icon: '🤝',
      features: ['Was already running', 'Garden discovered it', 'Now managed', 'Can be released'],
    },
    borrowed: {
      color: 'blue',
      title: 'BORROWED',
      subtitle: 'External, but known',
      description: "Your company's production database. Not here, but the garden knows how to find it.",
      command: 'garden-rake borrow prod-db --at db.company.com:5432',
      example: 'External PostgreSQL added to service discovery',
      icon: '🔗',
      features: ['Lives elsewhere', 'Garden tracks it', 'Apps can find it', 'No lifecycle control'],
    },
  };

  const current = origins[activeOrigin];
  
  const colorClasses = {
    green: {
      border: 'border-green-500',
      bg: 'bg-green-500/10',
      text: 'text-green-400',
      dim: 'text-green-400/60',
    },
    amber: {
      border: 'border-amber-500',
      bg: 'bg-amber-500/10',
      text: 'text-amber-400',
      dim: 'text-amber-400/60',
    },
    blue: {
      border: 'border-blue-500',
      bg: 'bg-blue-500/10',
      text: 'text-blue-400',
      dim: 'text-blue-400/60',
    },
  };
  
  const c = colorClasses[current.color];

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">SERVICE ORIGINS</h2>
      <p className="text-zinc-500 text-sm mb-8">three ways a service joins the garden</p>

      {/* Origin selector */}
      <div className="flex gap-4 mb-8">
        {Object.entries(origins).map(([key, origin]) => (
          <button
            key={key}
            onClick={() => setActiveOrigin(key)}
            className={`
              px-4 py-2 rounded-lg border-2 transition-all duration-300
              ${activeOrigin === key 
                ? `${colorClasses[origin.color].border} ${colorClasses[origin.color].bg}` 
                : 'border-zinc-700 hover:border-zinc-600'}
            `}
          >
            <div className="text-2xl mb-1">{origin.icon}</div>
            <div className={`text-sm font-medium ${
              activeOrigin === key ? colorClasses[origin.color].text : 'text-zinc-400'
            }`}>
              {origin.title}
            </div>
          </button>
        ))}
      </div>

      {/* Main content */}
      <div className={`
        max-w-2xl w-full p-6 rounded-xl border-2 transition-all duration-500
        ${c.border} ${c.bg}
      `}>
        <div className="flex items-start gap-4 mb-6">
          <div className="text-4xl">{current.icon}</div>
          <div>
            <h3 className={`text-xl font-medium ${c.text}`}>{current.title}</h3>
            <p className="text-zinc-400 text-sm">{current.subtitle}</p>
          </div>
        </div>

        <p className="text-zinc-300 mb-6">{current.description}</p>

        {/* Command example */}
        <div className="bg-zinc-900 rounded-lg p-4 mb-6">
          <div className="text-zinc-500 text-xs mb-2">COMMAND</div>
          <code className={`text-sm ${c.text}`}>$ {current.command}</code>
          <div className="text-zinc-500 text-xs mt-3 italic">{current.example}</div>
        </div>

        {/* Features */}
        <div className="grid grid-cols-2 gap-2">
          {current.features.map((feature, i) => (
            <div key={i} className="flex items-center gap-2">
              <div className={`w-1.5 h-1.5 rounded-full ${c.text.replace('text-', 'bg-')}`} />
              <span className="text-zinc-400 text-sm">{feature}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Visual representation */}
      <div className="mt-8 flex items-center gap-8">
        {/* Garden boundary */}
        <div className="relative">
          <div className="border-2 border-dashed border-zinc-700 rounded-xl p-6 w-64">
            <div className="text-zinc-600 text-xs mb-4 text-center">THE GARDEN</div>
            
            <div className="flex justify-center gap-3">
              {/* Planted service */}
              <div className={`
                w-16 h-16 rounded-lg border-2 flex items-center justify-center transition-all
                ${activeOrigin === 'planted' 
                  ? 'border-green-500 bg-green-500/20' 
                  : 'border-zinc-700 bg-zinc-800'}
              `}>
                <span className="text-xl">🌱</span>
              </div>
              
              {/* Adopted service */}
              <div className={`
                w-16 h-16 rounded-lg border-2 flex items-center justify-center transition-all
                ${activeOrigin === 'adopted' 
                  ? 'border-amber-500 bg-amber-500/20' 
                  : 'border-zinc-700 bg-zinc-800'}
              `}>
                <span className="text-xl">🤝</span>
              </div>
            </div>
          </div>
          
          {/* Borrowed - outside but connected */}
          <div className={`
            absolute -right-24 top-1/2 -translate-y-1/2
            w-16 h-16 rounded-lg border-2 flex items-center justify-center transition-all
            ${activeOrigin === 'borrowed' 
              ? 'border-blue-500 bg-blue-500/20' 
              : 'border-zinc-700 bg-zinc-800'}
          `}>
            <span className="text-xl">🔗</span>
          </div>
          
          {/* Connection line for borrowed */}
          <div className={`
            absolute right-0 top-1/2 w-8 border-t-2 border-dashed transition-colors
            ${activeOrigin === 'borrowed' ? 'border-blue-500' : 'border-zinc-700'}
          `} />
        </div>
      </div>

      {/* Key insight */}
      <div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-lg">
        <p className="text-amber-200/70 text-sm text-center">
          Not everything needs to be containerized. Not everything needs to be local.
          The garden meets your services where they are.
        </p>
      </div>
    </div>
  );
}
