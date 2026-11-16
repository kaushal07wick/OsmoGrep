//! Immutable user-selected image bytes back both the thumbnail and model input.
use crate::state::AgentState;
#[cfg(target_os = "macos")]
use crate::state::LogLevel;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{io::Reader, ImageOutputFormat};
use serde_json::{json, Value};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 20 * 1024 * 1024;
const MAX_IMAGES: usize = 8;

#[derive(Clone)]
pub struct Attachment {
    pub id: u32,
    pub label: String,
    pub thumbnail: String,
    pub width: u32,
    pub height: u32,
    data_url: String,
}

impl Attachment {
    pub fn from_path(path: &Path, id: u32) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::from_bytes(bytes, id)
    }
    fn from_bytes(bytes: Vec<u8>, id: u32) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Image exceeds 20 MiB".into());
        }
        let mut reader = Reader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::io::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| format!("Cannot decode attached image: {e}"))?;
        let (width, height) = (image.width(), image.height());
        let mut thumbnail = Cursor::new(Vec::new());
        image
            .thumbnail(180, 120)
            .write_to(&mut thumbnail, ImageOutputFormat::Png)
            .map_err(|e| e.to_string())?;
        let mime = match image::guess_format(&bytes).map_err(|e| e.to_string())? {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::Gif => "image/gif",
            image::ImageFormat::WebP => "image/webp",
            _ => return Err("Attach a PNG, JPEG, GIF or WebP image".into()),
        };
        Ok(Self {
            id,
            label: format!("[Image #{id}]"),
            thumbnail: STANDARD.encode(thumbnail.into_inner()),
            width,
            height,
            data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        })
    }
}

pub fn dropped_image(text: &str, root: &Path) -> Option<PathBuf> {
    let trimmed = text.trim();
    let path = if trimmed.starts_with("file://") {
        url::Url::parse(trimmed).ok()?.to_file_path().ok()?
    } else {
        PathBuf::from(trimmed.trim_matches(['\'', '"']).replace("\\ ", " "))
    };
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
        return None;
    }
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.is_file().then_some(path)
}

fn register(state: &mut AgentState, attachment: Attachment) -> Result<(), String> {
    state.ui.attachments.retain(|a| {
        state.ui.input.contains(&a.label)
            || state
                .ui
                .queued_agent_prompts
                .iter()
                .any(|p| p.contains(&a.label))
    });
    if state.ui.attachments.len() >= MAX_IMAGES {
        return Err("At most eight images can be attached".into());
    }
    state.ui.next_attachment_id = attachment.id.saturating_add(1);
    state.insert_text(&format!("{} ", attachment.label));
    state.ui.attachments.push(attachment);
    Ok(())
}

pub fn attach(state: &mut AgentState, path: &Path) -> Result<(), String> {
    let attachment = Attachment::from_path(path, state.ui.next_attachment_id)?;
    register(state, attachment)
}

pub fn message_for(state: &AgentState, text: &str) -> Option<Value> {
    let mut content = vec![
        json!({"type":"input_text", "text":"Images attached by the user. The following user message refers to these labels."}),
    ];
    for image in state
        .ui
        .attachments
        .iter()
        .filter(|image| text.contains(&image.label))
    {
        content.push(json!({"type":"input_text", "text":image.label}));
        content.push(json!({"type":"input_image", "image_url":image.data_url}));
    }
    (content.len() > 1).then(|| json!({"role":"user","content":content}))
}

