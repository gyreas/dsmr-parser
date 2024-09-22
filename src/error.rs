//! # Error handling types
//! note that this can be done a lot easier using [thiserror](https://docs.rs/thiserror/)
//! please check that out if you'd like! You are allowed (and encouraged) to use that.
#[allow(unused_imports, clippy::single_component_path_imports)]
use thiserror;

use std::{error::Error, fmt::Display, io};

use tudelft_dsmr_output_generator::PlotError;


#[derive(Debug)]
pub enum MainError {
    IoError(io::Error),
    PlotError(PlotError),
}

// Define how to print out the error when it occurs based on the type of error it is
impl Display for MainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MainError::IoError(e) => write!(f, "IO Error Occured: {e}"),
            MainError::PlotError(e) => write!(f, "Plot Error Occured: {e}"),
        }
    }
}

// Mark the fact MainError is an Error
impl Error for MainError {}

// Allow converting io:Error to MainError,
// which allows the use of the '?' operator to automatically convert this
impl From<io::Error> for MainError {
    fn from(value: io::Error) -> Self {
        MainError::IoError(value)
    }
}

// Allow seamlessly converting PlotError to MainError
impl From<PlotError> for MainError {
    fn from(value: PlotError) -> Self {
        MainError::PlotError(value)
    }
}
