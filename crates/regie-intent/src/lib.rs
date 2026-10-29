//! # regie-intent
//!
//! The intent layer of [`regie`](https://docs.rs/regie): a tiny local
//! micro-model that turns a natural-language request into a *structured*
//! specification.
//!
//! ## What it does
//!
//! The model answers questions like "validar um CPF", "extrair números
//! entre colchetes" or "bloquear as palavras spam e phishing" with one of
//! the enumerated [`CandidateSpec`](regie_core::spec::CandidateSpec) values:
//! a validation family, an extraction family, a filter or a custom template.
//! It never produces pattern text.  The deterministic compiler in
//! `regie-core` turns the specification into an automaton, so a model can
//! never smuggle arbitrary syntax into the engine.
//!
//! ## How it works
//!
//! * The request is normalized (case folded, accents folded, punctuation
//!   dropped) and split into tokens.
//! * The tokens, the pairs of adjacent tokens and a few coarse shape
//!   buckets form a small set of binary lexical features.
//! * Every intent class is scored with a quantized linear classifier whose
//!   weights are the log-odds ratios of a Bernoulli Naive Bayes fitted
//!   offline by `tools/train_intent.py`.  The weights are `i8` values in a
//!   `&'static` table, so scoring is a handful of integer operations.
//! * The best class wins, but only when it is far enough ahead of the
//!   runner-up.  Otherwise the layer returns
//!   [`spec::IntentSpec::Ambiguous`] and lets the caller decide.
//!
//! ## Guarantees
//!
//! * **Deterministic.** The same request always yields the same answer, on
//!   every platform: there is no randomness, no hashing and no floating
//!   point in the inference path.
//! * **Local.** No network, no GPU, no Python, no external service: the
//!   weights are compiled into the binary.
//! * **Small.** The weight table is 60 KiB and the whole model, vocabulary
//!   included, is 97 KiB - far below the 256 KiB budget the project set for
//!   it.
//! * **`no_std`.** The crate needs no allocator and forbids `unsafe`.
//!
//! ## Example
//!
//! ```
//! use regie_intent::IntentModel;
//!
//! let model = IntentModel::embedded();
//!
//! let spec = model.interpret("validar um CPF");
//! assert!(spec.is_confident());
//!
//! // A request the model does not understand is refused, not guessed.
//! assert!(!model.interpret("qwertyuiop zzzz").is_confident());
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.rs/regie-intent/0.1.0")]
#![forbid(unsafe_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod features;
mod model;

pub mod weights;

pub use model::IntentModel;
pub use regie_core::spec;
pub use regie_core::SmallVec;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

impl IntentModel {
    /// The ranked candidates of a request, as a heap allocated vector.
    ///
    /// This is the convenience form of [`IntentModel::candidates`], for
    /// callers that already work with the standard library.
    #[cfg(feature = "alloc")]
    pub fn candidates_vec(&self, request: &str) -> Vec<spec::CandidateSpec> {
        let mut out = Vec::new();
        out.extend_from_slice(self.candidates(request).as_slice());
        out
    }
}
