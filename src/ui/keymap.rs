use crate::preferences::Preferences;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers as M};

pub struct Binding {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub default: &'static str,
    pub command: Option<&'static str>,
}
pub const BINDINGS: &[Binding] = &[
    Binding {
        id: "commands",
        label: "Command center",
        group: "Workspace",
        default: "f1",
        command: Some("/help"),
    },
    Binding {
        id: "settings",
        label: "Settings",
        group: "Workspace",
        default: "f2",
        command: Some("/settings"),
    },
    Binding {
        id: "skills",
        label: "Skills",
        group: "Workspace",
        default: "f3",
        command: Some("/skills"),
    },
    Binding {
        id: "permissions",
        label: "Permissions",
        group: "Workspace",
        default: "f4",
        command: Some("/permissions"),
    },
    Binding {
        id: "model",
        label: "Model",
        group: "Workspace",
        default: "f5",
        command: Some("/model"),
    },
    Binding {
        id: "keymap",
        label: "Keymap",
        group: "Workspace",
        default: "f8",
        command: Some("/keymap"),
    },
    Binding {
        id: "submit",
        label: "Submit",
        group: "Composer",
        default: "enter",
        command: None,
    },
    Binding {
        id: "newline",
        label: "Insert newline",
        group: "Composer",
        default: "shift+enter",
        command: None,
    },
    Binding {
        id: "queue",
        label: "Queue follow-up",
        group: "Composer",
        default: "tab",
        command: None,
    },
    Binding {
        id: "select",
        label: "Select all",
        group: "Composer",
        default: "ctrl+a",
        command: None,
    },
    Binding {
        id: "copy",
        label: "Copy selection / draft",
        group: "Composer",
        default: "ctrl+c",
        command: None,
    },
    Binding {
        id: "cut",
        label: "Cut selection / draft",
        group: "Composer",
        default: "ctrl+x",
        command: None,
    },
    Binding {
        id: "paste",
        label: "Paste text or image",
        group: "Composer",
        default: "ctrl+v",
        command: None,
    },
    Binding {
        id: "clear",
        label: "Clear draft",
        group: "Composer",
        default: "ctrl+u",
        command: None,
    },
    Binding {
        id: "delete-word",
        label: "Delete previous word",
        group: "Composer",
        default: "ctrl+w",
        command: None,
    },
    Binding {
        id: "copy-output",
        label: "Copy latest response",
        group: "Transcript",
        default: "ctrl+o",
        command: None,
    },
    Binding {
        id: "scroll-up",
        label: "Scroll back",
        group: "Transcript",
        default: "pageup",
        command: None,
    },
    Binding {
        id: "scroll-down",
        label: "Scroll forward",
        group: "Transcript",
        default: "pagedown",
        command: None,
    },
];
pub fn chord(key: KeyEvent) -> Option<String> {
    let name = match key.code {
        KeyCode::Char(c) if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Enter => "enter".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        _ => return None,
    };
    if matches!(key.code, KeyCode::Char(_)) && !key.modifiers.intersects(M::CONTROL | M::ALT) {
        return None;
    }
    let mut value = String::new();
    for (flag, prefix) in [
        (M::CONTROL, "ctrl+"),
        (M::ALT, "alt+"),
        (M::SHIFT, "shift+"),
    ] {
        if key.modifiers.contains(flag) {
            value.push_str(prefix)
        }
    }
    value.push_str(&name);
    Some(value)
}
pub fn event(value: &str) -> Option<KeyEvent> {
    let mut mods = M::NONE;
    let mut parts = value.split('+').peekable();
    let mut name = "";
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            name = part;
            break;
        }
        match part {
            "ctrl" => mods |= M::CONTROL,
            "alt" => mods |= M::ALT,
            "shift" => mods |= M::SHIFT,
            _ => return None,
        }
    }
    let code = match name {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        _ => {
            if let Some(f) = name
                .strip_prefix('f')
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|n| (1..=12).contains(n))
            {
                KeyCode::F(f)
            } else if name.len() == 1
                && name.as_bytes()[0].is_ascii_alphabetic()
                && mods.intersects(M::CONTROL | M::ALT)
            {
                KeyCode::Char(name.chars().next()?)
            } else {
                return None;
            }
        }
    };
    Some(KeyEvent::new(code, mods))
}
pub fn effective<'a>(prefs: &'a Preferences, b: &'a Binding) -> &'a str {
    prefs
        .key_bindings
        .get(b.id)
        .map(String::as_str)
        .unwrap_or(b.default)
}
pub fn assign(prefs: &mut Preferences, id: &str, value: &str) -> Result<(), String> {
    if !BINDINGS.iter().any(|b| b.id == id) {
        return Err("Unknown action".into());
    }
    if id == "copy" {
        return Err("Ctrl+C stays available for copying and emergency cancellation".into());
    }
    if value != "unbound" {
        if event(value).is_none() || value == "ctrl+c" {
            return Err(
                "Use a function key or supported Ctrl/Alt chord; Escape and Ctrl+C are reserved"
                    .into(),
            );
        }
        if let Some(conflict) = BINDINGS
            .iter()
            .find(|b| b.id != id && effective(prefs, b) == value)
        {
            return Err(format!("Already used by {}", conflict.label));
        }
    }
    let binding = BINDINGS.iter().find(|b| b.id == id).unwrap();
    if value == binding.default {
        prefs.key_bindings.remove(id);
    } else {
        prefs.key_bindings.insert(id.into(), value.into());
    }
    Ok(())
}
pub enum Resolved {
    Command(&'static str),
    Key(KeyEvent),
    Ignore,
    Unchanged,
}
pub fn resolve(prefs: &Preferences, key: KeyEvent) -> Resolved {
    let Some(value) = chord(key) else {
        return Resolved::Unchanged;
    };
    for b in BINDINGS {
        if effective(prefs, b) == value {
            return if let Some(cmd) = b.command {
                Resolved::Command(cmd)
            } else {
                Resolved::Key(event(b.default).unwrap())
            };
        }
    }
    if BINDINGS
        .iter()
        .any(|b| b.default == value && effective(prefs, b) != b.default)
    {
        Resolved::Ignore
    } else {
        Resolved::Unchanged
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remapping_disables_old_key_and_rejects_conflicts_and_unsafe_chords() {
        let mut p = Preferences::default();
        assign(&mut p, "commands", "ctrl+p").unwrap();
        assert!(matches!(
            resolve(&p, event("ctrl+p").unwrap()),
            Resolved::Command("/help")
        ));
        assert!(matches!(
            resolve(&p, event("f1").unwrap()),
            Resolved::Ignore
        ));
        assert!(assign(&mut p, "skills", "ctrl+p").is_err());
        assert!(assign(&mut p, "skills", "a").is_err());
        assert!(assign(&mut p, "skills", "ctrl+c").is_err());
        assert!(assign(&mut p, "copy", "f6").is_err());
        assign(&mut p, "commands", "f1").unwrap();
        assert!(p.key_bindings.is_empty());
    }
}
