use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

const MAX_UPDATE_BYTES: u64 = 1024 * 1024 * 1024;
const UPDATE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct UpdateGate(AtomicBool);

struct UpdatePermit<'a>(&'a AtomicBool);

impl Drop for UpdatePermit<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current_version: String,
    configured: bool,
    automatic_checks: bool,
    offline_installer_available: bool,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    available: bool,
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
    published_at: Option<String>,
    byte_size: Option<u64>,
}

#[tauri::command]
pub fn get_update_status(app: AppHandle) -> UpdateStatus {
    match release_config() {
        Ok(_) => UpdateStatus {
            current_version: app.package_info().version.to_string(),
            configured: true,
            automatic_checks: false,
            offline_installer_available: true,
            reason: None,
        },
        Err(reason) => UpdateStatus {
            current_version: app.package_info().version.to_string(),
            configured: false,
            automatic_checks: false,
            offline_installer_available: true,
            reason: Some(reason),
        },
    }
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    gate: State<'_, UpdateGate>,
) -> Result<UpdateCheckResult, String> {
    let _permit = acquire(&gate)?;
    let current_version = app.package_info().version.to_string();
    let update = configured_updater(&app)?
        .check()
        .await
        .map_err(safe_update_error)?;
    let Some(update) = update else {
        return Ok(UpdateCheckResult {
            available: false,
            current_version,
            version: None,
            notes: None,
            published_at: None,
            byte_size: None,
        });
    };
    let evidence = validate_update(&update)?;
    let published_at = update
        .raw_json
        .get("pub_date")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(UpdateCheckResult {
        available: true,
        current_version,
        version: Some(update.version),
        notes: update.body,
        published_at,
        byte_size: Some(evidence.byte_size),
    })
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    gate: State<'_, UpdateGate>,
    expected_version: String,
) -> Result<(), String> {
    if expected_version.len() > 64 || expected_version.trim() != expected_version {
        return Err("The selected update version is invalid.".to_owned());
    }
    let _permit = acquire(&gate)?;
    let update = configured_updater(&app)?
        .check()
        .await
        .map_err(safe_update_error)?
        .ok_or_else(|| "No newer signed update is available.".to_owned())?;
    if update.version != expected_version {
        return Err("The available release changed. Check for updates again.".to_owned());
    }
    let evidence = validate_update(&update)?;
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(safe_update_error)?;
    if u64::try_from(bytes.len()).ok() != Some(evidence.byte_size) {
        return Err("The signed update size does not match its release manifest.".to_owned());
    }
    if sha256(&bytes) != evidence.sha256 {
        return Err("The signed update hash does not match its release manifest.".to_owned());
    }
    update.install(bytes).map_err(safe_update_error)
}

fn acquire(gate: &UpdateGate) -> Result<UpdatePermit<'_>, String> {
    gate.0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Another update operation is already running.".to_owned())?;
    Ok(UpdatePermit(&gate.0))
}

fn configured_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let (endpoint, public_key) = release_config()?;
    let redirect_endpoint = endpoint.clone();
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(safe_update_error)?
        .pubkey(public_key)
        .timeout(UPDATE_TIMEOUT)
        .configure_client(move |client| {
            let redirect_endpoint = redirect_endpoint.clone();
            client.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 4
                    || !is_approved_redirect_url(&redirect_endpoint, attempt.url())
                {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
        })
        .build()
        .map_err(safe_update_error)
}

fn release_config() -> Result<(Url, String), String> {
    let endpoint = option_env!("AIFOS_UPDATE_ENDPOINT")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "This build has no approved update channel. Use a signed offline installer.".to_owned()
        })?;
    let public_key = option_env!("AIFOS_UPDATE_PUBKEY")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "This build has no approved updater public key. Use a signed offline installer."
                .to_owned()
        })?;
    if public_key.len() > 4096 || public_key.contains("PRIVATE KEY") {
        return Err("The embedded updater public key is invalid.".to_owned());
    }
    let endpoint =
        Url::parse(endpoint).map_err(|_| "The approved update endpoint is invalid.".to_owned())?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err("The approved update endpoint must be credential-free HTTPS.".to_owned());
    }
    Ok((endpoint, public_key.to_owned()))
}

struct UpdateEvidence {
    byte_size: u64,
    sha256: String,
}

