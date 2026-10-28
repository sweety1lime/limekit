//! Per-user system parameters read and set through `SystemParametersInfo`,
//! for the tweaks that change them. Only on/off parameters are listed, each
//! one documented by Microsoft. Setting one saves it to the user profile and
//! tells running programs, as the Performance Options dialog does, so the
//! change shows at once and stays after signing out.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The names are what the tweak catalog and journal call the parameters, so
/// they stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    /// Animations and other transient effects inside windows.
    ClientAreaAnimation,
    /// Windows animate when minimized and maximized.
    MinimizeAnimation,
    /// Menus fade or slide into view.
    MenuAnimation,
    /// ToolTips fade or slide into view.
    TooltipAnimation,
    /// Combo boxes slide open.
    ComboBoxAnimation,
    /// List boxes scroll smoothly.
    ListBoxSmoothScrolling,
    /// A clicked menu item fades out.
    SelectionFade,
}

impl Parameter {
    /// The `SystemParametersInfo` action that sets it, as Microsoft names it.
    pub fn action_name(self) -> &'static str {
        match self {
            Parameter::ClientAreaAnimation => "SPI_SETCLIENTAREAANIMATION",
            Parameter::MinimizeAnimation => "SPI_SETANIMATION",
            Parameter::MenuAnimation => "SPI_SETMENUANIMATION",
            Parameter::TooltipAnimation => "SPI_SETTOOLTIPANIMATION",
            Parameter::ComboBoxAnimation => "SPI_SETCOMBOBOXANIMATION",
            Parameter::ListBoxSmoothScrolling => "SPI_SETLISTBOXSMOOTHSCROLLING",
            Parameter::SelectionFade => "SPI_SETSELECTIONFADE",
        }
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.action_name())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{parameter}: {message}")]
pub struct ParameterError {
    pub parameter: Parameter,
    pub message: String,
}

/// Whether `parameter` is on.
pub fn get(parameter: Parameter) -> Result<bool, ParameterError> {
    #[cfg(windows)]
    {
        win32::get(parameter)
    }
    #[cfg(not(windows))]
    {
        Err(unsupported(parameter))
    }
}

/// Turns `parameter` on or off for this user and announces the change.
pub fn set(parameter: Parameter, on: bool) -> Result<(), ParameterError> {
    #[cfg(windows)]
    {
        win32::set(parameter, on)
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        Err(unsupported(parameter))
    }
}

#[cfg(not(windows))]
fn unsupported(parameter: Parameter) -> ParameterError {
    ParameterError {
        parameter,
        message: "system parameters exist only on Windows".into(),
    }
}

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    use windows::Win32::UI::WindowsAndMessaging::{
        ANIMATIONINFO, SPI_GETANIMATION, SPI_GETCLIENTAREAANIMATION, SPI_GETCOMBOBOXANIMATION,
        SPI_GETLISTBOXSMOOTHSCROLLING, SPI_GETMENUANIMATION, SPI_GETSELECTIONFADE,
        SPI_GETTOOLTIPANIMATION, SPI_SETANIMATION, SPI_SETCLIENTAREAANIMATION,
        SPI_SETCOMBOBOXANIMATION, SPI_SETLISTBOXSMOOTHSCROLLING, SPI_SETMENUANIMATION,
        SPI_SETSELECTIONFADE, SPI_SETTOOLTIPANIMATION, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
        SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    use windows::core::BOOL;

    use super::{Parameter, ParameterError};

    /// The get and set actions of a parameter that is a plain BOOL.
    fn flag(
        parameter: Parameter,
    ) -> Option<(SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_ACTION)> {
        Some(match parameter {
            Parameter::ClientAreaAnimation => {
                (SPI_GETCLIENTAREAANIMATION, SPI_SETCLIENTAREAANIMATION)
            }
            Parameter::MenuAnimation => (SPI_GETMENUANIMATION, SPI_SETMENUANIMATION),
            Parameter::TooltipAnimation => (SPI_GETTOOLTIPANIMATION, SPI_SETTOOLTIPANIMATION),
            Parameter::ComboBoxAnimation => (SPI_GETCOMBOBOXANIMATION, SPI_SETCOMBOBOXANIMATION),
            Parameter::ListBoxSmoothScrolling => {
                (SPI_GETLISTBOXSMOOTHSCROLLING, SPI_SETLISTBOXSMOOTHSCROLLING)
            }
            Parameter::SelectionFade => (SPI_GETSELECTIONFADE, SPI_SETSELECTIONFADE),
            Parameter::MinimizeAnimation => return None,
        })
    }

    fn failed(parameter: Parameter, error: windows::core::Error) -> ParameterError {
        ParameterError {
            parameter,
            message: error.to_string(),
        }
    }

    fn animation_info(on: bool) -> ANIMATIONINFO {
        ANIMATIONINFO {
            cbSize: size_of::<ANIMATIONINFO>() as u32,
            iMinAnimate: on.into(),
        }
    }

    pub fn get(parameter: Parameter) -> Result<bool, ParameterError> {
        if let Some((action, _)) = flag(parameter) {
            let mut value = BOOL(0);
            // SAFETY: the action writes one BOOL through the pointer, which
            // points at `value` for the whole call.
            unsafe {
                SystemParametersInfoW(
                    action,
                    0,
                    Some((&raw mut value).cast()),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )
            }
            .map_err(|error| failed(parameter, error))?;
            return Ok(value.as_bool());
        }
        let mut info = animation_info(false);
        // SAFETY: SPI_GETANIMATION fills the ANIMATIONINFO whose size it is
        // given in both uiParam and cbSize.
        unsafe {
            SystemParametersInfoW(
                SPI_GETANIMATION,
                info.cbSize,
                Some((&raw mut info).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| failed(parameter, error))?;
        Ok(info.iMinAnimate != 0)
    }

    pub fn set(parameter: Parameter, on: bool) -> Result<(), ParameterError> {
        let update = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
        if let Some((_, action)) = flag(parameter) {
            // These actions take the BOOL itself in pvParam, not a pointer to
            // it.
            let value = usize::from(on) as *mut c_void;
            // SAFETY: no memory is passed; pvParam carries the value.
            return unsafe { SystemParametersInfoW(action, 0, Some(value), update) }
                .map_err(|error| failed(parameter, error));
        }
        let mut info = animation_info(on);
        // SAFETY: SPI_SETANIMATION reads the ANIMATIONINFO whose size it is
        // given in both uiParam and cbSize.
        unsafe {
            SystemParametersInfoW(
                SPI_SETANIMATION,
                info.cbSize,
                Some((&raw mut info).cast()),
                update,
            )
        }
        .map_err(|error| failed(parameter, error))
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    const ALL: [Parameter; 7] = [
        Parameter::ClientAreaAnimation,
        Parameter::MinimizeAnimation,
        Parameter::MenuAnimation,
        Parameter::TooltipAnimation,
        Parameter::ComboBoxAnimation,
        Parameter::ListBoxSmoothScrolling,
        Parameter::SelectionFade,
    ];

    // Reading only: setting would change the settings of whoever runs the
    // tests.
    #[test]
    fn every_parameter_reads() {
        for parameter in ALL {
            if let Err(error) = get(parameter) {
                panic!("{error}");
            }
        }
    }

    #[test]
    fn names_read_like_the_catalog() {
        assert_eq!(
            serde_json::to_string(&Parameter::ListBoxSmoothScrolling).unwrap(),
            r#""list_box_smooth_scrolling""#
        );
    }
}
