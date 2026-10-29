//! The sound events Windows plays for the current user, as the Sound dialog
//! lists them: which sound each has now and what it is called.

/// One event of one program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundEvent {
    /// The key under `HKCU\AppEvents\Schemes\Apps`: `.Default` is Windows
    /// itself, then `Explorer`, `sapisvr` and the programs that add their own.
    pub app: String,
    /// `SystemAsterisk`, `Notification.Default` and so on.
    pub event: String,
    /// What the Sound dialog calls the event, in the current language.
    pub label: String,
    /// The sound in `.Current` as stored, `%SystemRoot%` and the like not
    /// expanded. Empty for silence, `None` for an event that has no value
    /// there at all.
    pub sound: Option<String>,
}

/// Every event that has a `.Current` key, program by program.
pub fn events() -> Vec<SoundEvent> {
    #[cfg(windows)]
    {
        win32::events()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
mod win32 {
    use windows_registry::{CURRENT_USER, Key};

    use super::SoundEvent;

    pub fn events() -> Vec<SoundEvent> {
        let Ok(apps) = CURRENT_USER.open(r"AppEvents\Schemes\Apps") else {
            return Vec::new();
        };
        let labels = CURRENT_USER.open(r"AppEvents\EventLabels").ok();
        let mut found = Vec::new();
        for app in apps.keys().into_iter().flatten() {
            let Ok(app_key) = apps.open(&app) else {
                continue;
            };
            for event in app_key.keys().into_iter().flatten() {
                let Ok(current) = app_key.open(format!(r"{event}\.Current")) else {
                    continue;
                };
                let label = labels
                    .as_ref()
                    .and_then(|labels| label_of(labels, &event))
                    .unwrap_or_else(|| event.clone());
                found.push(SoundEvent {
                    app: app.clone(),
                    event,
                    label,
                    sound: current.get_string("").ok(),
                });
            }
        }
        found
    }

    /// The label is a reference into a resource file, which gives the name in
    /// the current language; the plain value next to it is English.
    fn label_of(labels: &Key, event: &str) -> Option<String> {
        let key = labels.open(event).ok()?;
        key.get_string("DispFileName")
            .ok()
            .and_then(|reference| crate::shell::indirect_string(&reference))
            .or_else(|| key.get_string("").ok().filter(|text| !text.is_empty()))
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    #[test]
    fn events_have_a_program_a_name_and_a_label() {
        let events = events();
        for event in &events {
            assert!(
                !event.app.is_empty() && !event.event.is_empty(),
                "{event:?}"
            );
            assert!(!event.label.is_empty(), "{event:?}");
        }
        // A sound is a file, or empty for silence.
        for event in &events {
            if let Some(sound) = event.sound.as_deref().filter(|sound| !sound.is_empty()) {
                assert!(sound.to_ascii_lowercase().ends_with(".wav"), "{event:?}");
            }
        }
    }
}