fn validate_update(update: &Update) -> Result<UpdateEvidence, String> {
    let (endpoint, _) = release_config()?;
    if !is_approved_artifact_url(&endpoint, &update.download_url) {
        return Err("The update artifact URL is outside the approved HTTPS origin.".to_owned());
    }
    if update.signature.is_empty() || update.signature.len() > 4096 {
        return Err("The update manifest has no bounded artifact signature.".to_owned());
    }
    if update.body.as_ref().is_some_and(|notes| notes.len() > 4000) {
        return Err("The update notes exceed the supported length.".to_owned());
    }
    let platform = update
        .raw_json
        .get("platforms")
        .and_then(serde_json::Value::as_object)
        .and_then(|platforms| {
            platforms.values().find(|candidate| {
                candidate.get("url").and_then(serde_json::Value::as_str)
                    == Some(update.download_url.as_str())
            })
        })
        .ok_or_else(|| "The update manifest omitted platform evidence.".to_owned())?;
    let byte_size = platform
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .filter(|size| *size > 0 && *size <= MAX_UPDATE_BYTES)
        .ok_or_else(|| "The update manifest has an invalid artifact size.".to_owned())?;
    let sha256 = platform
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| "The update manifest has an invalid SHA-256 value.".to_owned())?;
    Ok(UpdateEvidence {
        byte_size,
        sha256: sha256.to_owned(),
    })
}

fn is_approved_artifact_url(endpoint: &Url, artifact: &Url) -> bool {
    artifact.scheme() == "https"
        && artifact.scheme() == endpoint.scheme()
        && artifact.host_str() == endpoint.host_str()
        && artifact.port_or_known_default() == endpoint.port_or_known_default()
        && artifact.username().is_empty()
        && artifact.password().is_none()
}

fn is_approved_redirect_url(endpoint: &Url, candidate: &Url) -> bool {
    if is_approved_artifact_url(endpoint, candidate) {
        return true;
    }
    candidate.scheme() == "https"
        && candidate.port_or_known_default() == Some(443)
        && candidate.username().is_empty()
        && candidate.password().is_none()
        && endpoint.host_str() == Some("github.com")
        && candidate
            .host_str()
            .is_some_and(|host| host.ends_with(".githubusercontent.com"))
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn safe_update_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("signature") {
        "The update signature could not be verified. Nothing was installed.".to_owned()
    } else {
        "The update check or installation did not complete. Existing application and workspace data were preserved. Try the signed offline installer if the problem continues.".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_lowercase_and_deterministic() {
        assert_eq!(
            sha256(b"aifos"),
            "0640cb7557c9cd3d585f1741c69e8cdc0ba342a553df5c62509bb663fc710bac"
        );
    }

    #[test]
    fn update_gate_rejects_concurrent_operations() {
        let gate = UpdateGate::default();
        let permit = acquire(&gate).unwrap();
        assert!(acquire(&gate).is_err());
        drop(permit);
        assert!(acquire(&gate).is_ok());
    }

    #[test]
    fn artifact_urls_require_the_exact_credential_free_https_origin() {
        let endpoint = Url::parse("https://updates.example.test/latest.json").unwrap();
        assert!(is_approved_artifact_url(
            &endpoint,
            &Url::parse("https://updates.example.test/aifos.exe").unwrap()
        ));
        for candidate in [
            "http://updates.example.test/aifos.exe",
            "https://updates.example.test:444/aifos.exe",
            "https://cdn.updates.example.test/aifos.exe",
            "https://user:secret@updates.example.test/aifos.exe",
        ] {
            assert!(!is_approved_artifact_url(
                &endpoint,
                &Url::parse(candidate).unwrap()
            ));
        }
    }

    #[test]
    fn redirects_are_bounded_to_the_configured_origin_or_github_asset_hosts() {
        let github = Url::parse("https://github.com/aifos/project/latest.json").unwrap();
        assert!(is_approved_redirect_url(
            &github,
            &Url::parse("https://github.com/aifos/project/releases/latest.json").unwrap()
        ));
        assert!(is_approved_redirect_url(
            &github,
            &Url::parse("https://release-assets.githubusercontent.com/signed-object").unwrap()
        ));
        for candidate in [
            "https://githubusercontent.com/not-a-subdomain",
            "https://release-assets.githubusercontent.com:444/object",
            "https://example.com/object",
            "http://release-assets.githubusercontent.com/object",
        ] {
            assert!(!is_approved_redirect_url(
                &github,
                &Url::parse(candidate).unwrap()
            ));
        }
        let private_origin = Url::parse("https://updates.example.test/latest.json").unwrap();
        assert!(!is_approved_redirect_url(
            &private_origin,
            &Url::parse("https://release-assets.githubusercontent.com/object").unwrap()
        ));
    }
}
