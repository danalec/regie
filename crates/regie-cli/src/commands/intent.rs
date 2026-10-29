//! The `intent` subcommand.
//!
//! The subcommand interprets a natural-language request with the embedded
//! micro-model of `regie-intent` and prints the resulting specification.
//!
//! The output has two lines:
//!
//! 1. `summary: <one line of prose>`, the human readable form;
//! 2. the specification as a single line of JSON.
//!
//! The JSON line is the machine readable one: it is stable, and the shape is
//! documented in [`crate::render`].

use crate::render;
use crate::{CliError, EXIT_NO_MATCH, EXIT_OK};
use regie_intent::IntentModel;

/// Runs the `intent` subcommand.
///
/// Returns [`EXIT_OK`] when the model settled on one specification, and
/// [`EXIT_NO_MATCH`] when the request stayed ambiguous.
pub fn run_intent(request: &str) -> Result<u8, CliError> {
    let model = IntentModel::embedded();
    let spec = model.interpret(request);

    println!("summary: {}", render::intent_summary(&spec));
    match serde_json::to_string(&render::intent_json(&spec)) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            return Err(CliError::Usage(format!(
                "could not render the specification as JSON: {error}"
            )))
        }
    }

    Ok(if spec.is_confident() {
        EXIT_OK
    } else {
        EXIT_NO_MATCH
    })
}
