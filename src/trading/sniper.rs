//! Telegram-driven sniper.
//!
//! Consumes messages from a Telegram channel listener, identifies call-outs,
//! executes a buy via Jupiter, and spawns a fast-exit task per position.

use serde::{Deserialize, Serialize};

/// A parsed call-out signal from the monitored Telegram channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallSignal {
    /// Solana mint address (base58, always ends with "pump" for pump.fun tokens).
    pub mint: String,
    /// Optional ticker symbol (e.g. "RETARD") extracted from "$TICKER" mention.
    pub ticker: Option<String>,
    /// The trigger keyword that fired ("Gamboled" or "Gamboling").
    pub trigger: String,
}

pub mod parser {
    use super::CallSignal;
    use regex::Regex;
    use std::sync::OnceLock;

    /// Regex for a pump.fun mint: base58 chars (excluding 0, O, I, l)
    /// of length 30-44, ending in literal "pump", on its own line.
    fn mint_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r"(?m)^\s*([1-9A-HJ-NP-Za-km-z]{30,40}pump)\s*$").unwrap()
        })
    }

    /// Regex for the trigger keyword: "Gamboled" or "Gamboling" at the very
    /// start of the message (case-insensitive, allowing leading whitespace).
    fn trigger_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| {
            Regex::new(r"^\s*(?i)(Gamboled|Gamboling)\b").unwrap()
        })
    }

    /// Regex for a ticker mention like "$RETARD".
    fn ticker_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\$([A-Z][A-Z0-9_]{1,15})\b").unwrap())
    }

    /// Parse a message body into a `CallSignal` if it matches the call pattern.
    ///
    /// Rules:
    /// 1. Message must start with "Gamboled" or "Gamboling" (case-insensitive).
    /// 2. Message must contain a pump.fun mint (base58, ends in "pump") on its
    ///    own line.
    /// 3. Returns the first matching mint and the first ticker mention found.
    pub fn parse_call_message(text: &str) -> Option<CallSignal> {
        let trigger_match = trigger_regex().captures(text)?;
        let trigger = trigger_match.get(1)?.as_str().to_string();

        let mint = mint_regex().captures(text)?.get(1)?.as_str().to_string();

        let ticker = ticker_regex()
            .captures(text)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));

        Some(CallSignal { mint, ticker, trigger })
    }
}

#[cfg(test)]
mod tests {
    use super::parser::parse_call_message;

    const REAL_SAMPLE: &str = "Gamboled a bag here on $RETARD. Everybody in crypto feeling like this right now with the current market so heavy mindshare here, could be a runner. DYOR NFA.\n\nhttps://dexscreener.com/solana/J4kiZJMAge9qendsAfDeQXoanHqLHdR6RcCybeitcHJo\n\nhttps://retardcoin.wtf/\n\nhttps://x.com/OfficialRetardX\n\nhttps://t.me/BunchOfRetards\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";

    #[test]
    fn parses_the_real_retard_sample() {
        let signal = parse_call_message(REAL_SAMPLE).expect("should parse");
        assert_eq!(signal.mint, "ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump");
        assert_eq!(signal.ticker.as_deref(), Some("RETARD"));
        assert_eq!(signal.trigger, "Gamboled");
    }

    #[test]
    fn parses_gambling_present_tense() {
        let msg = "Gamboling on $WIF\n\nABCdefGHIjklMNOpqrSTUvwxYZ12345678pump";
        let signal = parse_call_message(msg).expect("should parse");
        assert_eq!(signal.trigger, "Gamboling");
        assert_eq!(signal.ticker.as_deref(), Some("WIF"));
        assert!(signal.mint.ends_with("pump"));
    }

    #[test]
    fn rejects_message_without_trigger() {
        let msg = "Just thinking about $RETARD\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn rejects_message_without_mint() {
        let msg = "Gamboled on $RETARD but no contract address attached";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn rejects_non_pump_address() {
        // Address ends in 'xyz' not 'pump' — should not match.
        let msg = "Gamboled\n\nABCdefGHIjklMNOpqrSTUvwxYZ12345678xyz";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn trigger_is_case_insensitive() {
        let msg = "GAMBOLED\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_some());
    }

    #[test]
    fn mint_inline_with_other_text_is_rejected() {
        // The mint must be on its own line. An inline mention isn't a call.
        let msg = "Gamboled on this token ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump btw";
        assert!(parse_call_message(msg).is_none());
    }

    #[test]
    fn ticker_optional_signal_still_parses() {
        let msg = "Gamboled hard\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        let signal = parse_call_message(msg).expect("should parse without ticker");
        assert_eq!(signal.ticker, None);
        assert_eq!(signal.mint, "ACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump");
    }

    #[test]
    fn trigger_must_be_at_start_not_buried() {
        // "Gamboled" appears but not as the first word — should NOT parse.
        let msg = "Yesterday I gamboled on something, today\n\nACuZX4asxyqcRd6BTgGBKXJjViUP3kZQuDUQawBapump";
        assert!(parse_call_message(msg).is_none());
    }
}
