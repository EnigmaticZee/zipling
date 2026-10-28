# Local patches

This is `preflate-rs` 0.7.6 from crates.io (Apache-2.0, Microsoft; see
https://github.com/microsoft/preflate-rs), with research patches. Search for
"Arca patch" to find each one.

1. **Step counter.** `HashChainHolder::steps` and `TokenPredictor::steps` count the
   hash chain entries visited. Predicting a stream visits exactly the entries that
   recreating it will, so the count measures the rebuild cost of a stream
   deterministically (~3.1 ns per step on an M1 Pro).
2. **Step budget.** `PreflateConfig::max_steps_per_byte` (0: off) abandons a stream
   with `ExitCode::OverBudget` once prediction passes that many steps per
   compressed byte (at least 1 Mi steps). Checked every 1,024 tokens, so an
   abandoned stream costs little.
3. **Chain cap.** `PreflateConfig::cap_max_chain` predicts with
   `max_chain_length` even when the stream was made with a deeper search, letting
   the corrections absorb the differences (research only; it did not pay).

Output for streams that are accepted is unchanged by all three.
