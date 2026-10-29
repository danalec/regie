//! The numbers of the micro-model itself.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example model_profile
//! ```
//!
//! The model is a table of quantized integers plus the vocabulary it looks
//! words up in.  Every number below is measured at run time, from the tables
//! that are linked into the binary, so the profile cannot drift away from the
//! model that actually runs.

use regie_intent::weights;
use regie_intent::IntentModel;

/// The size of the budget the project set for the model.
const BUDGET_BYTES: usize = 256 * 1024;

fn main() {
    // The fat pointer of a `&str` is two machine words; the vocabulary and the
    // class names are linked just like the strings they point at.
    let pointer = core::mem::size_of::<&str>();
    let class_names: usize = weights::CLASS_NAMES.iter().map(|name| name.len()).sum();
    let vocabulary: usize = weights::VOCAB.iter().map(|feature| feature.len()).sum();
    let pointers = (weights::CLASS_NAMES.len() + weights::VOCAB.len()) * pointer;
    let footprint = IntentModel::weight_bytes() + class_names + vocabulary + pointers;

    let buckets = weights::VOCAB.iter().filter(|f| f.starts_with('#')).count();
    let bigrams = weights::VOCAB.iter().filter(|f| f.contains(' ')).count();
    let words = weights::VOCAB.len() - buckets - bigrams;

    println!("model: regie-intent, embedded and quantized");
    println!("  intent classes:   {}", IntentModel::class_count());
    println!("    validate:       {}", count("validate:"));
    println!("    extract:        {}", count("extract:"));
    println!("    filter:         {}", count("filter"));
    println!("    custom:         {}", count("custom"));
    println!("  lexical features: {}", IntentModel::feature_count());
    println!("    words:          {words}");
    println!("    bigrams:        {bigrams}");
    println!("    shape buckets:  {buckets}");
    println!("  weight table:     {} bytes", IntentModel::weight_bytes());
    println!(
        "    i8 weights:     {} bytes",
        weights::CLASS_COUNT * weights::FEATURE_COUNT
    );
    println!(
        "    i32 biases:     {} bytes",
        weights::CLASS_COUNT * core::mem::size_of::<i32>()
    );
    println!(
        "  class names:      {} entries, {class_names} bytes",
        weights::CLASS_NAMES.len()
    );
    println!(
        "  vocabulary:       {} entries, {vocabulary} bytes",
        weights::VOCAB.len()
    );
    println!("  fat pointers:     {pointers} bytes ({pointer} bytes per &str)");
    println!("  linked footprint: {footprint} bytes");
    println!("  budget:           {BUDGET_BYTES} bytes");
    println!(
        "  a class wins when the score reaches {} and the margin reaches {}",
        weights::MIN_SCORE,
        weights::MARGIN_THRESHOLD
    );
}

/// The number of class names that carry `prefix`.
fn count(prefix: &str) -> usize {
    weights::CLASS_NAMES
        .iter()
        .filter(|name| name.starts_with(prefix))
        .count()
}
