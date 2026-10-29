//! Fuzz-style properties: no input may ever make the decoder panic.

use proptest::prelude::*;
use regie_codec::{decode, encode, encoded_len};
use regie_core::dfa::Dfa;

/// A small automaton with two classes and three states.
fn sample_dfa() -> Dfa {
    let mut byte_class = [0u8; 256];
    for byte in b"ab" {
        byte_class[*byte as usize] = 1;
    }
    let transitions = [0u16, 0, 0, 2, 0, 0];
    let accept = [0b0000_0100u8];
    Dfa::from_parts(2, 3, &byte_class, &accept, &transitions).unwrap()
}

proptest! {
    /// Any byte slice is either decoded or rejected with an error, but never
    /// panics.
    #[test]
    fn decode_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = decode(&bytes);
    }

    /// A truncated artifact is always rejected, whatever the cut point is.
    #[test]
    fn a_truncated_artifact_is_always_rejected(cut in 0usize..1024) {
        let dfa = sample_dfa();
        let mut artifact = vec![0u8; encoded_len(&dfa)];
        encode(&dfa, &mut artifact).unwrap();

        let cut = cut % artifact.len();
        prop_assert!(decode(&artifact[..cut]).is_err());
    }

    /// Flipping a single bit of an artifact is always detected.
    #[test]
    fn a_single_bit_flip_is_always_detected(index in 0usize..1024, bit in 0u8..8) {
        let dfa = sample_dfa();
        let mut artifact = vec![0u8; encoded_len(&dfa)];
        encode(&dfa, &mut artifact).unwrap();

        let index = index % artifact.len();
        let mut corrupted = artifact.clone();
        corrupted[index] ^= 1 << bit;
        prop_assert!(decode(&corrupted).is_err());
    }

    /// Encoding is deterministic and the artifact always decodes back into an
    /// equivalent automaton.
    #[test]
    fn encoding_is_deterministic_and_reversible(
        classes in 1u16..=4,
        states in 1u16..=6,
    ) {
        let states = states as usize;
        let classes = classes as usize;

        let mut byte_class = [0u8; 256];
        for (byte, slot) in byte_class.iter_mut().enumerate() {
            *slot = (byte % classes) as u8;
        }

        let mut accept = vec![0u8; states.div_ceil(8)];
        for state in 0..states {
            accept[state / 8] |= 1u8 << (state % 8);
        }

        let mut transitions = vec![0u16; states * classes];
        for state in 0..states {
            for class in 0..classes {
                transitions[state * classes + class] = ((state + class + 1) % states) as u16;
            }
        }

        let dfa = Dfa::from_parts(classes as u16, states as u16, &byte_class, &accept, &transitions)
            .unwrap();

        let mut first = vec![0u8; encoded_len(&dfa)];
        let mut second = vec![0u8; encoded_len(&dfa)];
        encode(&dfa, &mut first).unwrap();
        encode(&dfa, &mut second).unwrap();
        prop_assert_eq!(&first, &second);

        let restored = decode(&first).unwrap();
        prop_assert_eq!(restored.state_count(), dfa.state_count());
        prop_assert_eq!(restored.class_count(), dfa.class_count());
        prop_assert_eq!(restored.byte_classes(), dfa.byte_classes());
        prop_assert_eq!(restored.transitions(), dfa.transitions());
    }
}
