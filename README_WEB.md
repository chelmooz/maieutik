# Maieutik Web Frontend

## Stack Technologique
- React 18 + TypeScript 5
- Vite 5 (build tool)
- Axios (API client)
- Zustand (state management)
- React Router (navigation)
- Tailwind CSS (UI styling, optional)

## Démarrage
```bash
cd maieutik-web
npm install
npm run dev
```

L'application démarre sur http://localhost:5173

## Structure
```
maieutik-web/src/
├── pages/
├── components/
├── api/
│   └── client.ts
├── services/
└── store/
```

## API Endpoints
| Route | Méthode | Description |
|-------|---------|-------------|
| /api/files | GET | Lister fichiers markdown |
| /api/search | POST | Recherche BM25/hybrid |
| /health | GET | Health check |

## Connexion avec l'API Rust
Le frontend communique avec `maieutik-api` via Axios. La base URL est configurable via `VITE_API_URL`.

## Build production
```bash
npm run build
npm run preview
```
