"""One-off helper: add serde derives to the specification types."""

import re
import pathlib

SPEC = pathlib.Path(
    r"C:\Users\danalec\Documents\src\regie\crates\regie-core\src\spec.rs"
)
SMALLVEC = pathlib.Path(
    r"C:\Users\danalec\Documents\src\regie\crates\regie-core\src\smallvec.rs"
)

SERDE = '#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]\n'

TYPES = [
    "pub enum Strictness",
    "pub enum CharsetPolicy",
    "pub enum ValidationKind",
    "pub enum ExtractionKind",
    "pub enum BoundaryPolicy",
    "pub enum CapturePolicy",
    "pub enum FilterAction",
    "pub enum TermKind",
    "pub struct PatternTerm",
    "pub enum TemplatePiece",
    "pub enum PatternTemplate",
    "pub struct PatternParams",
    "pub enum AmbiguityReason",
    "pub enum CandidateSpec",
    "pub enum IntentSpec",
]


def patch_spec() -> None:
    text = SPEC.read_text(encoding="utf-8")
    for decl in TYPES:
        marker = decl + " {"
        if marker not in text:
            raise SystemExit(f"marker not found: {decl}")
        if SERDE.strip() in text.split(marker)[0][-200:]:
            continue
        text = text.replace(marker, SERDE + marker, 1)
    SPEC.write_text(text, encoding="utf-8", newline="\n")
    print(f"patched {SPEC}")


SMALLVEC_IMPLS = '''
#[cfg(feature = "serde")]
impl<T: serde::Serialize + Copy + Default, const N: usize> serde::Serialize for SmallVec<T, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for item in self.iter() {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de> + Copy + Default, const N: usize> serde::Deserialize<'de>
    for SmallVec<T, N>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T, const N: usize>(core::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de> + Copy + Default, const N: usize> serde::de::Visitor<'de>
            for Visitor<T, N>
        {
            type Value = SmallVec<T, N>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "a sequence of at most {N} elements")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = SmallVec::new();
                while let Some(item) = seq.next_element::<T>()? {
                    if !out.push(item) {
                        return Err(serde::de::Error::custom("too many elements"));
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_seq(Visitor::<T, N>(core::marker::PhantomData))
    }
}
'''


def patch_smallvec() -> None:
    text = SMALLVEC.read_text(encoding="utf-8")
    if "impl<'de, T: serde::Deserialize<'de>" in text:
        print("smallvec already patched")
        return
    text = text.rstrip("\n") + "\n" + SMALLVEC_IMPLS
    SMALLVEC.write_text(text, encoding="utf-8", newline="\n")
    print(f"patched {SMALLVEC}")


if __name__ == "__main__":
    patch_spec()
    patch_smallvec()
