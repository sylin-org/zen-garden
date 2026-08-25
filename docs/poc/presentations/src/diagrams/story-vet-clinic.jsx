import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Story: The Vet Clinic',
  description: 'A neighborhood clinic connects with its community',
  category: 'Stories',
  color: 'green',
  order: 2
};

export default function StoryVetClinic() {
  const [chapter, setChapter] = useState(0);
  const [autoPlay, setAutoPlay] = useState(true);

  useEffect(() => {
    if (!autoPlay) return;
    const durations = [5000, 5000, 5000, 5000, 5000, 5000, 6000, 7000];
    const timer = setTimeout(() => {
      setChapter(c => (c + 1) % 8);
    }, durations[chapter]);
    return () => clearTimeout(timer);
  }, [chapter, autoPlay]);

  const chapters = [
    {
      title: "The Clinic",
      subtitle: "Pinewood Animal Hospital",
      visual: "clinic",
      text: "Dr. Sarah Chen has been here for 22 years. She knows every pet by name. The clinic runs on paper forms, a fax machine, and love.",
    },
    {
      title: "The Wish",
      subtitle: "\"Wouldn't it be nice if...\"",
      visual: "wish",
      text: "A photo slideshow in the waiting room. Birthday reminders for pets. Maybe texts when vaccinations are due. But the quotes from vendors started at $300/month.",
    },
    {
      title: "The Discovery",
      subtitle: "Tommy's old gaming PC",
      visual: "discovery",
      text: "Her son upgraded last year. The old PC sat in the garage. \"It still works, Mom. It's actually pretty powerful.\"",
    },
    {
      title: "The Setup",
      subtitle: "stone-pinewood awakens",
      visual: "setup",
      text: "Tommy helped one Saturday afternoon. The PC found a home in the supply closet. A green light blinked steadily. The garden had begun.",
    },
    {
      title: "The Waiting Room",
      subtitle: "Patients become stars",
      visual: "waiting-room",
      text: "An old monitor on the wall. Photos of patients cycle through — dogs in bandanas, cats in cones of shame, the occasional rabbit. Mrs. Henderson cried when she saw Whiskers.",
    },
    {
      title: "The Reminders",
      subtitle: "The clinic remembers",
      visual: "reminders",
      text: "\"Hi! Biscuit is due for her rabies shot.\" Automated. Personal. The number of missed appointments dropped by half.",
    },
    {
      title: "The Gateway",
      subtitle: "pinewoodvet.care goes live",
      visual: "gateway",
      text: "Online appointment booking. Pet records for owners. Running from the supply closet. When clients ask about the website, Dr. Chen just smiles.",
    },
    {
      title: "The Moment",
      subtitle: "A Tuesday afternoon",
      visual: "moment",
      text: "A family comes in with a new puppy. On the screen behind them: photos of their old dog, Max, from years ago. \"You kept his pictures,\" the father says quietly. \"We keep everyone.\"",
    },
  ];

  const current = chapters[chapter];

  const Visual = ({ type }) => {
    switch (type) {
      case 'clinic':
        return (
          <div className="relative w-full h-48 bg-zinc-800/50 rounded-lg overflow-hidden flex items-center justify-center">
            <div className="text-center">
              <div className="text-5xl mb-3">🏥</div>
              <div className="flex justify-center gap-2 mb-2">
                <span className="text-2xl">🐕</span>
                <span className="text-2xl">🐈</span>
                <span className="text-2xl">🐇</span>
              </div>
              <div className="text-green-400/60 text-sm">Since 2003</div>
            </div>
          </div>
        );

      case 'wish':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="grid grid-cols-3 gap-4">
              {[
                { icon: '🖼️', label: 'Photo slideshow', cost: '$45/mo' },
                { icon: '📱', label: 'SMS reminders', cost: '$89/mo' },
                { icon: '📅', label: 'Online booking', cost: '$150/mo' },
              ].map((item, i) => (
                <div key={i} className="text-center">
                  <div className="w-16 h-16 mx-auto bg-zinc-800 rounded-lg flex items-center justify-center border border-dashed border-zinc-600">
                    <span className="text-2xl opacity-50">{item.icon}</span>
                  </div>
                  <div className="text-zinc-500 text-xs mt-2">{item.label}</div>
                  <div className="text-red-400/60 text-xs">{item.cost}</div>
                </div>
              ))}
            </div>
          </div>
        );

      case 'discovery':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="relative">
              <div className="w-32 h-32 bg-zinc-800 rounded-lg border-2 border-dashed border-zinc-700 flex items-center justify-center">
                <div className="text-center">
                  <div className="text-3xl mb-2">🖥️</div>
                  <div className="text-zinc-500 text-xs">GTX 1060 inside</div>
                  <div className="text-zinc-600 text-xs">"Still pretty powerful"</div>
                </div>
              </div>
              <div className="absolute -top-2 -right-2 text-green-400 animate-pulse">✨</div>
            </div>
          </div>
        );

      case 'setup':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex items-center gap-8">
              <div className="text-center">
                <div className="text-3xl">👨‍💻</div>
                <div className="text-zinc-500 text-xs mt-1">Tommy</div>
              </div>
              <div className="text-green-400">→</div>
              <div className="relative">
                <div className="w-24 h-24 bg-green-500/10 rounded-lg border-2 border-green-500/50 flex items-center justify-center">
                  <div className="text-center">
                    <div className="text-2xl">🪨</div>
                    <div className="text-green-400 text-xs mt-1">stone-pinewood</div>
                  </div>
                </div>
                <div className="absolute top-1 right-1 w-2 h-2 bg-green-400 rounded-full animate-pulse" />
              </div>
            </div>
          </div>
        );

      case 'waiting-room':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="relative">
              {/* Monitor frame */}
              <div className="w-48 h-32 bg-zinc-800 rounded-lg border-4 border-zinc-700 p-2">
                <div className="w-full h-full bg-zinc-900 rounded flex items-center justify-center overflow-hidden">
                  {/* Cycling photos */}
                  <div className="text-center animate-pulse">
                    <div className="text-4xl mb-1">🐕</div>
                    <div className="text-amber-400 text-xs">Biscuit</div>
                    <div className="text-zinc-500 text-xs">Good girl!</div>
                  </div>
                </div>
              </div>
              {/* Stand */}
              <div className="w-8 h-4 bg-zinc-700 mx-auto rounded-b" />
              
              {/* Reaction */}
              <div className="absolute -right-16 top-1/2 -translate-y-1/2 text-center">
                <div className="text-2xl">😢</div>
                <div className="text-zinc-500 text-xs">Mrs. Henderson</div>
              </div>
            </div>
          </div>
        );

      case 'reminders':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex items-center gap-6">
              {/* Stone */}
              <div className="w-16 h-16 bg-green-500/10 rounded-lg border border-green-500/50 flex items-center justify-center">
                <span className="text-xl">🪨</span>
              </div>
              
              {/* Arrow */}
              <div className="text-blue-400 animate-pulse">→</div>
              
              {/* Phone with message */}
              <div className="w-32 h-48 bg-zinc-800 rounded-2xl border-4 border-zinc-700 p-2">
                <div className="w-full h-full bg-zinc-900 rounded-xl p-2">
                  <div className="bg-green-500/20 rounded-lg p-2 text-xs">
                    <div className="text-green-400 mb-1">Pinewood Vet</div>
                    <div className="text-zinc-300">Hi! Biscuit is due for her rabies shot next week. Reply YES to book.</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        );

      case 'gateway':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex flex-col items-center gap-3">
              <div className="flex items-center gap-3">
                <span className="text-xl">🌍</span>
                <span className="text-zinc-400 text-sm">pinewoodvet.care</span>
              </div>
              
              <div className="text-purple-400 animate-pulse">↓</div>
              
              <div className="w-40 h-16 bg-purple-500/10 rounded-lg border border-purple-500/50 flex items-center justify-center gap-2">
                <span className="text-xl">🚪</span>
                <div className="text-center">
                  <div className="text-purple-400 text-sm">gateway</div>
                  <div className="text-zinc-500 text-xs">supply closet</div>
                </div>
              </div>
              
              <div className="text-zinc-600 text-xs mt-2">Online booking • Pet records • From the back room</div>
            </div>
          </div>
        );

      case 'moment':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="text-center">
              <div className="flex justify-center gap-4 mb-4">
                <div className="text-3xl">👨‍👩‍👧</div>
                <div className="text-3xl">🐕</div>
              </div>
              
              {/* The screen showing Max */}
              <div className="w-32 h-20 bg-zinc-800 rounded-lg border-2 border-amber-500/30 mx-auto mb-4 flex items-center justify-center">
                <div className="text-center">
                  <div className="text-2xl">🐕</div>
                  <div className="text-amber-400/60 text-xs">Max, 2018</div>
                </div>
              </div>
              
              <div className="text-amber-200/60 text-sm italic">"We keep everyone."</div>
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
      <h2 className="text-green-400 text-2xl font-light mb-1">{current.title}</h2>
      <p className="text-zinc-500 text-sm mb-8">{current.subtitle}</p>

      {/* Visual */}
      <div className="w-full max-w-lg mb-8">
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
              chapter === i ? 'bg-green-400 w-4' : 'bg-zinc-700 hover:bg-zinc-600'
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
