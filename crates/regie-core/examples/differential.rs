//! Reads `hex-pattern hex-input` lines from stdin and prints the match span.
//!
//! This is the helper used by `tools/differential_test.py`, which compares
//! the engine against another implementation of the same semantics.

use std::io::{self, BufRead};

use regie_core::Regex;

fn decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(hex.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let hi = (bytes[index] as char).to_digit(16)?;
        let lo = (bytes[index + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        index += 2;
    }
    Some(out)
}

fn main() {
    let stdin = io::stdin();
    let mut failures = 0usize;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let pattern_hex = parts.next().unwrap_or("");
        let input_hex = parts.next().unwrap_or("");
        let pattern = match decode(pattern_hex) {
            Some(bytes) => bytes,
            None => continue,
        };
        let input = decode(input_hex).unwrap_or_default();
        let pattern = String::from_utf8_lossy(&pattern).into_owned();
        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(_) => {
                println!("ERR");
                failures += 1;
                continue;
            }
        };
        match re.find(&input) {
            Some(hit) => println!("{} {}", hit.start, hit.end),
            None => println!("-"),
        }
    }
    if failures > 0 {
        eprintln!("{failures} patterns failed to compile");
        std::process::exit(1);
    }
}
