# Contributing

I track bugs in [Issues](https://github.com/vicrodh/chilpa/issues) and discuss
features in [Discussions](https://github.com/vicrodh/chilpa/discussions).
For a bug I need the Chilpa version, operating system, audio backend,
reproduction steps and relevant logs with account data removed.

## Code boundaries

I keep the audio engine and core independent of Qt. A frontend talks to
`chilpa-api`; it does not reach into the library or audio engine directly.
I keep source-specific integration code in separate plugin repositories.

I write identifiers, comments, tests and developer documentation in English.
I translate user-visible messages through the existing catalogs. I keep QML
features in their own modules and use the shared theme for colors, dimensions
and typography.

## Checks

I enable the repository hook after cloning:

```sh
git config core.hooksPath hooks
tools/lint/fast
python3 -m unittest discover -s scripts/tests
cargo test --locked -q -p chilpa-api --lib
```

I run only one Rust compilation or test build at a time on machines with
limited memory. I use the focused QML runner for UI behavior:

```sh
python3 scripts/test-qml.py crates/chilpa-qt-kit/tests/qml/tst_page_transition.qml
```

I include a focused regression when a change fixes behavior. I describe what
changed, how I tested it and any remaining platform limits in the pull request.
