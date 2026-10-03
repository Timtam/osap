# paddle-engines

The neural recogniser's model — PaddleOCR's recognition model, `crates/host/models/ppocr-rec.onnx`
— under two inference engines, on the same input: ONNX Runtime (the `ort` crate, what Windows
runs) and tract (pure Rust). It answers which of the two runs the recogniser on a Mac.

Every PNG in `crates/host/bench-data/ocr/`, and in `--pictures DIR`, goes through the recogniser's
own preprocessing (`crates/host/src/ocr/paddle_pre.rs`, borrowed by `#[path]`), with Windows' crop
and, for the 2x pictures, also with the Retina crop. Each engine runs each input 3 times unmeasured
and N times measured, on one thread; the tool prints the texts, the scores, the largest difference
between the outputs, the medians, and what making each engine cost. tract runs twice: with one
plan for every width (the input's width a symbol), which is how the application would use it, and
with a plan optimised for each width, which says how fast tract can be on this model.

Not part of the repository's workspace, and not built by CI. Run it by hand on Windows, in a release
build, while nobody is testing on the machine:

```powershell
$env:CARGO_TARGET_DIR = "<repository>\target-tools"
cd tools\paddle-engines
cargo run --release -- --pictures ..\..\crates\host\bench-data\ocr\real
```

It reads files and prints; it opens no window and makes no sound. A run takes about a minute.

## The three criteria

tract runs the recogniser on a Mac only if all three hold; otherwise ONNX Runtime does, as a dylib
loaded at run time:

1. **The same text** as ONNX Runtime on every input. The tool's `criterion 1` line.
2. **At most twice ONNX Runtime's time**, the medians summed over every input. The tool's
   `criterion 2` line, for the plan for every width.
3. **The Mac check still passes for both Mac targets.** Not something this tool can measure: tract
   has to type-check for `aarch64-apple-darwin` and for `x86_64-apple-darwin` from Windows, as
   `check-macos.ps1` checks the macOS code. With the targets installed
   (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`):

   ```powershell
   cd tools\paddle-engines
   cargo check --target aarch64-apple-darwin --no-default-features --features tract
   cargo check --target x86_64-apple-darwin --no-default-features --features tract
   ```

## The run of 2026-10-02 (i7-8700K, Windows 11)

55 inputs: the 28 drawn pictures, the 13 real ones of `crates/host/bench-data/ocr/real/`, and the
14 drawn 2x ones again with the Retina crop.

| | ONNX Runtime 1.22 (`ort` 2.0.0-rc.10) | tract 0.23.8, one plan | tract, a plan per width |
|---|---:|---:|---:|
| making it | 257 ms | 633 ms | 232 ms for each new width |
| the medians summed | 873 ms | 6597 ms (7.6x) | 2085 ms (2.4x) |
| the same text | — | 55 of 55 | 55 of 55 |

The outputs differed by at most 1.7e-5. Every output step sums to one: the model ends in a softmax.

1. Met: the same text on every input.
2. Not met: 7.6 times ONNX Runtime's time with one plan, and 2.4 times even with a plan per width.
3. Not met: `tract-linalg`'s build script assembles its kernels with a C compiler for the target
   (`cc`), which a Windows machine does not have for a Mac (`failed to find tool "cc"`), for both
   targets. `ort` with `load-dynamic`, the other way, type-checks for both targets from Windows.

So ONNX Runtime runs the recogniser on a Mac, as a dylib.
