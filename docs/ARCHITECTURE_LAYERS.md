# Architecture par Couches — Maieutik

## Couche 1 : Frontend (React/Tauri)
**Responsable** : Interface utilisateur  
**Technologies** : React, TypeScript, Tauri  
**Flux** : Événements → API routes  

## Couche 2 : API (Axum/Tauri Bridge)
**Responsable** : Points d'entrée HTTP/MCP  
**Technologies** : Axum, serde, tokio  
**Flux** : Requêtes → Core traits → Réponses  

## Couche 3 : Core (Rust Traits)
**Responsable** : Logique métier  
**Traits clés** : Embedder, Retriever, Generator, Judge, Transcriber  
**Flux** : Orchestration → FFI ou implémentations internes  

## Couche 4 : FFI (C++ Modules)
**Responsable** : Performance critiques  
**Fichiers** : `file_service.cpp`, `wiki_links.cpp`, `markdown_tables.cpp`, `license.cpp`  
**Flux** : Zero-copy ↔ Rust bindings  

## Couche 5 : Système
**Responsable** : Fichiers, réseaux, processus  
**Technologies** : std::fs, tokio, serde_json  
**Flux** : I/O bas niveau → Core
