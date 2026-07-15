# Working agreements

## Plan before building new systems
Before implementing a new system or non-trivial feature (e.g. adding a new
gameplay system, a new UI screen, reworking an architecture/subsystem), stop
and present a plan first — use Plan Mode so the user can review, adjust, or
comment before any code is written. Small fixes (bug fixes, typos, tweaks to
existing code) don't need this — use judgment on "non-trivial."

## Verify unfamiliar Bevy APIs against docs before using them
This project pins a specific Bevy version in `Cargo.lock`, and Bevy's API
changes significantly between versions/from training data. Before using a
Bevy API you're not confident about, fetch the matching docs.rs page for the
version in `Cargo.lock` first, rather than relying on memory. This doesn't
apply to APIs you've already verified/used correctly in this project — only
to unfamiliar or uncertain ones. `cargo check` passing is not sufficient
verification for runtime/plugin-registration behavior — prefer a quick real
run when feasible.
