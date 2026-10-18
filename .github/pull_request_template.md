## Change

<!-- Describe the problem and the resulting behavior. -->

## Validation

<!-- Record the checks and their results, including relevant regressions. -->

## Resource cost

<!-- For UI, polling, cache, worker, library, streaming or plugin changes:
Identify the limits, ownership and release conditions of new resources.
Attach matched release measurements with executable hashes, workload,
settings, renderer, RSS, CPU units, GPU and transient versus retained peaks.
Cover the inactive/hidden state and repeated use as well as the active state.
Run tools/lint/footprint --binary <release> for the clean Linux shell's
250 MiB RSS budget. Keep native playback measurements separate from it.
Resolve reproducible regressions before merging; document an explicit
budget decision before accepting an increase. Otherwise, explain why
the change cannot affect resource use.
-->
