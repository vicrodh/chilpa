## Change

<!-- I describe the problem and the resulting behavior. -->

## Validation

<!-- I record the checks I ran and their results, including relevant regressions. -->

## Resource cost

<!-- For UI, polling, cache, worker, library, streaming or plugin changes:
I identify the limits, ownership and release conditions of new resources.
I attach matched release measurements with executable hashes, workload,
settings, renderer, RSS, CPU units, GPU and transient versus retained peaks.
I cover the inactive/hidden state and repeated use as well as the active state.
I run tools/lint/footprint --binary <release> for the clean Linux shell's
250 MiB RSS budget. I keep native playback measurements separate from it.
I resolve reproducible regressions before merging; I document an explicit
budget decision before accepting an increase. Otherwise, I explain why
the change cannot affect resource use.
-->
