//! What the engine needs from the system parameters, so its logic can be
//! tested against an in-memory stand-in and run against the real ones.

use platform::parameters::{self as spi, Parameter, ParameterError};

use crate::registry::CurrentUser;

/// Per-user on/off parameters set through SystemParametersInfo.
pub trait Parameters {
    fn get(&self, parameter: Parameter) -> Result<bool, ParameterError>;
    /// Sets the parameter, saves it to the user profile and announces it.
    fn set(&mut self, parameter: Parameter, on: bool) -> Result<(), ParameterError>;
}

impl Parameters for CurrentUser {
    fn get(&self, parameter: Parameter) -> Result<bool, ParameterError> {
        spi::get(parameter)
    }

    fn set(&mut self, parameter: Parameter, on: bool) -> Result<(), ParameterError> {
        spi::set(parameter, on)
    }
}
