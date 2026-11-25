//! LearnHarness 0.7.0 text, graph, and chat-folder profile contract.
//!
//! The upstream 0.7.0 chat, graph, and source-organization stores are byte-for-byte
//! unchanged from 0.6.1. This uses the same strict parser, with a distinct wire
//! format so a 0.7.0 snapshot cannot be applied to a 0.6.1 managed profile.
use crate::Payload;

pub const FORMAT: &str = "learnharness-0.7.0-disposable-v1";

pub fn supported_path(path: &str) -> bool {
    crate::learnharness_061::supported_path(path)
}

pub fn validate(payload: &Payload) -> Result<(), String> {
    crate::learnharness_061::validate(payload)
}
