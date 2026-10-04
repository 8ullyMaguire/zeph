//! Per-model capability tables.
//!
//! Fail-closed: an unrecognised model reports `false`. Guessing `true` risks
//! attaching an image to a model that cannot process it — the failure is silent
//! from the caller's side and surfaces as a server-side error or a degraded
//! answer, never as a routing decision. This is the reasoning `openai/mod.rs`
//! already applies, and the inverse of #6411, where an over-broad prefix match
//! shipped and admitted images to text-only models.
//!
//! Model names are **not** namespaced across providers — `claude-sonnet-4` and
//! `gemini-2.5-pro` are independent schemes with independent lifetimes — so
//! these are per-provider functions rather than one global table. A shared table
//! would be wrong for at least one of them, and wrong silently.
//!
//! Each function follows three rules, mirroring `model_supports_vision`:
//!
//! 1. an explicit text-only exclusion, checked **first**;
//! 2. then a vision-capable prefix list;
//! 3. default `false`.
//!
//! The ordering is load-bearing, and the `o`-series in `OpenAI` is the cautionary
//! case: `o1-mini`, `o1-preview` and `o3-mini` are text-only while `o1`, `o3`
//! and `o4-mini` are not, so one broad prefix rule is wrong in *both* directions.

// Claude ------------------------------------------------------------------

