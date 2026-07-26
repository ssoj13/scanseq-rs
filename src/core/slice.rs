//! Python-style slice selector over an ordered frame sequence.
//!
//! [`FrameSlice`] parses a `start:end:step` spec (each part optional) and
//! resolves it to the 0-based POSITIONS to keep over a sequence of a given
//! length, using Python list-slicing semantics:
//!
//! - 0-indexed, **END-EXCLUSIVE** (`:100` = the first 100 positions).
//! - Negative indices count from the end (`-50:` = the last 50).
//! - Out-of-range `start`/`end` clamp to `[0, len]` (no panic).
//! - `step` defaults to 1 and MUST be `>= 1` — reverse ordering is meaningless
//!   for a frame sequence fed to SfM, so a `step <= 0` is a *parse error*, not a
//!   silent reversal.
//! - A bare number with no colon (`"3"`) is a parse ERROR: it is ambiguous
//!   ("from index 3" vs "every 3rd"), so the colon form is required.
//! - An empty spec selects EVERYTHING.
//!
//! The type is deliberately generic: [`FrameSlice::resolve`] works on any
//! length, so it is unit-testable in isolation from [`crate::Seq`], which layers
//! the frame-preserving [`Seq::select`](crate::Seq::select) on top of it.

use std::fmt;
use std::str::FromStr;

/// A parsed Python-style slice `start:end:step`. All three parts are optional;
/// resolve it against a concrete length with [`FrameSlice::resolve`].
///
/// `start`/`end` are stored pre-normalization (they may be negative — meaning
/// "from the end" — until [`resolve`](FrameSlice::resolve) is given a length).
/// `step` is normalized to `>= 1` at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSlice {
    /// Inclusive start position. `None` = 0. May be negative (counts from end).
    start: Option<i64>,
    /// Exclusive end position. `None` = len. May be negative (counts from end).
    end: Option<i64>,
    /// Stride between kept positions; always `>= 1` (enforced at parse time).
    step: usize,
}

/// Error from parsing a [`FrameSlice`] spec via [`FrameSlice::from_str`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameSliceError {
    /// A bare number with no `:` — ambiguous (index vs stride); require the colon form.
    BareNumber,
    /// More than three `:`-separated parts (`a:b:c:d`).
    TooManyParts(usize),
    /// A part is not a valid integer (holds the offending token).
    NotInteger(String),
    /// `step <= 0` (holds the offending value). Reverse/zero stride is rejected.
    NonPositiveStep(i64),
}

impl fmt::Display for FrameSliceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameSliceError::BareNumber => write!(
                f,
                "bare number is ambiguous — use the Python slice form with a colon \
                 (e.g. `3:` from index 3, `:3` first 3, or `::3` every 3rd)"
            ),
            FrameSliceError::TooManyParts(n) => {
                write!(f, "too many `:`-separated parts ({n}); expected at most 3 (start:end:step)")
            }
            FrameSliceError::NotInteger(tok) => {
                write!(f, "`{tok}` is not an integer")
            }
            FrameSliceError::NonPositiveStep(s) => {
                write!(f, "step must be >= 1 (got {s}); a reverse/zero stride is not allowed")
            }
        }
    }
}

impl std::error::Error for FrameSliceError {}

impl FromStr for FrameSlice {
    type Err = FrameSliceError;

    /// Parse `"a:b:c"` / `"a:b"` / `"::c"` / … into a [`FrameSlice`].
    ///
    /// Whitespace is trimmed; an empty spec selects everything. See the module
    /// docs for the full grammar and the [`FrameSliceError`] variants.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        // Empty spec => select all.
        if s.is_empty() {
            return Ok(FrameSlice { start: None, end: None, step: 1 });
        }

        let parts: Vec<&str> = s.split(':').collect();
        // A single token with NO colon (`"3"`) is ambiguous — reject it loudly.
        if parts.len() == 1 {
            return Err(FrameSliceError::BareNumber);
        }
        if parts.len() > 3 {
            return Err(FrameSliceError::TooManyParts(parts.len()));
        }

