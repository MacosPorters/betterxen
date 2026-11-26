# Shared hybrid runtime

`bx-cli` is the executable. This library supplies its single supervised execution path for named CrossOver bottles and explicit Wine prefixes, environment preflight, PE validation, process/session lifecycle, memory and output limits, and JSON run evidence. `bx-cli probe`, `shader` and `trace` retain the original #406 classifiers and telemetry aggregator.

The implementation is migrated from BetterXen #405 (`b42133a`) and the maintainer draft preserved at `2645c42`, with #406 (`30e551a`, merged as `f2c42ba`) prefix/shim/profile/log contracts integrated. Original attribution remains in each source file; no retired native crate is a dependency and no alternate runner binary is built.

CrossOver, Rosetta 2 and graphics backends remain external. A dedicated Wine environment is required for safe cleanup: stopping one run stops that environment's server. Session locking coordinates BetterXen launchers; it does not prevent CrossOver's UI or another launcher from using the same environment. A bottle is not a security sandbox.
