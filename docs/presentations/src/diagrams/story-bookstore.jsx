import React, { useState, useEffect } from 'react';

// Metadata for dynamic loading
export const metadata = {
  name: 'Story: The Bookstore',
  description: 'An independent bookstore builds its own catalog',
  category: 'Stories',
  color: 'purple',
  order: 3
};

export default function StoryBookstore() {
  const [chapter, setChapter] = useState(0);
  const [autoPlay, setAutoPlay] = useState(true);

  useEffect(() => {
    if (!autoPlay) return;
    const durations = [5000, 5000, 5000, 5000, 5500, 5500, 6000, 7000];
    const timer = setTimeout(() => {
      setChapter(c => (c + 1) % 8);
    }, durations[chapter]);
    return () => clearTimeout(timer);
  }, [chapter, autoPlay]);

  const chapters = [
    {
      title: "The Bookstore",
      subtitle: "Owl & Crescent Books",
      visual: "bookstore",
      text: "Three floors of used and new books. Philosophy upstairs, children's in the basement. The owners, James and Maria, know where everything is. The computer doesn't.",
    },
    {
      title: "The Question",
      subtitle: "\"Do you have...?\"",
      visual: "question",
      text: "Fifty times a day. \"Do you have...?\" And James walks the customer to the shelf himself, because the inventory system is a spreadsheet from 2016.",
    },
    {
      title: "The Alternative",
      subtitle: "What the vendors offered",
      visual: "vendors",
      text: "Modern inventory systems. Barcode scanners. Cloud sync. $200/month, plus setup fees, plus training. \"It's just not us,\" Maria said.",
    },
    {
      title: "The Laptop",
      subtitle: "Behind the register",
      visual: "laptop",
      text: "An old ThinkPad. James used it for accounting until the new one arrived. It sat under the counter, waiting for a purpose.",
    },
    {
      title: "The Catalog",
      subtitle: "stone-owl comes to life",
      visual: "catalog",
      text: "A weekend project. Scan a book's ISBN, add the shelf location. Aisle 3, second shelf from top. 4,000 books later, the question changed.",
    },
    {
      title: "The Terminal",
      subtitle: "\"Let me search that for you\"",
      visual: "terminal",
      text: "A tablet by the front door. Customers type a title, author, or topic. \"Aisle 2, philosophy section, eye level.\" James still walks them there. But now he doesn't have to.",
    },
    {
      title: "The Staff Picks",
      subtitle: "Every employee gets a voice",
      visual: "picks",
      text: "Each staff member curates their shelf. The website shows them all: \"Rosa's Mystery Corner,\" \"David's Sci-Fi Essentials.\" Regulars have favorites.",
    },
    {
      title: "The Legacy",
      subtitle: "What stays behind",
      visual: "legacy",
      text: "The catalog grows every day. When the store is quiet, James adds notes: \"First edition, water stain on page 40. Previous owner was a professor.\" The books remember their history.",
    },
  ];

  const current = chapters[chapter];

  const Visual = ({ type }) => {
    switch (type) {
      case 'bookstore':
        return (
          <div className="w-full h-48 bg-zinc-800/50 rounded-lg flex items-center justify-center">
            <div className="text-center">
              <div className="flex justify-center gap-1 mb-3">
                {/* Bookshelf visualization */}
                {[...Array(5)].map((_, i) => (
                  <div key={i} className="w-3 h-16 rounded-sm" style={{
                    backgroundColor: ['#8b5a2b', '#6b4423', '#a0522d', '#8b4513', '#654321'][i],
                    height: `${50 + Math.random() * 30}px`
                  }} />
                ))}
              </div>
              <div className="text-purple-400 text-lg">📚 Owl & Crescent</div>
              <div className="text-zinc-500 text-xs">Est. 1987</div>
            </div>
          </div>
        );

      case 'question':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex items-center gap-8">
              <div className="text-center">
                <div className="text-3xl mb-2">🧑</div>
                <div className="bg-zinc-800 rounded-lg p-2 text-sm text-zinc-300">
                  "Do you have<br/>Borges?"
                </div>
              </div>
              
              <div className="text-zinc-600">→</div>
              
              <div className="text-center">
                <div className="text-3xl mb-2">🚶</div>
                <div className="text-zinc-500 text-xs">*walks to<br/>third floor*</div>
              </div>
              
              <div className="text-zinc-600">→</div>
              
              <div className="text-center">
                <div className="text-3xl mb-2">📚</div>
                <div className="text-purple-400 text-xs">Found it!</div>
              </div>
            </div>
          </div>
        );

      case 'vendors':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="bg-zinc-800/50 rounded-lg p-6 border border-red-500/20">
              <div className="text-zinc-400 text-sm mb-4">Enterprise Inventory Solution™</div>
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-zinc-500">Monthly fee</span>
                  <span className="text-red-400">$200</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-zinc-500">Setup fee</span>
                  <span className="text-red-400">$500</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-zinc-500">Training</span>
                  <span className="text-red-400">$300</span>
                </div>
                <div className="flex justify-between pt-2 border-t border-zinc-700">
                  <span className="text-zinc-400">Year one</span>
                  <span className="text-red-400">$3,200</span>
                </div>
              </div>
              <div className="text-zinc-600 text-xs mt-4 italic">"It's just not us."</div>
            </div>
          </div>
        );

      case 'laptop':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="relative">
              {/* Counter */}
              <div className="w-64 h-8 bg-amber-900/30 rounded-t-lg" />
              
              {/* Laptop under counter */}
              <div className="w-64 h-20 bg-zinc-800 rounded-b-lg flex items-center justify-center border-t-2 border-amber-900/50">
                <div className="text-center">
                  <div className="text-2xl">💻</div>
                  <div className="text-zinc-500 text-xs">ThinkPad T460</div>
                  <div className="text-zinc-600 text-xs">Waiting...</div>
                </div>
              </div>
              
              <div className="absolute -top-2 right-4 text-purple-400 animate-pulse">✨</div>
            </div>
          </div>
        );

      case 'catalog':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex items-center gap-6">
              {/* Scanner */}
              <div className="text-center">
                <div className="text-3xl mb-1">📱</div>
                <div className="text-zinc-500 text-xs">scan ISBN</div>
              </div>
              
              <div className="text-purple-400">→</div>
              
              {/* Stone */}
              <div className="w-20 h-20 bg-purple-500/10 rounded-lg border border-purple-500/50 flex items-center justify-center">
                <div className="text-center">
                  <div className="text-xl">🪨</div>
                  <div className="text-purple-400 text-xs">stone-owl</div>
                </div>
              </div>
              
              <div className="text-purple-400">→</div>
              
              {/* Database growing */}
              <div className="text-center">
                <div className="text-3xl mb-1">📚</div>
                <div className="text-purple-400 text-sm">4,000 books</div>
                <div className="text-zinc-500 text-xs">and growing</div>
              </div>
            </div>
          </div>
        );

      case 'terminal':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="flex items-center gap-8">
              {/* Customer at tablet */}
              <div className="text-center">
                <div className="text-2xl mb-2">🧑</div>
                <div className="w-24 h-16 bg-zinc-800 rounded-lg border-2 border-zinc-700 flex items-center justify-center">
                  <div className="text-xs text-zinc-400">
                    <div className="text-purple-400">🔍 "Borges"</div>
                  </div>
                </div>
              </div>
              
              {/* Result */}
              <div className="bg-purple-500/10 rounded-lg p-4 border border-purple-500/30">
                <div className="text-purple-400 text-sm mb-1">Found: 3 results</div>
                <div className="text-zinc-300 text-xs">
                  📍 Aisle 2, Philosophy<br/>
                  &nbsp;&nbsp;&nbsp;Eye level, left side
                </div>
              </div>
            </div>
          </div>
        );

      case 'picks':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="grid grid-cols-3 gap-4">
              {[
                { name: 'Rosa', shelf: 'Mystery Corner', emoji: '🔍' },
                { name: 'David', shelf: 'Sci-Fi Essentials', emoji: '🚀' },
                { name: 'James', shelf: 'Philosophy Deep Cuts', emoji: '🤔' },
              ].map((staff, i) => (
                <div key={i} className="text-center">
                  <div className="w-16 h-20 bg-purple-500/10 rounded-lg border border-purple-500/30 mx-auto flex items-center justify-center">
                    <span className="text-2xl">{staff.emoji}</span>
                  </div>
                  <div className="text-zinc-300 text-sm mt-2">{staff.name}'s</div>
                  <div className="text-purple-400 text-xs">{staff.shelf}</div>
                </div>
              ))}
            </div>
          </div>
        );

      case 'legacy':
        return (
          <div className="w-full h-48 flex items-center justify-center">
            <div className="text-center">
              {/* Book with notes */}
              <div className="w-32 h-40 bg-amber-900/30 rounded-r-lg rounded-l-sm mx-auto mb-4 flex items-center justify-center border-l-4 border-amber-800">
                <div className="text-xs text-zinc-400 p-2">
                  <div className="text-amber-400 mb-1">📖 Ficciones</div>
                  <div className="text-zinc-500 italic text-xs">
                    "First edition.<br/>
                    Water stain, pg 40.<br/>
                    Prev. owner:<br/>
                    Prof. at State U."
                  </div>
                </div>
              </div>
              
              <div className="text-purple-200/60 text-sm italic">"The books remember their history."</div>
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
      <h2 className="text-purple-400 text-2xl font-light mb-1">{current.title}</h2>
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
              chapter === i ? 'bg-purple-400 w-4' : 'bg-zinc-700 hover:bg-zinc-600'
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
