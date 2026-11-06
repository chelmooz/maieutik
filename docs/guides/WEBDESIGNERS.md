# 🎨 **GUIDE WEBDESIGNERS - MAIEUTIK UX/UI**
**Version:** 2.0 | **Date:** 06 Nov 2026 | **Pour:** Équipe Webdesign/UX

---

## 🎯 **1. DEUX FRONTS EXISTANTS**

| Frontend | Techno | Statut | Dossier | Build |
|----------|--------|--------|---------|-------|
| **maieutik-web** | React 18 + TypeScript + Vite | **Principal** | `maieutik-web/` | `npm run build` |
| **maieutik-ui** | Rust + eframe/egui | Expérimental (`gui` feature) | `maieutik-ui/` | `cargo build --features cpp-modules,gui` |

> **Recommandation :** Travailler sur **maieutik-web** (React) pour la production.

---

## ⚛️ **2. MAIEUTIK-WEB (REACT) - STRUCTURE**

```
maieutik-web/
├── public/                    # Assets statiques
├── src/
│   ├── api/
│   │   └── client.ts          # Axios (baseURL: VITE_API_URL || http://localhost:3000)
│   ├── hooks/
│   │   └── useChat.ts         # Hook POST /chat
│   ├── pages/
│   │   ├── Chat.tsx           # Interface chat
│   │   └── FileList.tsx       # Liste fichiers indexés
│   ├── store/
│   │   └── useFiles.ts        # Zustand store (GET /api/files)
│   ├── App.tsx                # Router + layout
│   ├── main.tsx               # Entry point
│   └── style.css              # Styles globaux + variables CSS
├── package.json
├── vite.config.ts
├── tsconfig.json
└── playwright.config.ts       # Tests E2E
```

---

### **2.1. Composants Principaux**

#### **Chat.tsx** (Simplifié)
```tsx
// src/pages/Chat.tsx
import { useState, useCallback, FormEvent } from 'react';
import { useChat } from '../hooks/useChat';

interface Message { id: number; role: 'user' | 'assistant'; content: string; }

export function Chat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const { sendMessage, isLoading } = useChat();

  const handleSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;
    const userMsg: Message = { id: Date.now(), role: 'user', content: input };
    setMessages(p => [...p, userMsg]);
    setInput('');
    try {
      const resp = await sendMessage(input);
      setMessages(p => [...p, { id: Date.now(), role: 'assistant', content: resp?.message?.content || 'Erreur' }]);
    } catch (err) { console.error('Chat error:', err); }
  }, [input, isLoading, sendMessage]);

  return (
    <div className="chat-container">
      <div className="messages">
        {messages.map(m => (
          <div key={m.id} className={`message ${m.role}`}>
            <div className="content">{m.content}</div>
          </div>
        ))}
      </div>
      <form onSubmit={handleSubmit} className="chat-form">
        <input value={input} onChange={e => setInput(e.target.value)} placeholder="Votre question..." disabled={isLoading} />
        <button type="submit" disabled={isLoading || !input.trim()}>{isLoading ? '...' : 'Envoyer'}</button>
      </form>
    </div>
  );
}
```

#### **useChat.ts**
```ts
// src/hooks/useChat.ts
import { useState, useCallback } from 'react';
import axios from 'axios';

interface ChatMsg { role: 'user' | 'assistant'; content: string; }
interface ChatResp { message: ChatMsg; }

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:3000';

export function useChat() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const sendMessage = useCallback(async (msg: string): Promise<ChatResp | null> => {
    setLoading(true); setError(null);
    try {
      const r = await axios.post<ChatResp>(`${API_BASE}/chat`, { messages: [{ role: 'user', content: msg }] });
      return r.data;
    } catch (e) { const err = e instanceof Error ? e : new Error('Network error'); setError(err); throw err; }
    finally { setLoading(false); }
  }, []);

  return { sendMessage, isLoading: loading, error };
}
```

#### **useFiles.ts** (Zustand)
```ts
// src/store/useFiles.ts
import { create } from 'zustand';
import { fetchFiles } from '../api/client';

interface FilesState { files: string[]; loading: boolean; fetch: () => Promise<void>; }

export const useFiles = create<FilesState>((set) => ({
  files: [], loading: false,
  fetch: async () => { set({ loading: true }); const { data } = await fetchFiles(); set({ files: data, loading: false }); }
}));
```

> **Note API :** `fetchFiles` appelle `GET /api/files`. `searchFiles` appelle `POST /api/search` mais l'API réelle utilise `GET /api/search?query=`.

---

### **2.2. App.tsx (Router)**

```tsx
// src/App.tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Chat } from './pages/Chat';
import { FileList } from './pages/FileList';

export function App() {
  return (
    <BrowserRouter>
      <div className="app">
        <header className="app-header">
          <h1>Maieutik</h1>
          <nav><a href="/">Chat</a> | <a href="/files">Fichiers</a></nav>
        </header>
        <main className="container">
          <Routes>
            <Route path="/" element={<Chat />} />
            <Route path="/files" element={<FileList />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
```

---

## 🦀 **3. MAIEUTIK-UI (RUST/EGUI) - APERÇU**

```
maieutik-ui/
├── src/
│   ├── main.rs          # eframe entry point
│   ├── app.rs           # App state & UI loop
│   ├── ports.rs         # Traits agents (Odysseus, Laya, etc.)
│   ├── odysseus.rs      # Probe Odysseus (MCP client only)
│   ├── laya.rs          # Adaptateur Laya (MCP stdio)
│   ├── mcp_client.rs    # Client MCP JSON-RPC (framing 4 bytes LE)
│   └── lib.rs
├── contracts/
│   └── odysseus-bridge.schema.json
└── Cargo.toml           # feature "gui" = eframe/egui
```

