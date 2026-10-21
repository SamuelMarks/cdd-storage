//! Binary entry point for the `cdd-storage` microservice.

use cdd_storage::error::AppError;
use cdd_storage::run_with_vars;
use std::collections::HashMap;
use std::env;

/// Main entry point for the `cdd-storage` binary.
///
/// Reads configuration from process environment variables and starts the Actix Web HTTP server.
///
/// # Returns
///
/// Returns `Ok(())` when the server terminates cleanly.
///
/// # Errors
///
/// Returns an [`AppError`] if startup configuration or server execution fails.
#[actix_web::main]
async fn main() -> Result<(), AppError> {
    let mut vars: HashMap<String, String> = env::vars().collect();
    if cfg!(test) {
        vars.insert(String::from("PORT"), String::from("0"));
    }
    run_with_vars(&vars, cfg!(test)).await
}

#[cfg(test)]
/// Unit tests for the binary entry point.
mod tests {
    use super::*;

    /// Verifies that the [`main`] entry point initializes the Actix runtime, binds to an
    /// ephemeral port, and shuts down cleanly.
    #[test]
    fn test_main_entrypoint() -> Result<(), AppError> {
        main()
    }
}