        // Parse an optional integer part: empty => None, else i64 or error.
        let parse_opt = |tok: &str| -> Result<Option<i64>, FrameSliceError> {
            let tok = tok.trim();
            if tok.is_empty() {
                Ok(None)
            } else {
                tok.parse::<i64>()
                    .map(Some)
                    .map_err(|_| FrameSliceError::NotInteger(tok.to_string()))
            }
        };

        let start = parse_opt(parts[0])?;
        let end = parse_opt(parts[1])?;
        // Step: absent (2-part form) or empty 3rd part => 1; else must be >= 1.
        let step = match parts.get(2).copied() {
            None => 1,
            Some(tok) => match parse_opt(tok)? {
                None => 1,
                Some(v) if v <= 0 => return Err(FrameSliceError::NonPositiveStep(v)),
                Some(v) => v as usize,
            },
        };

        Ok(FrameSlice { start, end, step })
    }
}

impl FrameSlice {
    /// The uniform decode-time STRIDE this slice represents, if any: `Some(step)` iff it is a bare
    /// `::step` (no start, no end) with `step > 1`. That is the ONLY slice shape a streaming decoder
    /// can apply NATIVELY — it has a frame stride but no start offset and no from-end fold — so a
    /// caller (e.g. video ingest) can push `::N` straight into the decoder and skip the post-decode
    /// re-slice. Every other shape (any start/end, negative index, or `step <= 1`) returns `None`
    /// and must be applied as a normal post-decode [`Self::resolve`]/select.
    #[must_use]
    pub fn as_uniform_stride(&self) -> Option<usize> {
        (self.start.is_none() && self.end.is_none() && self.step > 1).then_some(self.step)
    }

