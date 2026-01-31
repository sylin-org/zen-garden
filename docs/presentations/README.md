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

## The Diagrams

### Core Concepts
- **mDNS Discovery** — Stones finding each other
- **Node vs Stone** — The vocabulary philosophy  
- **Tempo Breathing** — Firefly idle vs busy
- **Price Breakdown** — The $192.50 reveal
- **Seed-Bank Migration** — File journey with app

### How Things Work
- **Discovery Cascade** — --at → env → cache → UDP
- **Ceremony Workflow** — Harvest, update, rollback
- **Connection String** — Abstract → concrete
- **Stone Health** — Thriving / withering / wilting
- **Cost Comparison** — 5-year cloud vs garden
- **Capability-Aware App** — Features light up

### Architecture
- **Symmetric vs Asymmetric** — Cloud uniformity vs diversity
- **Service Origins** — Planted / Adopted / Borrowed
- **AWS Bridge** — Same code, anywhere
- **Tending** — Context like `cd`
- **Graceful Degradation** — Stone dies, garden heals

### Problem → Insight
- **Configuration Explosion** — 246 lines → 1 command
- **Abstraction Tax** — 8 layers → 2 layers
- **Feedback Through Glass** — Dashboards vs ambient
- **Scale Theater** — Billion-user arch, 12 users
- **Knowledge Wall** — Buttons vs systems

---

## Recording Tips

1. Run the project, press `F11` for fullscreen
2. Use OBS or DaVinci Resolve's capture feature
3. Most diagrams auto-cycle — let them loop
4. Click stage dots to jump to specific points
5. Use `←` `→` to quickly switch between diagrams

---

## Troubleshooting

**"Node.js is not installed"**
- Download from https://nodejs.org/ (LTS version)
- Install and restart your terminal

**Dependencies fail to install**
- Run `npm install` manually
- Check your internet connection

**Port 5173 already in use**
- Close other Vite projects
- Or edit `vite.config.js` to change the port

---

## License

Part of the Zen Garden project. Use freely.
