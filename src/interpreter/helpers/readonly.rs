//! Readonly and export variable helpers.
//!
//! Consolidates readonly and export variable logic used in declare, export, local, etc.

use std::collections::HashSet;
use crate::interpreter::types::InterpreterState;
use crate::interpreter::errors::ExitError;

/// Mark a variable as readonly.
pub fn mark_readonly(state: &mut InterpreterState, name: &str) {
    if state.readonly_vars.is_none() {
        state.readonly_vars = Some(HashSet::new());
    }
    state.readonly_vars.as_mut().unwrap().insert(name.to_string());
}

/// Check if a variable is readonly.
pub fn is_readonly(state: &InterpreterState, name: &str) -> bool {
    state.readonly_vars.as_ref().map_or(false, |vars| vars.contains(name))
}

/// Check if a variable is readonly and return an error if so.
/// Returns Ok(()) if the variable is not readonly (can be modified).
/// Returns Err with ExitError if variable is readonly.
///
/// Assigning to a readonly variable is a fatal error that stops script execution.
/// This matches the behavior of dash, mksh, ash, and bash in POSIX mode.
pub fn check_readonly_error(
    state: &InterpreterState,
    name: &str,
    command: &str,
) -> Result<(), ExitError> {
    if is_readonly(state, name) {
        let stderr = format!("{}: {}: readonly variable\n", command, name);
        return Err(ExitError::new(1, String::new(), stderr));
    }
    Ok(())
}

/// Mark a variable as exported.
///
/// If we're inside a local scope and the variable is local (exists in the
/// current scope), track it as a locally-exported variable. When the scope
/// is popped, the export attribute will be removed if it wasn't exported
/// before entering the function.
pub fn mark_exported(state: &mut InterpreterState, name: &str) {
    let was_exported = state.exported_vars.as_ref().map_or(false, |vars| vars.contains(name));

    if state.exported_vars.is_none() {
        state.exported_vars = Some(HashSet::new());
    }
    state.exported_vars.as_mut().unwrap().insert(name.to_string());

    // If we're in a local scope and the variable is local, track it
    if !state.local_scopes.is_empty() {
        let current_scope = state.local_scopes.last().unwrap();
        // Only track if: the variable is local AND it wasn't already exported before
        if current_scope.contains_key(name) && !was_exported {
            // Initialize local_exported_vars stack if needed
            if state.local_exported_vars.is_none() {
                state.local_exported_vars = Some(Vec::new());
            }
            let local_exported = state.local_exported_vars.as_mut().unwrap();

            // Ensure we have a set for the current scope depth
            while local_exported.len() < state.local_scopes.len() {
                local_exported.push(HashSet::new());
            }

            // Track this variable as locally exported
            if let Some(last) = local_exported.last_mut() {
                last.insert(name.to_string());
            }
        }
    }
}

/// Remove the export attribute from a variable.
/// The variable value is preserved, just no longer exported to child processes.
pub fn unmark_exported(state: &mut InterpreterState, name: &str) {
    if let Some(ref mut vars) = state.exported_vars {
        vars.remove(name);
    }
}

/// Check if a variable is exported.
pub fn is_exported(state: &InterpreterState, name: &str) -> bool {
    state.exported_vars.as_ref().map_or(false, |vars| vars.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_readonly() {
        let mut state = InterpreterState::default();
        assert!(!is_readonly(&state, "FOO"));

        mark_readonly(&mut state, "FOO");
        assert!(is_readonly(&state, "FOO"));
        assert!(!is_readonly(&state, "BAR"));
    }

    #[test]
    fn test_check_readonly_error() {
        let mut state = InterpreterState::default();

        // Not readonly - should succeed
        assert!(check_readonly_error(&state, "FOO", "bash").is_ok());

        // Mark as readonly
        mark_readonly(&mut state, "FOO");

        // Now should fail
        let result = check_readonly_error(&state, "FOO", "bash");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.exit_code, 1);
        assert!(err.stderr.contains("readonly variable"));
    }

    #[test]
    fn test_mark_exported() {
        let mut state = InterpreterState::default();
        assert!(!is_exported(&state, "FOO"));

        mark_exported(&mut state, "FOO");
        assert!(is_exported(&state, "FOO"));
        assert!(!is_exported(&state, "BAR"));
    }

    #[test]
    fn test_unmark_exported() {
        let mut state = InterpreterState::default();

        mark_exported(&mut state, "FOO");
        assert!(is_exported(&state, "FOO"));

        unmark_exported(&mut state, "FOO");
        assert!(!is_exported(&state, "FOO"));
    }
}
