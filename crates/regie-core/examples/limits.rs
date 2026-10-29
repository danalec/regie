//! The budgets of this build, and exactly what the engine does when one of
//! them is exceeded.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example limits
//! ```

use regie_core::{Regex, MAX_CAPTURE_GROUPS, MAX_PATTERN_LEN};

fn main() {
    println!("the budgets of this build:");
    println!("  the length of a pattern: {MAX_PATTERN_LEN} bytes");
    println!("  the number of groups:    {MAX_CAPTURE_GROUPS}");
    println!();

    pattern_length();
    capture_groups();
    program_size();
    repetition_bound();
    nesting_depth();
}

/// The length of a pattern is checked before anything else.
fn pattern_length() {
    println!("the length of a pattern:");
    let accepted = "a{3}".repeat(128);
    let refused = "a{3}".repeat(128) + "a";
    println!("  {} bytes -> {}", accepted.len(), outcome(&accepted));
    println!("  {} bytes -> {}", refused.len(), outcome(&refused));
}

/// The number of capture groups that the engine accepts is measured, because
/// the budget and the compiler do not agree on where it ends.
fn capture_groups() {
    println!("the number of capture groups:");
    let mut count = 1usize;
    let mut accepted = 0usize;
    let refused = loop {
        let pattern = "(a)".repeat(count);
        match Regex::new(&pattern) {
            Ok(_) => {
                accepted = count;
                count += 1;
            }
            Err(err) => break err,
        }
    };
    println!("  {accepted} groups -> compiled");
    println!("  {count} groups -> refused: {refused}");
}

/// The instruction budget is not a public constant, so it is measured: the
/// program grows by one instruction per repeated byte.
fn program_size() {
    println!("the size of the compiled program:");
    let mut count = 1usize;
    let mut accepted = String::new();
    let refused = loop {
        let pattern = format!("(?:a{{{count}}}){{3}}");
        match Regex::new(&pattern) {
            Ok(_) => {
                accepted = pattern;
                count += 1;
            }
            Err(err) => break err,
        }
    };
    println!("  {accepted} -> compiled");
    println!("  (?:a{{{count}}}){{3}} -> refused: {refused}");
}

/// The upper bound of a repetition is 255.
fn repetition_bound() {
    println!("the upper bound of a repetition:");
    println!("  a{{255}} -> {}", outcome("a{255}"));
    println!("  a{{256}} -> {}", outcome("a{256}"));
}

/// The groups may not nest deeper than 32 levels.
///
/// The groups are not capturing here: the budget of the capture groups would
/// be reached first.
fn nesting_depth() {
    println!("the depth of the nesting:");
    let shallow = format!("{}a{}", "(?:".repeat(32), ")".repeat(32));
    let deep = format!("{}a{}", "(?:".repeat(33), ")".repeat(33));
    println!("  32 nested groups -> {}", outcome(&shallow));
    println!("  33 nested groups -> {}", outcome(&deep));
}

/// What the engine does with a pattern: it compiles, or it refuses and says
/// why.
fn outcome(pattern: &str) -> String {
    match Regex::new(pattern) {
        Ok(_) => "compiled".to_owned(),
        Err(err) => err.to_string(),
    }
}
