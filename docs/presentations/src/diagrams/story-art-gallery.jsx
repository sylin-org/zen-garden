import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Story: The Art Gallery',
  description: 'A small gallery finds its digital voice',
  category: 'Stories',
  color: 'amber',
  order: 1
};

export default function StoryArtGallery() {
  const [chapter, setChapter] = useState(0);
  const [autoPlay, setAutoPlay] = useState(true);

  useEffect(() => {
    if (!autoPlay) return;
    const durations = [5000, 5000, 5000, 5000, 5000, 6000, 6000, 7000];
    const timer = setTimeout(() => {
      setChapter(c => (c + 1) % 8);
    }, durations[chapter]);
    return () => clearTimeout(timer);
  }, [chapter, autoPlay]);

  const chapters = [
    {
      title: "The Gallery",
      subtitle: "Riverside Arts Collective",
      visual: "gallery",
      text: "A converted warehouse. Three artists share the space. They rotate exhibitions monthly. The art is beautiful. The technology... isn't.",
    },
    {
      title: "The Problem", 
      subtitle: "Square pegs, round holes",
      visual: "problem",
      text: "Gallery management software: $89/month. Digital signage service: $45/month. Guest book app: $15/month. None of them talk to each other. None of them feel like *theirs*.",
    },
    {
      title: "The Closet",
      subtitle: "Forgotten treasure",
      visual: "closet",
      text: "Behind the paint supplies: a 2015 Mac Mini. Donated years ago. \"We should do something with that someday.\"",
    },
    {
      title: "The First Stone",
      subtitle: "stone-riverside comes online",
      visual: "first-stone",
      text: "30 minutes later, it's running. The old Mac hums quietly in the corner. A single amber LED pulses slowly. The garden has its first stone.",
    },
    {
      title: "The Guest Book",
      subtitle: "First offering planted",
      visual: "guestbook",
      text: "An iPad at the entrance. Visitors sign in, leave notes. \"Loved the Hernandez exhibit!\" The data stays here. In this building. Theirs.",
    },
    {
      title: "The Gateway",
      subtitle: "A window to the world",
      visual: "gateway",
      text: "The artists wanted a website. Not Squarespace. Theirs. The gateway stone opens a door: riversidearts.gallery now serves from the back room.",
    },
    {
      title: "The Backup",
      subtitle: "Peace of mind",
      visual: "backup",
      text: "Every night at 3am, the seed-bank wakes up. Guest signatures, artist portfolios, sales records — all copied to the fireproof safe. Automatic. Silent.",
    },
    {
      title: "The Feeling",
      subtitle: "Six months later",
      visual: "feeling",
      text: "The gallery runs on gallery hardware. When the internet goes down, the guest book still works. When visitors ask \"what software is this?\" they smile. \"Ours.\"",
    },
  ];

  const current = chapters[chapter];

  const Visual = ({ type }) => {
    switch (type) {
      case 'gallery':
        return (
          <div className="relative w-full h-48 bg-zinc-800/50 rounded-lg overflow-hidden">
            {/* Gallery interior */}
            <div className="absolute inset-0 flex items-end justify-center gap-4 p-4">
              {/* Art frames */}
              {[...Array(3)].map((_, i) => (
                <div key={i} className="w-16 h-20 bg-amber-900/30 border-4 border-amber-800/50 rounded" />
              ))}
            </div>
            <div className="absolute bottom-2 left-4 text-amber-200/30 text-xs">Est. 2019</div>
          </div>
        );

      case 'problem':
        return (
          <div className="w-full h-48 flex items-center justify-center gap-4">
            {[
              { name: 'ArtBase Pro', cost: '$89/mo' },
              { name: 'SignageCloud', cost: '$45/mo' },
              { name: 'GuestBook.io', cost: '$15/mo' },
            ].map((service, i) => (
              <div key={i} className="flex flex-col items-center">
                <div className="w-16 h-16 bg-red-500/10 border border-red-500/30 rounded-lg flex items-center justify-center">
                  <span className="text-red-400 text-2xl">☁️</span>
                </div>
                <div className="text-zinc-400 text-xs mt-2">{service.name}</div>
                <div className="text-red-400 text-xs">{service.cost}</div>
              </div>
            ))}
            <div className="text-zinc-600 text-sm ml-4">= $149/month</div>
          </div>
        );

      case 'closet':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="relative">
              <div className="w-32 h-32 bg-zinc-800 rounded-lg border-2 border-dashed border-zinc-700 flex items-center justify-center">
                <div className="text-center">
                  <div className="text-3xl mb-2">🖥️</div>
                  <div className="text-zinc-500 text-xs">Mac Mini 2015</div>
                  <div className="text-zinc-600 text-xs">Dusty but functional</div>
                </div>
              </div>
              <div className="absolute -top-2 -right-2 text-amber-400 animate-pulse">✨</div>
            </div>
          </div>
        );

      case 'first-stone':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="relative">
              <div className="w-32 h-32 bg-green-500/10 rounded-lg border-2 border-green-500/50 flex items-center justify-center">
                <div className="text-center">
                  <div className="text-3xl mb-2">🪨</div>
                  <div className="text-green-400 text-sm">stone-riverside</div>
                  <div className="text-zinc-500 text-xs">online</div>
                </div>
              </div>
              {/* Pulsing LED */}
              <div className="absolute top-2 right-2 w-3 h-3 bg-amber-400 rounded-full animate-pulse" />
            </div>
          </div>
        );

      case 'guestbook':
        return (
          <div className="w-full h-48 flex items-center justify-center gap-8">
            {/* iPad */}
            <div className="w-24 h-32 bg-zinc-800 rounded-xl border-2 border-zinc-700 p-2">
              <div className="w-full h-full bg-zinc-900 rounded-lg flex flex-col items-center justify-center">
                <div className="text-2xl mb-1">📝</div>
                <div className="text-zinc-400 text-xs">Sign In</div>
              </div>
            </div>
            
            {/* Arrow */}
            <div className="text-green-400 animate-pulse">→</div>
            
            {/* Stone */}
            <div className="w-20 h-20 bg-green-500/10 rounded-lg border border-green-500/50 flex items-center justify-center">
              <div className="text-center">
                <div className="text-xl">🪨</div>
                <div className="text-green-400 text-xs">local</div>
              </div>
            </div>
          </div>
        );

      case 'gateway':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex flex-col items-center gap-4">
              {/* World */}
              <div className="flex items-center gap-4">
                <div className="text-2xl">🌍</div>
                <div className="text-zinc-500 text-sm">riversidearts.gallery</div>
              </div>
              
              {/* Arrow down */}
              <div className="text-purple-400 animate-pulse">↓</div>
              
              {/* Gateway stone */}
              <div className="w-32 h-16 bg-purple-500/10 rounded-lg border border-purple-500/50 flex items-center justify-center gap-2">
                <div className="text-xl">🚪</div>
                <div className="text-center">
                  <div className="text-purple-400 text-sm">gateway</div>
                  <div className="text-zinc-500 text-xs">stone-riverside</div>
                </div>
              </div>
            </div>
          </div>
        );

      case 'backup':
        return (
          <div className="w-full h-48 flex items-center justify-center gap-8">
            {/* Stone */}
            <div className="w-20 h-20 bg-green-500/10 rounded-lg border border-green-500/50 flex items-center justify-center">
              <div className="text-2xl">🪨</div>
            </div>
            
            {/* Arrow with time */}
            <div className="flex flex-col items-center">
              <div className="text-zinc-600 text-xs mb-1">3:00 AM</div>
              <div className="text-blue-400 animate-pulse">→→→</div>
              <div className="text-zinc-600 text-xs mt-1">automatic</div>
            </div>
            
            {/* Safe */}
            <div className="w-20 h-20 bg-blue-500/10 rounded-lg border border-blue-500/50 flex items-center justify-center">
              <div className="text-center">
                <div className="text-2xl">🔒</div>
                <div className="text-blue-400 text-xs">seed-bank</div>
              </div>
            </div>
          </div>
        );

      case 'feeling':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="text-center">
              <div className="text-5xl mb-4">🖼️</div>
              <div className="flex gap-2 justify-center mb-4">
                <div className="w-3 h-3 bg-green-400 rounded-full animate-pulse" />
                <div className="w-3 h-3 bg-amber-400 rounded-full animate-pulse" style={{ animationDelay: '0.3s' }} />
                <div className="w-3 h-3 bg-purple-400 rounded-full animate-pulse" style={{ animationDelay: '0.6s' }} />
              </div>
              <div className="text-amber-200/60 text-sm italic">"This is ours."</div>
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">
      {/* Chapter indicator */}
      <div className="text-zinc-600 text-xs tracking-wider mb-2">
        CHAPTER {chapter + 1} OF {chapters.length}
      </div>
      
      {/* Title */}
      <h2 className="text-amber-400 text-2xl font-light mb-1">{current.title}</h2>
      <p className="text-zinc-500 text-sm mb-8">{current.subtitle}</p>

      {/* Visual */}
      <div className="w-full max-w-md mb-8">
        <Visual type={current.visual} />
      </div>

      {/* Narrative text */}
      <div className="max-w-lg text-center">
        <p className="text-zinc-300 leading-relaxed">{current.text}</p>
      </div>

      {/* Chapter navigation */}
      <div className="flex gap-2 mt-8">
        {chapters.map((_, i) => (
          <button
            key={i}
            onClick={() => { setChapter(i); setAutoPlay(false); }}
            className={`w-2 h-2 rounded-full transition-all ${
              chapter === i ? 'bg-amber-400 w-4' : 'bg-zinc-700 hover:bg-zinc-600'
            }`}
          />
        ))}
      </div>

      {/* Controls */}
      <div className="flex gap-4 mt-4">
        <button
          onClick={() => setAutoPlay(!autoPlay)}
          className="text-zinc-600 text-xs hover:text-zinc-400 transition-colors"
        >
          {autoPlay ? '⏸ pause' : '▶ play'}
        </button>
        <button
          onClick={() => { setChapter(0); setAutoPlay(true); }}
          className="text-zinc-600 text-xs hover:text-zinc-400 transition-colors"
        >
          ↺ restart
        </button>
      </div>
    </div>
  );
}
