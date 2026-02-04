//! Error helper functions for the interpreter.

use std::error::Error;

/// Extract message from an error.
/// Handles both Error instances and other error values.
pub fn get_error_message<E: Error>(error: &E) -> String {
    error.to_string()
}

/// Extract message from a boxed error.
pub fn get_boxed_error_message(error: &Box<dyn Error>) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_get_error_message() {
        let err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        assert_eq!(get_error_message(&err), "file not found");
    }
}