    /// Resolve to the ascending 0-based positions to KEEP over a sequence of
    /// `len` items, implementing Python's `slice(start, end, step).indices(len)`
    /// for a positive step: a negative index adds `len` ONCE, then the result is
    /// clamped into `[0, len]`; iteration is `start..end` (end-exclusive).
    ///
    /// TOTAL: never panics for ANY `i64` `start`/`end` and ANY `len` (including
    /// 0). The negative fold uses `saturating_add` so even `i64::MIN` clamps to
    /// 0 instead of overflowing, and an out-of-range or empty (`start >= end`)
    /// slice yields an empty `Vec`.
    #[must_use]
    pub fn resolve(&self, len: usize) -> Vec<usize> {
        // `usize::MAX` would cast to a negative i64; clamp so `len_i >= 0` holds
        // (a frame count never approaches this, but keep `resolve` total anyway).
        let len_i = i64::try_from(len).unwrap_or(i64::MAX);
        // Guard the cast: parse bounds `step` to a positive i64, but a
        // hand-built `FrameSlice` could hold `usize::MAX` (which `as i64` would
        // turn NEGATIVE); `try_from` keeps it a positive i64 so the loop only
        // ever advances.
        let step = i64::try_from(self.step).unwrap_or(i64::MAX).max(1);

        // Python `slice.indices(len)` normalization for a POSITIVE step: fold a
        // negative index from the end (add `len` once, `saturating_add` so a
        // huge-negative index can't overflow), then clamp into `[0, len]`.
        let norm = |v: i64| -> i64 {
            if v < 0 {
                len_i.saturating_add(v).max(0)
            } else {
                v.min(len_i)
            }
        };
        let start = self.start.map_or(0, norm);
        let end = self.end.map_or(len_i, norm);

        let mut out = Vec::new();
        let mut i = start;
        while i < end {
            // Invariant: 0 <= i < end <= len, so `i as usize` is a valid index.
            out.push(i as usize);
            // `saturating_add` so a huge `step` near `i64::MAX` ends the loop
            // instead of overflowing (keeps `resolve` total for any input).
            i = i.saturating_add(step);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse helper: panics on error with the spec for a readable failure.
    fn slice(spec: &str) -> FrameSlice {
        spec.parse().unwrap_or_else(|e| panic!("parse {spec:?}: {e}"))
    }

    // --- The examples from the feature spec (len = 100 unless noted) ---

    #[test]
    fn every_third() {
        // `::3` over 0..10 -> 0,3,6,9
        assert_eq!(slice("::3").resolve(10), vec![0, 3, 6, 9]);
    }

    #[test]
    fn from_ten_to_end() {
        assert_eq!(slice("10:").resolve(15), vec![10, 11, 12, 13, 14]);
    }

    #[test]
    fn first_hundred() {
        let got = slice(":100").resolve(250);
        assert_eq!(got.len(), 100);
        assert_eq!(*got.first().unwrap(), 0);
        assert_eq!(*got.last().unwrap(), 99);
    }

    #[test]
    fn ten_to_two_hundred_step_three() {
        // 10,13,...,<200 over a long-enough sequence
        let got = slice("10:200:3").resolve(300);
        assert_eq!(*got.first().unwrap(), 10);
        assert_eq!(*got.last().unwrap(), 199);
        assert!(got.iter().all(|&p| (p as i64 - 10) % 3 == 0));
    }

    #[test]
    fn last_fifty() {
        // `-50:` over len 100 -> 50..100
        let got = slice("-50:").resolve(100);
        assert_eq!(got.len(), 50);
        assert_eq!(*got.first().unwrap(), 50);
        assert_eq!(*got.last().unwrap(), 99);
    }

    #[test]
    fn drop_first_and_last_five() {
        // `5:-5` over len 100 -> 5..95
        let got = slice("5:-5").resolve(100);
        assert_eq!(*got.first().unwrap(), 5);
        assert_eq!(*got.last().unwrap(), 94);
        assert_eq!(got.len(), 90);
    }

    // --- Empty / defaults ---

    #[test]
    fn empty_spec_selects_all() {
        let all = slice("").resolve(5);
        assert_eq!(all, vec![0, 1, 2, 3, 4]);
        // Whitespace-only is also "all".
        assert_eq!(slice("   ").resolve(3), vec![0, 1, 2]);
    }

    #[test]
    fn bare_colon_selects_all() {
        assert_eq!(slice(":").resolve(4), vec![0, 1, 2, 3]);
        assert_eq!(slice("::").resolve(4), vec![0, 1, 2, 3]);
    }

    #[test]
    fn empty_step_part_defaults_to_one() {
        // `10:200:` -> step 1
        assert_eq!(slice("10:13:").resolve(100), vec![10, 11, 12]);
    }

    // --- Negatives (explicit, on a known len = 100) ---
    // Python `slice.indices(100)`: a negative index adds 100 once, then clamps.

    #[test]
    fn neg_last_fifty() {
        // -50: -> start = 100-50 = 50 -> indices 50..100
        let got = slice("-50:").resolve(100);
        assert_eq!(got.len(), 50);
        assert_eq!(*got.first().unwrap(), 50);
        assert_eq!(*got.last().unwrap(), 99);
    }

    #[test]
    fn neg_drop_first_and_last_five() {
        // 5:-5 -> start 5, end 100-5 = 95 -> indices 5..95
        let got = slice("5:-5").resolve(100);
        assert_eq!(*got.first().unwrap(), 5);
        assert_eq!(*got.last().unwrap(), 94);
        assert_eq!(got.len(), 90);
    }

    #[test]
    fn neg_all_but_last() {
        // :-1 -> start 0, end 100-1 = 99 -> indices 0..99 (drops the last one)
        let got = slice(":-1").resolve(100);
        assert_eq!(got.len(), 99);
        assert_eq!(*got.first().unwrap(), 0);
        assert_eq!(*got.last().unwrap(), 98);
    }

    #[test]
    fn neg_just_the_last() {
        // -1: -> start 100-1 = 99, end 100 -> [99]
        assert_eq!(slice("-1:").resolve(100), vec![99]);
    }

    #[test]
    fn neg_start_clamps_to_zero() {
        // -200: -> start = (100-200).max(0) = 0 -> whole seq, no panic
        let got = slice("-200:").resolve(100);
        assert_eq!(got.len(), 100);
        assert_eq!(*got.first().unwrap(), 0);
        assert_eq!(*got.last().unwrap(), 99);
    }

    #[test]
    fn neg_both_ends() {
        // -3:-1 -> start 97, end 99 -> [97, 98]
        assert_eq!(slice("-3:-1").resolve(100), vec![97, 98]);
    }

    #[test]
    fn neg_resolved_start_after_end_is_empty() {
        // -1:-5 -> start 99, end 95 -> empty (no panic)
        assert!(slice("-1:-5").resolve(100).is_empty());
        // -1:1 -> start 99, end 1 -> empty
        assert!(slice("-1:1").resolve(100).is_empty());
    }

    #[test]
    fn neg_with_step() {
        // -10::2 over len 100 -> start 90, step 2 -> 90,92,94,96,98
        assert_eq!(slice("-10::2").resolve(100), vec![90, 92, 94, 96, 98]);
    }

    // Small-len negatives (guards the fold at a different scale).
    #[test]
    fn neg_small_len() {
        assert_eq!(slice("-3:-1").resolve(10), vec![7, 8]);
        // -100 on len 10 folds to -90 -> clamped to 0 -> whole seq
        assert_eq!(slice("-100:").resolve(10), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    // --- Totality: never panics for any i64 start/end and any len ---

    #[test]
    fn totality_extreme_bounds_never_panic() {
        // Build slices directly to exercise i64 extremes the parser also accepts.
        let cases = [
            (Some(i64::MIN), None),           // start fold overflow-guarded to 0
            (None, Some(i64::MIN)),           // end folds to 0 -> empty
            (Some(i64::MAX), None),           // start clamps to len -> empty
            (None, Some(i64::MAX)),           // end clamps to len -> whole
            (Some(i64::MIN), Some(i64::MAX)),
            (Some(i64::MAX), Some(i64::MIN)),
        ];
        for len in [0usize, 1, 10, 1000] {
            for &(start, end) in &cases {
                let s = FrameSlice { start, end, step: 1 };
                let out = s.resolve(len); // must not panic
                // Every emitted position is a valid in-range index.
                assert!(out.iter().all(|&p| p < len));
            }
        }
        // i64::MIN start with an enormous step and len 0 -> empty, no panic.
        let s = FrameSlice { start: Some(i64::MIN), end: Some(i64::MAX), step: usize::MAX };
        assert!(s.resolve(0).is_empty());
    }

    // --- start > end => empty ---

    #[test]
    fn start_after_end_is_empty() {
        assert!(slice("8:3").resolve(10).is_empty());
        // Same via negatives: 9 .. (10-8=2) -> empty
        assert!(slice("9:-8").resolve(10).is_empty());
    }

    // --- Out-of-range clamp (no panic) ---

    #[test]
    fn out_of_range_clamps() {
        // start beyond len -> empty, no panic
        assert!(slice("50:").resolve(10).is_empty());
        // end beyond len -> clamps to len
        assert_eq!(slice("8:999").resolve(10), vec![8, 9]);
        // both beyond -> empty
        assert!(slice("100:200").resolve(10).is_empty());
    }

    #[test]
    fn resolve_on_empty_sequence() {
        assert!(slice("::2").resolve(0).is_empty());
        assert!(slice("").resolve(0).is_empty());
    }

    // --- Errors ---

    #[test]
    fn bare_number_is_error() {
        assert_eq!("3".parse::<FrameSlice>(), Err(FrameSliceError::BareNumber));
        assert_eq!("-5".parse::<FrameSlice>(), Err(FrameSliceError::BareNumber));
    }

    #[test]
    fn non_positive_step_is_error() {
        assert_eq!("::0".parse::<FrameSlice>(), Err(FrameSliceError::NonPositiveStep(0)));
        assert_eq!("::-1".parse::<FrameSlice>(), Err(FrameSliceError::NonPositiveStep(-1)));
        assert_eq!("0:10:-2".parse::<FrameSlice>(), Err(FrameSliceError::NonPositiveStep(-2)));
    }

    #[test]
    fn non_integer_part_is_error() {
        assert_eq!("a:5".parse::<FrameSlice>(), Err(FrameSliceError::NotInteger("a".into())));
        assert_eq!(":b".parse::<FrameSlice>(), Err(FrameSliceError::NotInteger("b".into())));
        assert_eq!("0:5:x".parse::<FrameSlice>(), Err(FrameSliceError::NotInteger("x".into())));
    }

    #[test]
    fn too_many_parts_is_error() {
        assert_eq!("1:2:3:4".parse::<FrameSlice>(), Err(FrameSliceError::TooManyParts(4)));
    }
}
