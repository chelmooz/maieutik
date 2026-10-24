//! Google Drive Sync — OAuth2 + Drive API v3
//!

use anyhow::Result;
use oauth2::basic::BasicClient;
use oauth2::{ClientId, RedirectUrl};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use google_drive3::api::Drive;
use google_drive3::api::File;

// The Google OAuth client id is NOT embedded: it is supplied at runtime via
// `MAIEUTIK_GOOGLE_CLIENT_ID` (the client secret must never be embedded in a
// native binary either; if a flow needs one, load it from the keyring/config).
// Without it, `GoogleDriveAuth` builds no OAuth client and every OAuth
// operation reports that Google Drive is not configured.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64, // Unix timestamp
    pub token_type: String,
}

impl DriveToken {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() >= self.expires_at - 60 // 1 min buffer
    }
}

pub struct GoogleDriveAuth {
    /// `None` until `MAIEUTIK_GOOGLE_CLIENT_ID` is provided at runtime.
    client: Option<BasicClient>,
    http: HttpClient,
    token_store: KeyringTokenStore,
}

impl GoogleDriveAuth {
    pub fn new() -> Self {
        let client = std::env::var("MAIEUTIK_GOOGLE_CLIENT_ID")
            .ok()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .map(|id| {
                BasicClient::new(ClientId::new(id))
                    .set_redirect_uri(
                        RedirectUrl::new("http://localhost:54321/callback".to_string())
                            .expect("static redirect URL is valid"),
                    )
            });

        Self {
            client,
            http: HttpClient::new(),
            token_store: KeyringTokenStore,
        }
    }

    /// The configured OAuth client, or an error explaining how to configure it.
    fn oauth_client(&self) -> Result<&BasicClient> {
        self.client.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Google Drive is not configured: set MAIEUTIK_GOOGLE_CLIENT_ID \
                 to your own OAuth client id"
            )
        })
    }

    pub async fn revoke_token(&self) -> Result<()> {
        if let Some(token) = self.token_store.load_token().await? {
            let _ = self.http
                .post("https://oauth2.googleapis.com/revoke")
                .form(&[("token", token.access_token)])
                .send()
                .await;
            self.token_store.delete_token().await?;
        }
        Ok(())
    }
}

pub struct KeyringTokenStore;

impl KeyringTokenStore {
    pub async fn save_token(&self, token: &DriveToken) -> Result<()> {
        crate::config::store::secrets::set(
            crate::config::store::secrets::GOOGLE_REFRESH_TOKEN,
            &token.refresh_token
        )?;
        crate::config::store::secrets::set(
            crate::config::store::secrets::GOOGLE_ACCESS_TOKEN,
            &token.access_token
        )?;
        Ok(())
    }

    pub async fn load_token(&self) -> Result<Option<DriveToken>> {
        let refresh = crate::config::store::secrets::get(crate::config::store::secrets::GOOGLE_REFRESH_TOKEN)?;
        let access = crate::config::store::secrets::get(crate::config::store::secrets::GOOGLE_ACCESS_TOKEN)?;
        if let (Some(refresh), Some(access)) = (refresh, access) {
            Ok(Some(DriveToken {
                access_token: access,
                refresh_token: refresh,
                expires_at: chrono::Utc::now().timestamp() + 3600,
                token_type: "Bearer".to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_token(&self) -> Result<()> {
        crate::config::store::secrets::delete(crate::config::store::secrets::GOOGLE_REFRESH_TOKEN)?;
        crate::config::store::secrets::delete(crate::config::store::secrets::GOOGLE_ACCESS_TOKEN)?;
        Ok(())
    }
}

pub struct DriveSync {
    auth: GoogleDriveAuth,
    drive: Option<Drive>,
    settings: Arc<RwLock<crate::config::AppSettings>>,
    local_root: PathBuf,
}

impl DriveSync {
    pub fn new(
        auth: GoogleDriveAuth,
        settings: Arc<RwLock<crate::config::AppSettings>>,
        local_root: PathBuf,
    ) -> Self {
        Self {
            auth,
            drive: None,
            settings,
            local_root,
        }
    }

    async fn ensure_drive(&mut self) -> Result<&Drive> {
        // Google Drive sync not configured — set GOOGLE_ACCESS_TOKEN/GOOGLE_REFRESH_TOKEN in keyring
        Err(anyhow::anyhow!("Google Drive sync not configured. Set GOOGLE_ACCESS_TOKEN and GOOGLE_REFRESH_TOKEN in keyring."))
    }

    pub async fn sync(&mut self) -> Result<SyncResult> {
        info!("Starting Drive sync");
        let drive = self.ensure_drive().await?;

        // TODO: implement full bidirectional sync
        // 1. List remote backup folder
        // 2. Compare with local files (hash, mtime)
        // 3. Upload new/changed local files
        // 4. Download new/changed remote files
        // 5. Handle conflicts per strategy
        // 6. Update local index

        let mut result = SyncResult::default();
        // Placeholder
        result.files_uploaded = 0;
        result.files_downloaded = 0;
        result.conflicts = 0;

        info!("Sync complete: {:?}", result);
        Ok(result)
    }

    pub async fn backup_file(&mut self, local_path: &PathBuf, remote_name: &str) -> Result<String> {
        let drive = self.ensure_drive().await.map_err(|e| anyhow::anyhow!("Drive not configured: {}", e))?;
        let file = File {
            name: Some(remote_name.to_string()),
            ..Default::default()
        };
        let content = tokio::fs::read(local_path).await?;
        // Placeholder: Google Drive upload not yet implemented
        // let upload = /* ... */
        Ok(format!("Uploaded {} to placeholder location", remote_name))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncResult {
    pub files_uploaded: usize,
    pub files_downloaded: usize,
    pub conflicts: usize,
    pub errors: Vec<String>,
}

pub mod models {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackupFolder {
        pub file_id: String,
        pub name: String,
        pub origin_machine_id: Option<String>,
        pub origin_device_name: Option<String>,
        pub size_bytes: Option<u64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RestorePlan {
        pub items: Vec<RestoreItem>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RestoreItem {
        pub file_id: String,
        pub name: String,
        pub destination: std::path::PathBuf,
        pub origin_machine_id: Option<String>,
    }
}