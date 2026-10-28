//! Per-user system parameters read and set through the system's own calls,
//! for the tweaks that change them: `SystemParametersInfo` for animation flags
//! Microsoft documents, and `SHGetSetSettings` for four shell settings that
//! File Explorer keeps twice. Only on/off parameters are listed. Setting a
//! `SystemParametersInfo` one saves it to the user profile and tells running
//! programs, as the Performance Options dialog does, so the change shows at
//! once and stays after signing out.

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
    /// File name extensions are shown. This one and the next three are shell
    /// settings that File Explorer keeps in two places: the Advanced registry
    /// values its folder windows read, and a copy of its own state that the
    /// Folder Options dialog loads and writes back when it closes. Changing
    /// only the registry value lets that dialog undo the change, so a tweak
    /// sets these through the shell as well.
    ShowExtensions,
    /// Hidden files and folders are shown.
    ShowAllObjects,
    /// Items have check boxes for selecting them.
    AutoCheckSelect,
    /// Files show plain icons instead of thumbnails.
    IconsOnly,
}

impl Parameter {
    /// The system call and setting that sets it, as Microsoft names them.
    pub fn action_name(self) -> &'static str {
        match self {
            Parameter::ClientAreaAnimation => "SPI_SETCLIENTAREAANIMATION",
            Parameter::MinimizeAnimation => "SPI_SETANIMATION",
            Parameter::MenuAnimation => "SPI_SETMENUANIMATION",
            Parameter::TooltipAnimation => "SPI_SETTOOLTIPANIMATION",
            Parameter::ComboBoxAnimation => "SPI_SETCOMBOBOXANIMATION",
            Parameter::ListBoxSmoothScrolling => "SPI_SETLISTBOXSMOOTHSCROLLING",
            Parameter::SelectionFade => "SPI_SETSELECTIONFADE",
            Parameter::ShowExtensions => "SSF_SHOWEXTENSIONS",
            Parameter::ShowAllObjects => "SSF_SHOWALLOBJECTS",
            Parameter::AutoCheckSelect => "SSF_AUTOCHECKSELECT",
            Parameter::IconsOnly => "SSF_ICONSONLY",
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

    use windows::Win32::UI::Shell::{
        SHELLSTATEA, SHGetSetSettings, SSF_AUTOCHECKSELECT, SSF_ICONSONLY, SSF_MASK,
        SSF_SHOWALLOBJECTS, SSF_SHOWEXTENSIONS,
    };
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

    /// How a parameter is read and set.
    enum Kind {
        /// A plain BOOL: the get and set actions of `SystemParametersInfo`.
        Bool(SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_ACTION),
        /// The minimize animation, which goes through an ANIMATIONINFO.
        MinimizeAnimation,
        /// A flag of the shell's own state, a bit of one of the two bit
        /// fields of `SHELLSTATE`.
        Shell {
            mask: SSF_MASK,
            second_field: bool,
            bit: u32,
        },
    }

    fn kind(parameter: Parameter) -> Kind {
        match parameter {
            Parameter::ClientAreaAnimation => {
                Kind::Bool(SPI_GETCLIENTAREAANIMATION, SPI_SETCLIENTAREAANIMATION)
            }
            Parameter::MenuAnimation => Kind::Bool(SPI_GETMENUANIMATION, SPI_SETMENUANIMATION),
            Parameter::TooltipAnimation => {
                Kind::Bool(SPI_GETTOOLTIPANIMATION, SPI_SETTOOLTIPANIMATION)
            }
            Parameter::ComboBoxAnimation => {
                Kind::Bool(SPI_GETCOMBOBOXANIMATION, SPI_SETCOMBOBOXANIMATION)
            }
            Parameter::ListBoxSmoothScrolling => {
                Kind::Bool(SPI_GETLISTBOXSMOOTHSCROLLING, SPI_SETLISTBOXSMOOTHSCROLLING)
            }
            Parameter::SelectionFade => Kind::Bool(SPI_GETSELECTIONFADE, SPI_SETSELECTIONFADE),
            Parameter::MinimizeAnimation => Kind::MinimizeAnimation,
            // The bits follow the declaration of SHELLSTATE in shlobj.h.
            Parameter::ShowAllObjects => Kind::Shell {
                mask: SSF_SHOWALLOBJECTS,
                second_field: false,
                bit: 0,
            },
            Parameter::ShowExtensions => Kind::Shell {
                mask: SSF_SHOWEXTENSIONS,
                second_field: false,
                bit: 1,
            },
            Parameter::AutoCheckSelect => Kind::Shell {
                mask: SSF_AUTOCHECKSELECT,
                second_field: true,
                bit: 3,
            },
            Parameter::IconsOnly => Kind::Shell {
                mask: SSF_ICONSONLY,
                second_field: true,
                bit: 4,
            },
        }
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
        match kind(parameter) {
            Kind::Bool(action, _) => {
                let mut value = BOOL(0);
                // SAFETY: the action writes one BOOL through the pointer,
                // which points at `value` for the whole call.
                unsafe {
                    SystemParametersInfoW(
                        action,
                        0,
                        Some((&raw mut value).cast()),
                        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                    )
                }
                .map_err(|error| failed(parameter, error))?;
                Ok(value.as_bool())
            }
            Kind::MinimizeAnimation => {
                let mut info = animation_info(false);
                // SAFETY: SPI_GETANIMATION fills the ANIMATIONINFO whose size
                // it is given in both uiParam and cbSize.
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
            Kind::Shell {
                mask,
                second_field,
                bit,
            } => {
                let mut state = SHELLSTATEA::default();
                // SAFETY: the call fills in the bits named by the mask in the
                // struct behind the pointer, which lives for the whole call.
                unsafe { SHGetSetSettings(Some(&raw mut state), mask, false) };
                // Fields of a packed struct are read by value.
                let field = if second_field {
                    state._bitfield2
                } else {
                    state._bitfield1
                };
                Ok(field & (1 << bit) != 0)
            }
        }
    }

    pub fn set(parameter: Parameter, on: bool) -> Result<(), ParameterError> {
        let update = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
        match kind(parameter) {
            Kind::Bool(_, action) => {
                // These actions take the BOOL itself in pvParam, not a pointer
                // to it.
                let value = usize::from(on) as *mut c_void;
                // SAFETY: no memory is passed; pvParam carries the value.
                unsafe { SystemParametersInfoW(action, 0, Some(value), update) }
                    .map_err(|error| failed(parameter, error))
            }
            Kind::MinimizeAnimation => {
                let mut info = animation_info(on);
                // SAFETY: SPI_SETANIMATION reads the ANIMATIONINFO whose size
                // it is given in both uiParam and cbSize.
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
            Kind::Shell {
                mask,
                second_field,
                bit,
            } => {
                let mut state = SHELLSTATEA::default();
                let value = if on { 1 << bit } else { 0 };
                if second_field {
                    state._bitfield2 = value;
                } else {
                    state._bitfield1 = value;
                }
                // SAFETY: the call reads the bits named by the mask from the
                // struct behind the pointer, which lives for the whole call.
                // It returns nothing; the caller reads the flag back.
                unsafe { SHGetSetSettings(Some(&raw mut state), mask, true) };
                Ok(())
            }
        }
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    const ALL: [Parameter; 11] = [
        Parameter::ClientAreaAnimation,
        Parameter::MinimizeAnimation,
        Parameter::MenuAnimation,
        Parameter::TooltipAnimation,
        Parameter::ComboBoxAnimation,
        Parameter::ListBoxSmoothScrolling,
        Parameter::SelectionFade,
        Parameter::ShowExtensions,
        Parameter::ShowAllObjects,
        Parameter::AutoCheckSelect,
        Parameter::IconsOnly,
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
        assert_eq!(
            serde_json::to_string(&Parameter::ShowExtensions).unwrap(),
            r#""show_extensions""#
        );
    }
}