pub fn paste(state: &mut AgentState) {
    #[cfg(target_os = "macos")]
    {
        // This is only called by the user's explicit paste key.
        let script = include_str!("clipboard_image.applescript");
        if state.ui.input_mode != crate::state::InputMode::ApiKey {
            if let Ok(output) = Command::new("osascript")
                .args(["-l", "AppleScript", "-e", script])
                .output()
            {
                if output.status.success() && output.stdout.len() <= MAX_BYTES * 2 {
                    if let Ok(bytes) =
                        STANDARD.decode(String::from_utf8_lossy(&output.stdout).trim())
                    {
                        if !bytes.is_empty() {
                            let result = Attachment::from_bytes(bytes, state.ui.next_attachment_id)
                                .and_then(|a| register(state, a));
                            if let Err(e) = result {
                                state.logs.push(LogLevel::Warn, e);
                            }
                            return;
                        }
                    }
                }
            }
        }
        if let Ok(output) = Command::new("pbpaste").output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n");
                if let Some(path) = dropped_image(&text, &state.repo_root)
                    .filter(|_| state.ui.input_mode != crate::state::InputMode::ApiKey)
                {
                    if let Err(e) = attach(state, &path) {
                        state.logs.push(LogLevel::Warn, e);
                    }
                } else {
                    state.insert_text(&text);
                }
                return;
            }
        }
    }
    state.paste_input();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_clipboard_image_script_compiles() {
        let output =
            std::env::temp_dir().join(format!("osmogrep-clipboard-{}.scpt", uuid::Uuid::new_v4()));
        let result = Command::new("osacompile")
            .arg("-o")
            .arg(&output)
            .arg("-e")
            .arg(include_str!("clipboard_image.applescript"))
            .output()
            .unwrap();
        let _ = std::fs::remove_file(output);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_clipboard_reads_an_image_without_changing_the_users_clipboard() {
        let board = format!("osmogrep-test-{}", uuid::Uuid::new_v4());
        let path = std::env::temp_dir().join(format!("{board}.png"));
        image::DynamicImage::new_rgb8(24, 12).save(&path).unwrap();
        let setup = "use framework \"AppKit\"\non run argv\nset p to current application's NSPasteboard's pasteboardWithName:(item 1 of argv)\np's clearContents()\nset img to current application's NSImage's alloc()'s initWithContentsOfFile:(item 2 of argv)\np's writeObjects:{img}\nend run";
        let copied = Command::new("osascript")
            .args(["-e", setup, &board])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            copied.status.success(),
            "{}",
            String::from_utf8_lossy(&copied.stderr)
        );
        let result = Command::new("osascript")
            .args(["-e", include_str!("clipboard_image.applescript"), &board])
            .output()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = STANDARD
            .decode(String::from_utf8_lossy(&result.stdout).trim())
            .unwrap();
        let attachment = Attachment::from_bytes(bytes, 1).unwrap();
        assert_eq!((attachment.width, attachment.height), (24, 12));
    }
    #[test]
    fn attachment_preview_matches_model_bytes() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(24, 12)
            .write_to(&mut png, ImageOutputFormat::Png)
            .unwrap();
        let bytes = png.into_inner();
        let a = Attachment::from_bytes(bytes.clone(), 1).unwrap();
        assert_eq!((a.width, a.height), (24, 12));
        assert_eq!(
            STANDARD
                .decode(a.data_url.split(',').nth(1).unwrap())
                .unwrap(),
            bytes
        );
        assert!(image::load_from_memory(&STANDARD.decode(&a.thumbnail).unwrap()).is_ok());
        let mut state = crate::init_state();
        state.ui.attachments.push(a);
        let message = message_for(&state, "Explain [Image #1]").unwrap();
        assert_eq!(message["content"][2]["type"], "input_image");
        assert_eq!(message_for(&state, "Retry [Image #1]"), Some(message));
        assert!(message_for(&state, "plain text").is_none());
    }
    #[test]
    fn deleting_any_part_of_an_attachment_removes_the_whole_item() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, ImageOutputFormat::Png)
            .unwrap();
        let image = Attachment::from_bytes(png.into_inner(), 2).unwrap();
        for forward in [false, true] {
            for offset in if forward {
                0..image.label.len()
            } else {
                1..image.label.len() + 1
            } {
                let mut state = crate::init_state();
                state.ui.input = format!("界 before {} after", image.label);
                state.ui.input_cursor = "界 before ".len() + offset;
                state.ui.attachments.push(image.clone());
                if forward {
                    state.delete_forward();
                } else {
                    state.backspace();
                }
                assert_eq!(state.ui.input, "界 before  after");
                assert_eq!(state.ui.input_cursor, "界 before ".len());
                assert!(message_for(&state, &state.ui.input).is_none());
            }
        }
        let mut state = crate::init_state();
        state.ui.input = "before [Image #2] after".into();
        state.ui.input_selection_anchor = Some("before [Im".len());
        state.ui.input_cursor = "before [Image #2".len();
        state.ui.attachments.push(image.clone());
        state
            .ui
            .queued_agent_prompts
            .push_back("Explain [Image #2]".into());
        assert!(state.cut_input());
        assert_eq!(state.ui.input_clipboard, "[Image #2]");
        assert_eq!(state.ui.input, "before  after");
        assert_eq!(
            state.ui.attachments.len(),
            1,
            "a queued message still owns its image"
        );
        assert!(state.paste_input());
        assert_eq!(state.ui.input, "before [Image #2] after");
        assert!(message_for(&state, &state.ui.input).is_some());
        state.ui.input_cursor = "before [Image #2]".len();
        assert!(state.delete_previous_word());
        assert_eq!(state.ui.input, "before  after");
        assert_eq!(state.ui.input_clipboard, "[Image #2]");
        state.ui.input = "[Image #2]".into();
        state.ui.input_cursor = "[Image #2]".len();
        state.ui.attachments.clear();
        state.backspace();
        assert_eq!(
            state.ui.input, "[Image #2",
            "ordinary typed text stays editable"
        );
    }

    #[test]
    fn corrupt_images_are_rejected() {
        assert!(Attachment::from_bytes(b"not an image".to_vec(), 1).is_err());
    }
}
