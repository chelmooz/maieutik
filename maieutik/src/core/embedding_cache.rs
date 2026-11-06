//! Cache LRU pour embeddings calculés
//!
//! Évite le recalcul des embeddings pour le même texte, améliorant
//! significativement les performances lors de la recherche ou de l'indexation
//! de fichiers dont le contenu n'a pas changé.

use anyhow::Result;
use lru::LruCache;
use parking_lot::RwLock;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Cache LRU pour embeddings textuels.
///
/// # Thread Safety
/// Utilise un RwLock ParkingLot pour un verrouillage léger adapté aux
/// lecturesConcurrentes. L'ID unique du texte est calculé via un hash Murmur3.
///
/// # Performance
/// Par défaut 1000 entrées (~10 Mo pour des embeddings BGE-M3), configurable
/// via `MAIEUTIK_EMBED_CACHE_SIZE` environment variable.
#[derive(Debug, Clone)]
pub struct EmbeddingCache {
    /// Nombre maximum d'entrées dans le cache
    capacity: usize,
    /// Cache LRU protégé par RwLock
    cache: Arc<RwLock<LruCache<u64, Vec<f32>>>>,
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl EmbeddingCache {
    /// Crée un nouveau cache avec la capacité spécifiée
    pub fn new(capacity: usize) -> Self {
        let cache = LruCache::new(NonZeroUsize::new(capacity.max(1)).expect("Cache capacity must be at least 1"));
        Self {
            capacity,
            cache: Arc::new(RwLock::new(cache)),
        }
    }

    /// Calcule un hash unique pour le texte (Murmur3-like)
    fn hash_text(text: &str) -> u64 {
        let hash1: u64 = {
            let mut h: u64 = 0xc924c924c924c924;
            for byte in text.as_bytes() {
                h = h.wrapping_add(*byte as u64);
                h = h.wrapping_mul(0x9e3779b97f4a7c15);
            }
            h
        };
        let hash2: u64 = {
            let mut h: u64 = 0x9e3779b97f4a7c15;
            for byte in text.bytes().rev() {
                h ^= (h << 7).wrapping_add(byte as u64).wrapping_add(0xd6e8feb5);
            }
            h
        };
        hash1 ^ hash2
    }

    /// Recherche un embedding dans le cache
    /// Retourne `Some(embedding)` si trouvé, `None` sinon
    pub fn get(&self, text: &str) -> Option<Vec<f32>> {
        let hash = Self::hash_text(text);
        let mut guard = self.cache.write();
        guard.get(&hash).cloned()
    }

    /// Stocke un embedding dans le cache
    /// Retourne l'ancien embedding si remplacé, ou None si nouveau
    pub fn insert(&self, text: &str, embedding: Vec<f32>) -> Option<Vec<f32>> {
        let hash = Self::hash_text(text);
        let mut guard = self.cache.write();
        guard.push(hash, embedding).map(|(_, v)| v)
    }

    /// Obtient ou calcule un embedding avec cache
    ///
    /// Si le texte est déjà en cache, retourne l'embedding en 1 lecture (O(1)).
    /// Sinon, appelle `compute_fn`, stocke le résultat et le retourne.
    pub fn get_or_insert<F>(&self, text: &str, compute_fn: F) -> Result<Vec<f32>>
    where
        F: FnOnce() -> Result<Vec<f32>>,
    {
        if let Some(cached) = self.get(text) {
            return Ok(cached);
        }

        let embedding = compute_fn()?;
        let len = embedding.len();
        self.insert(text, embedding.clone());
        tracing::debug!("Cached embedding de taille {} pour texte de {} bytes", len, text.len());
        Ok(embedding)
    }

    /// VIDE le cache (utile pour les changements de modèle)
    pub fn clear(&self) {
        self.cache.write().clear();
    }

    /// Retourne la taille actuelle du cache
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Vérifie si le cache est vide
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Statistiques du cache (pour debug/metrics)
    pub fn stats(&self) -> (usize, usize) {
        let guard = self.cache.read();
        (guard.len(), guard.cap().get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = EmbeddingCache::new(10);
        let text = "test text for caching";
        let embedding = vec![1.0, 2.0, 3.0];

        // Insert
        let old = cache.insert(text, embedding.clone());
        assert!(old.is_none(), "Première insertion ne devrait pas remplacer");

        // Get
        let cached = cache.get(text);
        assert_eq!(cached, Some(embedding.clone()), "Embedding devrait être trouvé");

        // Length
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_replacement() {
        let cache = EmbeddingCache::new(2);
        let text1 = "text one";
        let text2 = "text two";
        let text3 = "text three";

        cache.insert(text1, vec![1.0]);
        cache.insert(text2, vec![2.0]);
        cache.insert(text3, vec![3.0]);

        // text1 devrait avoir été évincé (LRU)
        assert!(cache.get(text1).is_none(), "text1 devrait être évincé");
        assert!(cache.get(text2).is_some(), "text2 devrait être présent");
        assert!(cache.get(text3).is_some(), "text3 devrait être présent");
    }

    #[test]
    fn test_hash_consistency() {
        let cache = EmbeddingCache::new(100);
        let text = "same text";

        // Même texte = même hash
        let hash1 = EmbeddingCache::hash_text(text);
        let hash2 = EmbeddingCache::hash_text(text);
        assert_eq!(hash1, hash2, "Hash doit être constant");

        // Texte différent = hash différent (probable)
        let other_hash = EmbeddingCache::hash_text("different text");
        assert_ne!(hash1, other_hash, "Hashes différents devraient être différents");
    }

    #[test]
    fn test_get_or_insert() {
        let cache = EmbeddingCache::new(10);
        let text = "text to compute";
        let mut compute_count = 0;

        let result1 = cache.get_or_insert(text, || {
            compute_count += 1;
            Ok(vec![compute_count as f32])
        });
        assert!(result1.is_ok());
        assert_eq!(result1.expect("test"), vec![1.0]);
        assert_eq!(compute_count, 1);

        // Deuxième appel devrait utiliser le cache
        let result2 = cache.get_or_insert(text, || {
            compute_count += 1;
            Ok(vec![compute_count as f32])
        });
        assert!(result2.is_ok());
        assert_eq!(result2.expect("test"), vec![1.0]); // Pas [2.0] - cache utilisé!
        assert_eq!(compute_count, 1); // Pas appelé de nouveau
    }
}