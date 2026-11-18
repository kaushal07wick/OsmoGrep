//! Cached presentation data never contains account credentials and never needs startup HTTP.
use super::Profile;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub workspace: Option<String>,
    pub avatar: Option<String>,
}
impl Identity {
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            id: profile.user.id.clone(),
            name: profile.user.label().into(),
            username: profile.user.username.clone(),
            workspace: profile.workspace.as_ref().map(|w| w.name.clone()),
            avatar: None,
        }
    }
}
fn path() -> PathBuf {
    crate::persistence::config_dir().join("account/profile.json")
}
pub fn cached() -> Option<Identity> {
    if !crate::credentials::account_present() {
        return None;
    }
    if std::fs::symlink_metadata(path())
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path())
        .ok()?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 128 * 1024 {
        return None;
    }
    let value: Identity = serde_json::from_slice(&bytes).ok()?;
    if [&value.id, &value.name]
        .into_iter()
        .chain(value.username.iter())
        .chain(value.workspace.iter())
        .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        || value.avatar.as_ref().is_some_and(|s| !valid_avatar(s))
    {
        return None;
    }
    Some(value)
}
pub fn remember(profile: &Profile, token: &str) {
    let mut identity = Identity::from_profile(profile);
    identity.avatar = cached()
        .filter(|v| v.id == identity.id)
        .and_then(|v| v.avatar)
        .or_else(|| avatar(&identity.id));
    let Ok(_lock) = crate::auth::credential_lock("osmogrep") else {
        return;
    };
    // A concurrent sign-out or account switch must not resurrect an older identity.
    let same = crate::credentials::get("oauth/osmogrep")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|saved| saved["access_token"].as_str() == Some(token));
    if same {
        let file = path();
        if std::fs::create_dir_all(file.parent().unwrap()).is_ok() {
            if let Ok(bytes) = serde_json::to_vec(&identity) {
                let _ = crate::storage::atomic_write(&file, &bytes);
            }
        }
    }
}
pub fn forget() {
    let _ = std::fs::remove_file(path());
}
fn avatar(id: &str) -> Option<String> {
    if id.is_empty() || id.len() > 32 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1200))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let response = client
        .get(format!("https://avatars.githubusercontent.com/u/{id}?s=96"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?;
    let mut bytes = Vec::new();
    response.take(256 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 256 * 1024 {
        return None;
    }
    encode_avatar(&bytes)
}
fn valid_avatar(value: &str) -> bool {
    if value.len() > 64 * 1024 {
        return false;
    }
    let Ok(bytes) = STANDARD.decode(value) else {
        return false;
    };
    if bytes.len() < 24
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes[16..24] != [0, 0, 0, 96, 0, 0, 0, 96]
    {
        return false;
    }
    image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).is_ok()
}
fn encode_avatar(bytes: &[u8]) -> Option<String> {
    let mut reader = image::io::Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let mut image = reader
        .decode()
        .ok()?
        .resize_to_fill(96, 96, image::imageops::FilterType::Triangle)
        .to_rgba8();
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let distance = ((x as f32 - 47.5).powi(2) + (y as f32 - 47.5).powi(2)).sqrt();
        pixel[3] = (pixel[3] as f32 * (48.0 - distance).clamp(0.0, 1.0)) as u8;
    }
    let image = image::DynamicImage::ImageRgba8(image);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageOutputFormat::Png)
        .ok()?;
    Some(STANDARD.encode(png.into_inner()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn portrait_is_a_bounded_circular_png_and_cached_control_sequences_are_rejected() {
        use base64::Engine;
        let source = image::DynamicImage::new_rgb8(2, 2);
        let mut bytes = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut bytes, image::ImageOutputFormat::Png)
            .unwrap();
        let encoded = super::encode_avatar(bytes.get_ref()).unwrap();
        assert!(super::valid_avatar(&encoded));
        assert!(!super::valid_avatar("\x1b_Ga=T;secret"));
        assert!(!super::valid_avatar(&super::STANDARD.encode(b"not a PNG")));
        let image = image::load_from_memory(&super::STANDARD.decode(encoded).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (96, 96));
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(48, 48)[3], 255);
    }
    #[test]
    fn identity_contains_only_presentation_fields_and_skips_non_github_ids() {
        let profile=super::super::parse(br#"{"user":{"id":"fixture-user","name":"Alex Lee","username":"alex"},"workspace":{"id":"w","name":"Personal","kind":"personal"}}"#).unwrap();
        let value = serde_json::to_value(super::Identity::from_profile(&profile)).unwrap();
        assert_eq!(value["name"], "Alex Lee");
        assert_eq!(value["workspace"], "Personal");
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert!(super::avatar("fixture-user").is_none());
        assert!(super::avatar("../1").is_none());
    }
}
