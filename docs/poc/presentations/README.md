# Zen Garden Diagrams

21 animated React diagrams for the Zen Garden video presentation.

## Quick Start

### Windows
```
Double-click run.bat
```

### Linux / macOS
```bash
chmod +x run.sh
./run.sh
```

That's it! The script will:
1. Check for Node.js (prompts to install if missing)
2. Install dependencies (first run only)
3. Start the dev server
4. Open your browser to the diagram menu

---

## Controls

| Key | Action |
|-----|--------|
| `M` | Toggle menu |
| `Escape` | Close menu |
| `←` `→` | Previous / Next diagram |
| `F11` | Fullscreen (for recording) |

---

## Adding New Diagrams

The loader **automatically discovers** all `.jsx` files in `src/diagrams/`.

### 1. Create a new file

```jsx
// src/diagrams/my-new-diagram.jsx

import React, { useState } from 'react';

// Metadata for the menu (optional but recommended)
export const metadata = {
  name: 'My New Diagram',           // Display name in menu
  description: 'What this shows',   // Subtitle in menu
  category: 'Core Concepts',        // Which section (see below)
  color: 'amber',                   // Menu accent color
  order: 10                         // Sort order within category
};

export default function MyNewDiagram() {
  return (
    <div className="w-full h-screen bg-zinc-900 flex items-center justify-center">
      <h1 className="text-white">Hello World</h1>
    </div>
  );
}
```

### 2. That's it!

Save the file, and it appears in the menu automatically. No imports to update, no config to change.

---

## Metadata Reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | No | Display name (defaults to filename) |
| `description` | string | No | Subtitle shown in menu |
| `category` | string | No | Menu section (defaults to "Other") |
| `color` | string | No | Accent color (defaults to "zinc") |
| `order` | number | No | Sort order within category (defaults to 999) |

### Categories

- `Core Concepts` — Fundamental ideas
- `How Things Work` — Technical explanations  
- `Architecture` — System design
- `Problem → Insight` — Before/after comparisons
- `Other` — Uncategorized (fallback)

### Colors

`amber` · `blue` · `purple` · `green` · `red` · `zinc`

---

## Design Language

For visual consistency, use these Tailwind classes:

| Element | Class |
|---------|-------|
| Background | `bg-zinc-900` |
| Primary accent | `text-amber-400`, `border-amber-500` |
| Success | `text-green-400` |
| Warning | `text-pink-400` |
| Error | `text-red-400` |
| Body text | `text-zinc-300` |
| Muted text | `text-zinc-500` |

### Common patterns

```jsx
// Full-screen container
<div className="w-full h-screen bg-zinc-900 flex flex-col items-center justify-center p-8">

// Title
<h2 className="text-zinc-400 text-lg mb-2 tracking-wide">TITLE HERE</h2>

// Subtitle
<p className="text-zinc-500 text-sm mb-8">subtitle here</p>

// Key insight box
<div className="mt-8 p-4 border border-zinc-800 rounded-lg max-w-xl">
  <p className="text-amber-200/70 text-sm text-center">
    The insight goes here.
  </p>
</div>

// Stage indicators (clickable dots)
<div className="flex gap-2 mt-6">
  {[0,1,2,3].map(i => (
    <button
      key={i}
      onClick={() => setStage(i)}
      className={`w-2 h-2 rounded-full transition-colors ${
        stage === i ? 'bg-amber-400' : 'bg-zinc-700 hover:bg-zinc-600'
      }`}
    />
  ))}
</div>

// Reset button
<button 
  onClick={() => setStage(0)}
  className="mt-4 text-zinc-700 text-xs hover:text-zinc-500 transition-colors"
>
  reset
</button>
```

---

## Recording Tips

1. Press `F11` for fullscreen
2. Use OBS or DaVinci Resolve to capture
3. Most diagrams auto-cycle — let them loop, pick best takes
4. Click stage dots to jump to specific points
5. Press `M` to hide menu before recording

---

## Troubleshooting

**"Node.js is not installed"**
- Download from https://nodejs.org/ (LTS version)
- Install and restart terminal

**Dependencies fail to install**
- Run `npm install` manually
- Check internet connection

**Port 5173 already in use**
- Close other Vite projects
- Or edit `vite.config.js` to change port

**New diagram not appearing**
- Check file is in `src/diagrams/`
- Check file ends in `.jsx`
- Check for JavaScript syntax errors in console
- Verify `export default function` exists

---

## License

Part of the Zen Garden project. Use freely.
