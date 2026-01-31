import React, { useState, useEffect, useRef } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Tempo Breathing',
  description: 'Firefly LED behavior: idle vs busy',
  category: 'Core Concepts',
  color: 'amber',
  order: 3
};


export default function TempoBreathing() {
  // Each firefly: { id, x, y, phase, progress, color }
  const [idleFireflies, setIdleFireflies] = useState([]);
  const [busyFireflies, setBusyFireflies] = useState([]);
  const nextId = useRef(0);
  const idleLastSpawn = useRef(0);
  const busyLastSpawn = useRef(0);

  // Timing constants (in ms)
  const IDLE = {
    fadeIn: 5000,
    peak: 1000,
    fadeOut: 5000,
    spawnInterval: 3000,
    maxConcurrent: 2,
  };

  const BUSY = {
    fadeIn: 1000,
    peak: 300,
    fadeOut: 1000,
    spawnInterval: 300,
    maxConcurrent: 4,
  };

  const colors = {
    warmWhite: [255, 180, 100],
    serviceBlue: [80, 140, 255],
    storageGreen: [80, 255, 80],
  };

  const pickColor = () => {
    const roll = Math.random();
    if (roll < 0.1) return colors.storageGreen;
    if (roll < 0.2) return colors.serviceBlue;
    return colors.warmWhite;
  };

  const getAvailablePositions = (fireflies) => {
    const occupied = new Set(fireflies.map(f => `${f.x},${f.y}`));
    const available = [];
    for (let y = 0; y < 5; y++) {
      for (let x = 0; x < 5; x++) {
        if (!occupied.has(`${x},${y}`)) {
          available.push({ x, y });
        }
      }
    }
    return available;
  };

  const easeInOut = (t) => t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;

  const getBrightness = (phase, progress) => {
    switch (phase) {
      case 'fadeIn': return easeInOut(progress);
      case 'peak': return 1;
      case 'fadeOut': return 1 - easeInOut(progress);
      default: return 0;
    }
  };

  // Main update loop
  useEffect(() => {
    const update = () => {
      const now = Date.now();

      // Update idle fireflies
      setIdleFireflies(prev => {
        let updated = prev.map(f => {
          const elapsed = now - f.startTime;
          let { phase } = f;
          let progress = 0;

          if (phase === 'fadeIn') {
            progress = Math.min(1, elapsed / IDLE.fadeIn);
            if (progress >= 1) return { ...f, phase: 'peak', progress: 0, startTime: now };
          } else if (phase === 'peak') {
            progress = Math.min(1, elapsed / IDLE.peak);
            if (progress >= 1) return { ...f, phase: 'fadeOut', progress: 0, startTime: now };
          } else if (phase === 'fadeOut') {
            progress = Math.min(1, elapsed / IDLE.fadeOut);
            if (progress >= 1) return { ...f, phase: 'dormant' };
          }

          return { ...f, progress };
        }).filter(f => f.phase !== 'dormant');

        // Spawn new if needed
        if (now - idleLastSpawn.current > IDLE.spawnInterval && updated.length < IDLE.maxConcurrent) {
          const available = getAvailablePositions(updated);
          if (available.length > 0) {
            const pos = available[Math.floor(Math.random() * available.length)];
            updated = [...updated, {
              id: nextId.current++,
              x: pos.x,
              y: pos.y,
              phase: 'fadeIn',
              progress: 0,
              startTime: now,
              color: pickColor(),
            }];
            idleLastSpawn.current = now;
          }
        }

        return updated;
      });

      // Update busy fireflies
      setBusyFireflies(prev => {
        let updated = prev.map(f => {
          const elapsed = now - f.startTime;
          let { phase } = f;
          let progress = 0;

          if (phase === 'fadeIn') {
            progress = Math.min(1, elapsed / BUSY.fadeIn);
            if (progress >= 1) return { ...f, phase: 'peak', progress: 0, startTime: now };
          } else if (phase === 'peak') {
            progress = Math.min(1, elapsed / BUSY.peak);
            if (progress >= 1) return { ...f, phase: 'fadeOut', progress: 0, startTime: now };
          } else if (phase === 'fadeOut') {
            progress = Math.min(1, elapsed / BUSY.fadeOut);
            if (progress >= 1) return { ...f, phase: 'dormant' };
          }

          return { ...f, progress };
        }).filter(f => f.phase !== 'dormant');

        // Spawn new if needed
        if (now - busyLastSpawn.current > BUSY.spawnInterval && updated.length < BUSY.maxConcurrent) {
          const available = getAvailablePositions(updated);
          if (available.length > 0) {
            const pos = available[Math.floor(Math.random() * available.length)];
            updated = [...updated, {
              id: nextId.current++,
              x: pos.x,
              y: pos.y,
              phase: 'fadeIn',
              progress: 0,
              startTime: now,
              color: pickColor(),
            }];
            busyLastSpawn.current = now;
          }
        }

        return updated;
      });
    };

    const timer = setInterval(update, 50);
    return () => clearInterval(timer);
  }, []);

  // Initial spawn
  useEffect(() => {
    const now = Date.now();
    setIdleFireflies([{
      id: nextId.current++,
      x: Math.floor(Math.random() * 5),
      y: Math.floor(Math.random() * 5),
      phase: 'fadeIn',
      progress: 0,
      startTime: now,
      color: colors.warmWhite,
    }]);
    setBusyFireflies([{
      id: nextId.current++,
      x: Math.floor(Math.random() * 5),
      y: Math.floor(Math.random() * 5),
      phase: 'fadeIn',
      progress: 0,
      startTime: now,
      color: colors.warmWhite,
    }]);
    idleLastSpawn.current = now;
    busyLastSpawn.current = now;
  }, []);

  const FireflyGrid = ({ fireflies, label }) => {
    const fireflyMap = new Map(fireflies.map(f => [`${f.x},${f.y}`, f]));

    return (
      <div className="flex flex-col items-center">
        <div className="grid grid-cols-5 gap-1 mb-4">
          {Array.from({ length: 25 }).map((_, i) => {
            const x = i % 5;
            const y = Math.floor(i / 5);
            const firefly = fireflyMap.get(`${x},${y}`);
            const brightness = firefly ? getBrightness(firefly.phase, firefly.progress) : 0;
            const color = firefly ? firefly.color : [63, 63, 70];

            return (
              <div
                key={i}
                className="w-4 h-4 rounded-sm"
                style={{
                  backgroundColor: firefly
                    ? `rgba(${color[0]}, ${color[1]}, ${color[2]}, ${brightness * 0.9 + 0.1})`
                    : 'rgba(63, 63, 70, 0.3)',
                  boxShadow: firefly && brightness > 0.3
                    ? `0 0 ${brightness * 10}px rgba(${color[0]}, ${color[1]}, ${color[2]}, ${brightness * 0.6})`
                    : 'none',
                  transition: 'box-shadow 0.1s ease'
                }}
              />
            );
          })}
        </div>
        <span className="text-zinc-500 text-xs">{label}</span>
      </div>
    );
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      <h2 className="text-zinc-400 text-lg mb-2 tracking-wide">TEMPO</h2>
      <p className="text-zinc-500 text-sm mb-12">the garden breathes with its workload</p>

      <div className="flex gap-24 items-start">
        {/* Idle state */}
        <div className="flex flex-col items-center">
          <div className="text-amber-200/60 text-sm mb-6 tracking-wide">IDLE</div>
          <FireflyGrid
            fireflies={idleFireflies}
            label="1-2 fireflies"
          />
          <div className="mt-6 space-y-2 text-center">
            <div className="text-zinc-600 text-xs">5s fade in</div>
            <div className="text-zinc-600 text-xs">1s peak</div>
            <div className="text-zinc-600 text-xs">5s fade out</div>
          </div>

          {/* Waveform representation */}
          <svg viewBox="0 0 120 40" className="w-32 mt-6 opacity-60">
            <path
              d="M 0 20 Q 30 5, 60 20 Q 90 35, 120 20"
              fill="none"
              stroke="#fbbf24"
              strokeWidth="2"
              opacity="0.5"
            />
            <text x="60" y="38" textAnchor="middle" fill="#71717a" fontSize="8">~11s cycle</text>
          </svg>
        </div>

        {/* Arrow */}
        <div className="flex flex-col items-center justify-center h-48">
          <svg width="60" height="24" viewBox="0 0 60 24" fill="none">
            <path d="M 0 12 L 50 12 M 40 6 L 50 12 L 40 18" stroke="#71717a" strokeWidth="1.5"/>
          </svg>
          <span className="text-zinc-600 text-xs mt-2">load increases</span>
        </div>

        {/* Busy state */}
        <div className="flex flex-col items-center">
          <div className="text-amber-400/80 text-sm mb-6 tracking-wide">BUSY</div>
          <FireflyGrid
            fireflies={busyFireflies}
            label="up to 4 concurrent"
          />
          <div className="mt-6 space-y-2 text-center">
            <div className="text-zinc-500 text-xs">1s fade in</div>
            <div className="text-zinc-500 text-xs">0.3s peak</div>
            <div className="text-zinc-500 text-xs">1s fade out</div>
          </div>

          {/* Waveform representation */}
          <svg viewBox="0 0 120 40" className="w-32 mt-6 opacity-60">
            <path
              d="M 0 20 Q 7 8, 15 20 Q 23 32, 30 20 Q 37 8, 45 20 Q 53 32, 60 20 Q 67 8, 75 20 Q 83 32, 90 20 Q 97 8, 105 20 Q 113 32, 120 20"
              fill="none"
              stroke="#fbbf24"
              strokeWidth="2"
              opacity="0.7"
            />
            <text x="60" y="38" textAnchor="middle" fill="#71717a" fontSize="8">~2.3s cycle</text>
          </svg>
        </div>
      </div>

      <p className="text-zinc-600 text-sm mt-12 max-w-md text-center">
        You don't check a dashboard. You notice the rhythm change.
      </p>
    </div>
  );
}