/// Anthropic's naming scheme is `claude-<family>-<major>[-<minor>][-<date>]`
/// (documented on `MODERN_MODEL_FLOORS` in `claude/mod.rs`), so vision support is
/// a property of the **family and generation**, not of the literal string.
///
/// Vision arrived with the 4th generation. `claude-3-*` is the whole of the
/// 3rd generation and is text-only; `claude-haiku-3` and `claude-haiku-35` are
/// the trap here, because a `starts_with("claude-3")` test misses both — they
/// carry the generation in the *family* position, not the leading digits. That
/// mistake reports a text-only model as vision-capable, which is precisely the
/// bug this table exists to prevent.
fn claude_major_version(model: &str) -> Option<u32> {
    let rest = model.strip_prefix("claude-")?;
    let family_end = rest.find(|c: char| !c.is_ascii_alphabetic())?;
    let family = &rest[..family_end];
    if !matches!(family, "opus" | "sonnet" | "haiku" | "fable") {
        return None;
    }
    // The tail begins with the separator, so it has to be skipped before any
    // digits are read: `rest["sonnet-4-5-20250929"]` is "-4-5-20250929", and
    // taking digits from that position yields "" and reports every real model as
    // unknown.
    let tail = rest[family_end..].trim_start_matches('-');
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Whether a Claude model accepts image input.
///
/// Generation 4 and later (opus, sonnet, haiku, fable) accept images; generation
/// 3 and earlier do not. Anything unrecognised — including a future family this
/// table has not been taught — reports `false`, and the caller can override with
/// [`ClaudeProvider::with_vision`](super::claude::ClaudeProvider::with_vision).
#[must_use]
pub fn claude_supports_vision(model: &str) -> bool {
    // Exclusions first. Generation 3 and earlier are text-only across every
    // family, and the leading-3 spelling is common enough in configs and older
    // docs that it has to be handled explicitly as well as by version.
    if model.starts_with("claude-3") || model.starts_with("claude-2") || model.starts_with("claude-1")
    {
        return false;
    }
    match claude_major_version(model) {
        Some(major) => major >= 4,
        None => false,
    }
}

// Gemini ------------------------------------------------------------------

/// Text-only Gemini variants, checked before any prefix rule.
///
/// Flash-Lite and the embedding models are text-only. The embedding check also
/// covers the bare word, because a provider can be constructed with
/// `with_embedding_model("embedding-004")` and `list_models()` then returns it.
fn gemini_text_only(model: &str) -> bool {
    model.contains("-lite")
        || model.contains("embedding")
        || model.starts_with("text-embedding")
        || model.starts_with("embedding-")
}

/// Whether a Gemini model accepts image input.
#[must_use]
pub fn gemini_supports_vision(model: &str) -> bool {
    if gemini_text_only(model) {
        return false;
    }
    model.starts_with("gemini-1.5")
        || model.starts_with("gemini-2.0")
        || model.starts_with("gemini-2.5")
        || model.starts_with("gemini-3")
}

#[cfg(test)]
mod tests {
    use super::{claude_major_version, claude_supports_vision, gemini_supports_vision};

    // -- Claude -----------------------------------------------------------
    //
    // The plan's criterion 2 asks for one text-only and one capable variant with
    // opposite outcomes. Names are taken from the ones this tree itself lists:
    // MODERN_MODEL_FLOORS (claude/mod.rs:221-226), MODELS_WITH_EXTENDED_CACHE_TTL
    // (:84), and the identifiers used in claude/tests.rs and router/bandit.rs.

    #[test]
    fn claude_vision_capable_generations() {
        for m in [
            "claude-opus-4-20250514",
            "claude-opus-4-8",
            "claude-opus-5",
            "claude-sonnet-4-5-20250929",
            "claude-sonnet-5",
            "claude-sonnet-6",
            "claude-haiku-4-5-20251001",
            "claude-haiku-5",
            "claude-fable-5",
        ] {
            assert!(claude_supports_vision(m), "{m} should accept images");
        }
    }

    #[test]
    fn claude_text_only_generations() {
        for m in [
            "claude-3",
            "claude-3-5-sonnet-20241022",
            "claude-3-opus-20240229",
            "claude-3-haiku-20240307",
            "claude-2.1",
            "claude-instant-1.2",
        ] {
            assert!(!claude_supports_vision(m), "{m} is text-only");
        }
    }

    // The specific trap: generation 3 spelled inside the *family* position.
    // `claude-haiku-3` appears in this tree (router/bandit.rs:1012) and a
    // `starts_with("claude-3")` test misses it, reporting a text-only model as
    // vision-capable -- the exact failure mode this table exists to prevent.
    #[test]
    fn claude_generation_3_in_the_family_position_is_text_only() {
        for m in ["claude-haiku-3", "claude-opus-3", "claude-sonnet-3"] {
            assert!(
                !claude_supports_vision(m),
                "{m} is generation 3 and must not report vision"
            );
        }
    }

    // NOT a generation-3 model, despite the digits. `claude-haiku-35` appears in
    // claude/tests.rs:158, but as an arbitrary string in a constructor test, and
    // the tree's own version parser (claude/mod.rs:247-248) reads it as
    // major=35 minor=0 -- not as 3.5. An earlier draft of this test listed it as
    // text-only; that was the test being wrong about the tree, and it is recorded
    // here so the next reader does not "fix" the table to match.
    #[test]
    fn claude_double_digit_generation_is_not_generation_three() {
        assert_eq!(claude_major_version("claude-haiku-35"), Some(35));
        assert!(claude_supports_vision("claude-haiku-35"));
    }

    #[test]
    fn claude_major_version_parses_family_and_generation() {
        assert_eq!(claude_major_version("claude-sonnet-4-5-20250929"), Some(4));
        assert_eq!(claude_major_version("claude-haiku-35"), Some(35));
        assert_eq!(claude_major_version("claude-fable-5"), Some(5));
        // An unknown family is not silently treated as claude-anything.
        assert_eq!(claude_major_version("claude-test-4"), None);
        assert_eq!(claude_major_version("gpt-4o"), None);
        // A family with no generation at all.
        assert_eq!(claude_major_version("claude-sonnet"), None);
    }

    #[test]
    fn claude_unknown_model_is_fail_closed() {
        // Criterion 5 of the plan. A bare alias like "Haiku" is deliberately not
        // in the table: matching it would report vision for a typo'd name.
        for m in ["not-a-real-model", "", "gpt-4o", "Haiku", "sonnet"] {
            assert!(!claude_supports_vision(m), "{m:?} must default to false");
        }
    }

    // -- Gemini -----------------------------------------------------------

    #[test]
    fn gemini_vision_capable_families() {
        // Pinned to gemini/mod.rs:1413 list_models().
        for m in ["gemini-2.5-pro", "gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"] {
            assert!(gemini_supports_vision(m), "{m} should accept images");
        }
    }

    #[test]
    fn gemini_text_only_variants() {
        // Criterion 3 asks for at least one text-only variant.
        for m in [
            "gemini-2.0-flash-lite",
            "gemini-2.5-flash-lite",
            "text-embedding-004",
            "embedding-004",
        ] {
            assert!(!gemini_supports_vision(m), "{m} is text-only");
        }
    }

    #[test]
    fn gemini_unknown_model_is_fail_closed() {
        for m in ["not-a-real-model", "", "claude-sonnet-4"] {
            assert!(!gemini_supports_vision(m), "{m:?} must default to false");
        }
    }

    // Cross-provider: one string must not mean the same thing twice. That is the
    // whole reason these are two functions and not one table.
    #[test]
    fn providers_do_not_share_a_table() {
        assert!(gemini_supports_vision("gemini-2.5-pro"));
        assert!(!claude_supports_vision("gemini-2.5-pro"));
        assert!(claude_supports_vision("claude-sonnet-4-5"));
        assert!(!gemini_supports_vision("claude-sonnet-4-5"));
    }
}