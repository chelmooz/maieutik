//! Local Registry - Stockage JSON des métadonnées de modèles
//!
//! Format : `$MAIEUTIK_DATA_DIR/models/registry.json`
//! Structure : `{ "models": { "name": ModelMetadata } }`

use super::{ModelMetadata, ModelStatus};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// Registre local des modèles
pub struct LocalRegistry {
    path: PathBuf,
    models: RwLock<HashMap<String, ModelMetadata>>,
}

impl LocalRegistry {
    /// Crée un nouveau registre, charge les données existantes
    pub fn new(data_dir: &std::path::Path) -> Result<Self> {
        let models_dir = data_dir.join("models");
        let path = models_dir.join("registry.json");

        let models = if path.exists() {
            load_registry(&path)?
        } else {
            std::fs::create_dir_all(&models_dir)?;
            HashMap::new()
        };

        // Sauvegarde initiale si le fichier n'existait pas
        if !path.exists() {
            save_registry(&path, &models)?;
        }

        Ok(Self {
            path,
            models: RwLock::new(models),
        })
    }

    /// Ajoute ou met à jour un modèle
    pub fn add(&self, metadata: ModelMetadata) -> Result<()> {
        let mut models = self.models.write().unwrap();
        models.insert(metadata.name.clone(), metadata);
        save_registry(&self.path, &models)?;
        Ok(())
    }

    /// Supprime un modèle
    pub fn remove(&self, name: &str) -> Result<()> {
        let mut models = self.models.write().unwrap();
        if models.remove(name).is_some() {
            save_registry(&self.path, &models)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Model '{}' not found", name))
        }
    }

    /// Récupère un modèle
    pub fn get(&self, name: &str) -> Result<Option<ModelMetadata>> {
        let models = self.models.read().unwrap();
        Ok(models.get(name).cloned())
    }

    /// Liste tous les modèles
    pub fn list(&self) -> Result<Vec<ModelMetadata>> {
        let models = self.models.read().unwrap();
        let mut list: Vec<ModelMetadata> = models.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(list)
    }

    /// Vérifie si un modèle est installé
    pub fn is_installed(&self, name: &str) -> Result<bool> {
        let models = self.models.read().unwrap();
        Ok(models.contains_key(name))
    }

    /// Retourne le statut d'un modèle
    pub fn status(&self, name: &str) -> Result<ModelStatus> {
        let models = self.models.read().unwrap();
        match models.get(name) {
            Some(metadata) => Ok(ModelStatus::Installed {
                version: metadata.version.clone(),
                backend: metadata.backend,
            }),
            None => Ok(ModelStatus::NotInstalled),
        }
    }

    /// Met à jour l'empreinte d'un modèle après vérification
    pub fn update_sha256(&self, name: &str, sha256: String) -> Result<()> {
        let mut models = self.models.write().unwrap();
        if let Some(metadata) = models.get_mut(name) {
            metadata.sha256 = sha256;
            save_registry(&self.path, &models)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Model '{}' not found", name))
        }
    }
}

/// Charge le registre depuis le fichier
fn load_registry(path: &PathBuf) -> Result<HashMap<String, ModelMetadata>> {
    let content = std::fs::read_to_string(path)?;
    let registry: HashMap<String, ModelMetadata> = serde_json::from_str(&content)?;
    Ok(registry)
}

/// Sauvegarde le registre dans le fichier (écriture atomique)
fn save_registry(path: &PathBuf, models: &HashMap<String, ModelMetadata>) -> Result<()> {
    let temp_path = path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(models)?;
    std::fs::write(&temp_path, json)?;

    // Renommage atomique
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelMetadata, ModelStatus};
    use tempfile::tempdir;

    #[test]
    fn registry_new_creates_empty_registry() {
        let dir = tempdir().unwrap();
        let reg = LocalRegistry::new(dir.path()).unwrap();
        assert!(reg.list().unwrap().is_empty());
    }

    #[test]
    fn registry_add_and_get() {
        let dir = tempdir().unwrap();
        let reg = LocalRegistry::new(dir.path()).unwrap();
        let meta = ModelMetadata {
            name: "test-model".into(),
            version: "1.0".into(),
            backend: "llama".into(),
            sha256: "abc".into(),
            size: 0,
        };
        reg.add(meta.clone()).unwrap();
        let got = reg.get("test-model").unwrap().unwrap();
        assert_eq!(got.name, meta.name);
    }

    #[test]
    fn registry_remove_not_found_error() {
        let dir = tempdir().unwrap();
        let reg = LocalRegistry::new(dir.path()).unwrap();
        let err = reg.remove("missing").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