> **Build :** `cargo build --features cpp-modules,gui`
> **Architecture :** Ports/Adapters (Hexagonal) — UI appelle traits, implémentations interchangeables.

---

## 🎨 **4. DESIGN SYSTEM (maieutik-web)**

### **4.1. Variables CSS (`src/style.css`)**

```css
:root {
  --primary: #6c5ce7;
  --primary-hover: #5649c0;
  --secondary: #a29bfe;
  --bg: #f8f9fa;
  --text: #2d3436;
  --text-muted: #636e72;
  --border: #dee2e6;
  --success: #2ecc71;
  --error: #e74c3c;
  --shadow: 0 2px 10px rgba(0,0,0,.1);
  --radius: 8px;
  --transition: all .3s ease;
}

/* Dark mode */
[data-theme="dark"] {
  --primary: #bb86fc; --primary-hover: #9a67ea; --secondary: #03dac6;
  --bg: #121212; --text: #e0e0e0; --text-muted: #a0a0a0; --border: #333;
  --shadow: 0 2px 10px rgba(0,0,0,.3);
}
```

### **4.2. Composants de Base**

```css
/* Layout */
.container { max-width: 1200px; margin: 0 auto; padding: 0 20px; }
.flex { display: flex; }
.flex-col { flex-direction: column; }
.gap-1 { gap: .5rem; }
.gap-2 { gap: 1rem; }

/* Inputs */
.input {
  padding: .75rem 1rem; border: 1px solid var(--border); border-radius: var(--radius);
  font-size: 1rem; background: var(--bg); color: var(--text);
}
.input:focus { outline: none; border-color: var(--primary); box-shadow: 0 0 0 2px rgba(108,92,231,.2); }

/* Buttons */
.btn { padding: .75rem 1.5rem; border: none; border-radius: var(--radius); cursor: pointer; transition: var(--transition); }
.btn-primary { background: var(--primary); color: white; }
.btn-primary:hover { background: var(--primary-hover); }
.btn:disabled { background: #ccc; cursor: not-allowed; }

/* Chat */
.chat-container { display: flex; flex-direction: column; height: 80vh; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; background: white; }
.messages { flex: 1; padding: 1.5rem; overflow-y: auto; }
.message { padding: .75rem 1rem; margin-bottom: 1rem; border-radius: var(--radius); max-width: 80%; }
.message.user { background: var(--primary); color: white; align-self: flex-end; }
.message.assistant { background: #f0f0f0; align-self: flex-start; }
[data-theme="dark"] .message.assistant { background: #2d2d2d; }
```

### **4.3. Theme Toggle**

```tsx
// src/components/ThemeToggle.tsx
import { useState, useEffect } from 'react';

export function ThemeToggle() {
  const [theme, setTheme] = useState<'light'|'dark'>('light');
  useEffect(() => {
    const saved = localStorage.getItem('theme') as 'light'|'dark'|null;
    if (saved) setTheme(saved);
    else if (window.matchMedia('(prefers-color-scheme: dark)').matches) setTheme('dark');
  }, []);
  useEffect(() => { document.documentElement.setAttribute('data-theme', theme); localStorage.setItem('theme', theme); }, [theme]);
  return <button onClick={() => setTheme(t => t==='light'?'dark':'light')} className="theme-toggle" aria-label="Toggle theme">
    {theme==='light'?'🌙':'☀️'}
  </button>;
}
```

---

## 📱 **5. RESPONSIVE & ACCESSIBILITÉ**

### **Breakpoints**
- Mobile : `< 640px`
- Tablet : `640px - 1024px`
- Desktop : `> 1024px`

### **Accessibilité (WCAG AA)**
- ✅ `aria-label` sur boutons icon-only
- ✅ Focus visible : `outline: 2px solid var(--primary)`
- ✅ Contraste texte/background ≥ 4.5:1
- ✅ Navigation clavier (Tab, Enter, Escape)
- ✅ `alt` sur images, `label` sur inputs

---

## 🧪 **6. TESTS E2E (Playwright)**

```bash
cd maieutik-web
npm run build          # Build production
npm test               # Playwright tests
npm run test:ui        # UI mode
```

**Tests existants** (`tests/screenshots.spec.ts`) :
- Capture home page (mock `/api/files`)
- Capture chat page

**Ajouter un test :**
```ts
// tests/mon-test.spec.ts
import { test, expect } from '@playwright/test';
test('recherche affiche résultats', async ({ page }) => {
  await page.route('**/api/search*', route => route.fulfill({ json: { query: 'test', hits: ['file.md: résultat'] } }));
  await page.goto('/');
  await expect(page.locator('.message')).toContainText('résultat');
});
```

---

## 🔧 **7. DÉVELOPPEMENT LOCAL**

```bash
cd maieutik-web
npm install
npm run dev            # Vite dev server (http://localhost:5173)
# API attendue sur http://localhost:3000 (config VITE_API_URL)
```

**Variables d'env (`.env`) :**
```bash
VITE_API_URL=http://localhost:3000
```

---

## ✅ **CHECKLIST WEBDESIGNER**

- [ ] `npm run build` passe sans erreur TypeScript
- [ ] `npm run typecheck` propre
- [ ] `npm test` (Playwright) passe
- [ ] Dark/Light mode fonctionnel
- [ ] Responsive testé mobile/tablet/desktop
- [ ] Accessibilité : focus, aria, contraste OK
- [ ] Composants réutilisables (Button, Input, Message, Card)
- [ ] Variables CSS centralisées dans `style.css`
- [ ] Pas de styles inline (sauf cas exceptionnels)
- [ ] Documentation composants mise à jour