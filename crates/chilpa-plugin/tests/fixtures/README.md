# The compiled fixture

`fixture.wasm` is the `fixture` plugin of the `sdk` branch of the plugins
repository, built from Rust with the SDK. It is kept here already built so
this suite needs neither that repository nor the wasm target to run.

To rebuild it, from a checkout of that branch:

```
tools/build-plugin fixture <out>
cp <out>/app.chilpa.fixture/plugin.wasm   <here>/fixture.wasm
cp <out>/app.chilpa.fixture/manifest.json <here>/manifest.json
```
