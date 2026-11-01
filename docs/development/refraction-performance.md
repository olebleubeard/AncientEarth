# Refraction performance verification

Use native application captures to assess flight responsiveness. Isolated GPU
probes locate expensive shader work and preserve outputs for numerical review;
their timings do not establish application frame rates or visual correctness.
Scientific acceptance belongs in the [integrator review](../science/optical-integrator-review.md).

## Preserve a reproducible checkpoint

Stop editing the shader, fixture and wrapper inputs before starting a run. Give
each run a new descriptive name. The wrappers refuse to overwrite existing
evidence and record source, linked-library and executable hashes. They check
that inputs stayed unchanged through compilation and execution. Failed runs
retain their logs and partial evidence.

Run only one GPU workload at a time for timing comparisons. Do not compile the
app, run another graphics probe or change the tested source during a capture.
Keep the actual input buffers and executable identity with the result; the Git
commit alone does not identify an uncommitted build.

## Isolated solver probes

The normal-coordinate probes are development diagnostics. Their math-owned
fixtures are separate from the production primary shader. A passing diagnostic
does not enable that solver in the app.

Compile and validate without starting graphics:

```powershell
./scripts/verify_optical_normal_trace.ps1 -CompileOnly
./.cache/optical-normal-trace-probe.exe --validate-only
./scripts/verify_optical_normal_performance.ps1 -CompileOnly
./.cache/optical-normal-performance-probe.exe --validate-only
```

After independent review of the fixture and numerical change, run a uniquely
named checkpoint:

```powershell
./scripts/verify_optical_normal_trace.ps1 -Name optical-normal-trace-example
./scripts/verify_optical_normal_performance.ps1 -Name optical-normal-performance-example
```

Each wrapper owns a hidden child process and a bounded watchdog. It terminates
only that child on timeout. The performance probe preserves timestamp pairs,
all output records, source buffers and selected sample records. Shader
compilation and host readback time are recorded separately. The independent
review scripts check the recorded evidence; a successful process exit alone is
not numerical acceptance.

The performance wrapper's `-Selected` switch instead dispatches the explicit
pixel selection from its math-owned fixture. The report keeps the original
camera dimensions and separately records output count, record stride, dispatch
and sample-to-record mapping. Extra diagnostic words follow the original
terminal words. Compare those original words with the retained full-frame run
before attributing the diagnostics to that run; instrumentation can affect
compiler output. Selected-ray timings are not full-frame timings.

The trace wrapper's `-Capture` switch retains attempted solver inputs and
intermediate results for cost isolation. Check the original diagnostic words
against the corresponding uninstrumented run, and check that the capture did
not overflow, before using those records. Capture instrumentation is not a
performance measurement.

`scripts/verify_optical_height_trials.ps1` runs the independently reviewed
trial fixture against those retained inputs. It saves the full detail output
and separate full-trial, stage-only and bounds-only checksum buffers. Use
`scripts/review_optical_height_trials.py` to compare the detail records and
independently reconstruct the checksums. Keep all raw buffers even when a
comparison fails: changing a shader's compilation context can change its
results. The separate component timings are not additive, exclude later
controller work and do not describe a complete frame. The shared mechanical
runtime is `scripts/optical_gpu_runtime.rs`; it preserves the same serial
timestamp protocol as the full-resolution diagnostic.

The same trial runner's `-CartesianPromoted` mode selects the separate native
precision control, using `scripts/optical_cartesian_trial_fixture.rs` and its
reviewed input inventory. It is exclusive with `-FastBounds` and requires the
Vulkan adapter's shader-f64 and shader-int64 features. `-ValidateOnly` retains
assembled source, uploaded inputs and an offline validation report without
creating a graphics instance. Both modes preserve pre-compilation inputs,
compiler failures, a per-run executable, the owned process lifecycle and raw
outputs. The original host/wrapper versions are retained under
`artifacts/optical-cartesian-trial-host-before-source`. The new component
control has no full-controller or native-frame performance interpretation.

`-CartesianDirect` is a distinct, fully observed trial workload. It cannot be
combined with either earlier candidate mode. The mathematics-owned fixture
declares the output allocation and two entry points. The host clears the
entire output before every dispatch and saves its complete contents after
every warmup and measured dispatch, before the next dispatch can overwrite
them. Each timestamp row identifies its raw capture; the wrapper's output
manifest records the capture's hash. The clear and readback occur outside the
timestamped interval, while the shader's full output stores are timed. These
measurements therefore include the observation workload and cannot establish
an uninstrumented solver's cost or native frame rate. Validate every retained
numeric output before interpreting cost. The earlier checksum experiment's
rejection remains preserved; this mode does not reinterpret its timing.

The diagnostic wrappers accept `-FastBounds` to select the separately reviewed
bound-calculation candidate. This flag does not change the application's
solver. The cost probe retains an additional original-reference kernel and
buffer in this mode. Its report identifies which detail block belongs to each
timed kernel; compare the matching blocks. Trial capture and fast bounds are
separate modes. A full-resolution or selected performance run can use fast
bounds after the corresponding complete-ray checks pass.

`-FastMetric` additionally selects the reviewed endpoint-metric candidate and
requires `-FastBounds`. Trace and performance reports record both selections.
`-ProjectedHeightRhs` selects the separately reviewed algebraic field evaluator
and requires both flags. Its arithmetic order changes, so retained inputs and
fresh numerical comparisons are required; earlier byte parity cannot be
assumed for this candidate.
For a selected performance run, `-Selection artifacts/example.json` selects an
explicit math-owned diagnostic list and requires `-Selected -FastBounds`.
The wrapper fingerprints that exact list. The original selection remains the
default when the option is omitted.

`-GroupSize` selects an explicitly reviewed workgroup specialization for the
atmosphere performance fixture; the default remains 32. Preserve all outputs
when comparing sizes, since a scheduling declaration can affect compilation.
Only accept a timing improvement after checking record correspondence. Group
size does not change image dimensions, the pixel list or source inputs.

The trace wrapper's separate `-Map` mode dispatches the math-owned canonical
coordinate and derivative fixture. Its input stride comes from fixture
metadata; each output record remains intact in the raw readback. The report
identifies this mode explicitly. It is an interval check, not a complete ray,
scene-contact or performance test, and cannot be combined with trial capture
or the other candidate switches.

`scripts/verify_optical_normal_packet.ps1` is a CPU-only check of the candidate
host uniform. It runs the math-owned source fixture, serializes the actual
`ShaderType` with Bevy's `encase` implementation, and compares the result with
retained diagnostic bytes. Both byte buffers and the comparison report are
saved before assertions. Source-fixture failures can occur earlier; preserve
their logs and input fingerprints as well. The wrapper fingerprints the exact
retained canonical inputs and native linker libraries. Its successful canonical
fixture check does not assert that a fresh weather import produces identical
inputs. This check neither registers the candidate in the application nor
measures GPU performance.

`scripts/prepare_optical_normal_scene.ps1` runs the CPU-only math-owned scene
preparer after independent source review. It fingerprints the actual source
data and the retained geometry/frame references, and preserves the generated
packet plus the preparer's logs even when validation fails. It does not run a
ray or create a graphics device.

The six-binding scene host is `scripts/verify_optical_normal_scene.ps1`. It
requires the reviewed source and runtime dependency inventory in
`scripts/optical_normal_scene_inputs.txt` before it can compile. Its default
dispatch uses the fixture's explicit pixel selection. `-Full` requires a
record for every native pixel; `-Time` additionally enables the serial GPU
timestamp protocol and requires `-Full`. `-ValidateOnly` checks the assembled
shader without creating a graphics device. All scene inputs and output words
are retained, including the original terminal header and separate diagnostic
tail. Scene-dispatch timing still excludes stellar calculations, shading, UI
and presentation; native flight remains a separate acceptance check.
`-Reference` selects the fixture's unchanged original primary solver. The
diagnostic tail can retain both counters and the exact GPU launch direction;
its byte stride is recorded separately from the production terminal layout.
The host logs shader-module creation, pipeline creation, dispatch and readback
separately. The wrapper retains a lifecycle report identifying its own child
process, timeout and exit status, including unsuccessful runs. A timeout during
pipeline creation is a compilation result, not a ray-performance measurement.
Keep failed attempts alongside later successful results; each run requires a
new evidence prefix.

`-Curves` runs the separate scene consumer on retained accepted interval
records. Binding 6 contains the complete math-owned record buffer and is saved
with the other inputs. This mode cannot be combined with full-image, reference
or timing modes. Its dimensions identify storage records, not camera pixels;
its status and diagnostic fields use the fixture's interval-specific schema.
It does not validate a complete atmospheric ray or application frame.
`-PruneUnused` asks Naga for the entry-point function closure, then removes only
the original source spans of unreachable functions. It saves the original and
submitted shader plus a reachability report; all retained function bodies remain
byte-identical. Both versions pass Naga validation. This compiler diagnostic
does not change equations or establish numerical acceptance by itself.

`-ReferenceReuse` selects the isolated first-field reuse adapter for the original
midpoint solver; `-ReferenceCarry` also retains accepted endpoint geometry;
`-Reference` selects the unchanged solver. These modes are mutually exclusive
and support selected or full-resolution dispatches. Preserve
both complete terminal buffers and source inventories before comparing timing.

`-CartesianCurves -PruneUnused` runs the isolated certified-piece consumer.
It uploads the math fixture's retained curves and piece packet through the
existing seventh binding and preserves the complete result buffer. The reported
coordinates identify interval records. This is not a camera render or a
full-flight benchmark.

`-CartesianCovered` uses the separately reviewed coverage variant;
`-CartesianStages` adds return-site diagnostics to that variant. Stage runs
preserve the original terminal records and the first eight diagnostic words,
then append four diagnostic words per record. The report records the resulting
48-byte diagnostic stride. Check those original words against the covered
baseline before interpreting the added tags.

`-PositiveContacts` selects the six manufactured contact cases from the
reviewed CPU packet. It uses the same seventh binding and the original
32-byte diagnostics. Its coordinates identify fixture records, not camera
pixels. Compare it with the independently retained contact oracle; this test
does not establish accuracy for a complete physical ray.
`-PositiveStages` adds the reviewed return-site diagnostics to those same
cases. It retains the original terminal and diagnostic words and records the
extra fields with the same 48-byte stride used by `-CartesianStages`.
`-PositiveChronology` selects the separately reviewed chronological-exclusion
experiment on those same six cases with the original 32-byte diagnostics.
It is mutually exclusive with the other interval modes. Compare its complete
output with the retained independent oracle; it is not enabled in the app.

`-UniformLoops` is restricted to an isolated interval fixture with
`-PruneUnused`. It applies the math-owned compiler adapter and retains its
report and exact uniform bytes. This switch is a compilation experiment;
pipeline creation time is separate from GPU dispatch timing.

The CPU-only wrappers `scripts/prepare_optical_coverage_cores.ps1` and
`scripts/prepare_optical_positive_contacts.ps1` prepare reviewed fixture data.
Each requires a fresh evidence name and records source fingerprints before
compilation and after execution. The latter creates manufactured contact
cases for testing the scene consumer, not physical atmospheric trajectories.
Neither wrapper runs the GPU or establishes application performance.

`scripts/verify_optical_wavefront.ps1` is a separate host for the reviewed
continuation experiment. It preserves the original input packet through an
ordered initialization, continuation and finalization sequence. `-Reference`
selects the original solver with attempt counters; `-SliceAttempts` selects
the fixture's supported batch size. `-Full -Time` measures the complete
sequence with the same serial timestamp protocol. Progress is cleared before
each sequence, and final states, progress, terminal records and diagnostics
are all saved before acceptance assertions. A matching final header requires
the fixture's publication guard to succeed. These files remain diagnostic
tools; they do not enable continuation in ordinary application runs.

`-ReduceBookkeeping` selects a separate continuation control that replaces
per-ray progress atomics with final metadata validation. Its additional
storage binding contains partial validation records, which are saved alongside
the final state and progress buffers. The timestamp still covers the complete
sequence, including validation. This mode is incompatible with `-Reference`.

The separate `-CartesianSuffix` diagnostic hosts the math-owned fixture in
`scripts/optical_cartesian_suffix_fixture.rs`. It is incompatible with
`-Reference`, `-ReduceBookkeeping` and an explicit `-SliceAttempts`. Each
fixture module is retained separately, checked offline against the host
binding layout and its requested entry points, and compiled with the normal
runtime checks. The ordered dispatch sequence comes from the fixture;
timestamps enclose the complete sequence, including fallback and validation.
The original `.wgsl` file is retained fixture context; the report's module
list identifies the separately submitted shader files. State-record stride
is supplied by the fixture and recorded in the report. All raw state,
progress, terminal and diagnostic buffers are saved before completion checks.
The common and suffix-specific input inventories are both fingerprinted,
including on compilation failures. This host support is not an application
integration or a performance result; the
[scientific design and acceptance gates](../science/cartesian-suffix-design.md)
must be satisfied separately.

`-CartesianSuffixTrace` is a selected-record diagnostic for accepted-step
logging. It implies suffix mode and rejects `-Full` and `-Time`. The host
allocates the fixture's additional storage buffer, clears it before dispatch
and saves it as `-auxiliary.bin`; the report records its layout. All existing
terminal, state and progress evidence is retained. Its separate trace input
inventory is also fingerprinted. The telemetry must reproduce the frozen
candidate's original output before the mathematics reviewers interpret it.
This mode does not alter the candidate's numerical acceptance policy.

`-CartesianSuffixTight` selects the separately reviewed convergence fixture
through that same logging interface. It also rejects full-frame and timing
runs. The original candidate and logging wrapper are retained. This is one
bounded numerical comparison under unchanged acceptance gates; neither its
name nor successful execution establishes improved accuracy or performance.

`-CartesianSuffixBranch CASE` runs a math-owned matrix case through the same
selected logging interface. `-BranchReference` disables the handoff for that
case's original-solver baseline; it requires a case name. Both modes reject
full-frame and timing options. Preserve the original case outputs and its
reviewed acceptance policy before running the candidate. The case adapter's
inventory is included in every build that imports it.

`-CartesianSuffixF64 selected` and
`-CartesianSuffixF64 atmosphere-100-gradient` select a separate, initially
selected-record-only controller experiment. The wrapper and host reject
full-view timing and combinations with other candidate/reference modes. The
existing fixture supplies all buffer sizes and dispatch ordering; this mode
requires shader-f64 and shader-int64 support. Its source inventory and exact
inputs are fingerprinted like the other modes. It is a new engineering
feasibility effort after independent design review. Its first selected GPU
run failed the frozen required-route gate; that rejection is retained. The
atmosphere run under this policy was not executed.

`-CartesianCompositeF64 CASE` is a separately authorized correctness
evaluation using the unchanged numerical shaders and a new reviewed case
adapter. The selected result is reused from the prior run rather than rerun.
The wrapper accepts the eleven explicit branch, atmosphere and returning case
names; it rejects full-frame/timing options and other fixture combinations.
`-CartesianCompositeReference` is allowed only with `returning-gradient` to
retain that case's original control. New report keys are
`cartesian_composite_f64` and `cartesian_composite_reference`; the old mode and
verdict remain distinct. Source, adapter, prospective policy and result-auditor
reviews precede execution. No full-frame run or production adoption is
authorized until the mathematical and performance gates pass.

`-CartesianCompositeFull -Full` is the separate complete-frame decision mode;
`-Time` enables the existing complete-sequence timestamp protocol, and
`-ValidateOnly` checks assembled modules without requesting a graphics device.
Other fixture switches and explicit slice counts are rejected. The
math-owned full adapter supplies layout, dispatches and initialization inside
the measured pass. Its source and result policy require independent review
after the returning-case closure, before a GPU run. This diagnostic flag does
not enable a new production renderer. Production adoption still requires the
accuracy, complete-frame performance and native moving-view gates.

The first matched complete-frame pair passed its frozen correctness checks
and failed its performance gate. Independent raw review confirmed the rejection.
The candidate was not integrated into the application. See
`docs/science/cartesian-composite-full-outcome.md` and
`artifacts/optical-composite-f64-full-candidate-first-closure-review.json`.

The continuation wrapper preserves a separate `-executable.exe` for each new
evidence run and launches that exact file. Its hash is checked after copying
and after execution, and retained in the source and lifecycle reports. Older
runs that reused `.cache/optical-wavefront-probe.exe` retain their recorded
hashes and compiler inputs, but their overwritten binaries are unavailable.

`-OriginalClearance` selects a separate math-owned original-solver experiment
through the existing multi-module host. It cannot be combined with reference,
Cartesian suffix, reduced-bookkeeping or sliced-continuation options. Its
fixture supplies the state stride and ordered dispatches, and its separate
input inventory is fingerprinted. This flag does not select the rejected
tighter Cartesian solver. Full-frame timing remains subject to independent
correctness review and complete terminal-output comparison.

`scripts/verify_optical_deviation_stencil.ps1` is a CPU-only cost and rejection
screen. It compiles the reviewed scalar experiment with optimization and the
isolated math dependencies, fingerprints its exact retained inputs and saves
the full report under a fresh name. Its sampled comparisons cannot admit a
shared cell. The reference policies differ, and the report's single-run CPU
durations are not a GPU speedup forecast. Keep CPU cost measurements separate
from shader compilation and native application tests.

The wrapper's `-Flow` switch selects the separate common-schedule bounds
screen. It fingerprints its four source modules and the retained
`optical-deviation-stencil-progress-first.json` input, and writes the mode to
the source manifest. Its output is still experimental; it cannot establish
physical-ray acceptance or native flight performance.

`scripts/verify_optical_normal_selected.ps1` hosts the separate selected-ray
integration experiment. It uses the common scene inventory plus
`scripts/optical_normal_selected_inputs.txt`, validates both shader modules,
and optionally applies `-PruneUnused` while retaining the original sources.
`-ValidateOnly` makes no GPU calls. The GPU path initializes the fixture,
orders each producer/consumer pair, and finalizes it within one compute pass.
It clears the writable buffers before dispatch and saves the complete
terminal, continuation, generated-piece and progress buffers before checking
the final header. Round limits, buffer sizes and all numerical rules remain
fixture-owned. This host has no full-view or performance mode; source review
and a successful independent output audit are required before advancing.
`-SplitCertificate` selects a separate three-module fixture. Its certificate
entry runs after each producer dispatch and before scene consumption, using
the same explicit storage bindings. The host preserves original and submitted
source for all modules and records the selected mode. A failed or incomplete
header still fails the run after preserving the complete raw output.
Compiler inputs and output are saved before compilation so a build failure
also remains reproducible. A completed process and valid header only establish
that the diagnostic finished: explicit unresolved ray results still require
investigation and cannot qualify the solver for use in the app.

The separate `scripts/verify_optical_normal_scene.ps1 -Reference -Full
-FiniteLoops` experiment is restricted to an exact archived reference shader
and uniform packet. Its mathematics-owned termination guard runs before device
creation and records `-termination.json`; unreviewed source or packet changes
are rejected. It changes only `ShaderRuntimeChecks::force_loop_bounding`,
retaining the other checked runtime settings, WGSL validation, input buffers
and arithmetic. It is not a general permission to disable loop checks in
production. Compare with a fresh run of the same full reference fixture
without `-FiniteLoops`, using `-Time` for the existing timestamp protocol.

`scripts/prepare_optical_patch_certificates.ps1` prepares the separate static
terrain and per-eye patch packet. It compiles the math-owned entry with `-O`,
retains the exact source and data fingerprints, passes a fresh artifact
prefix, and verifies all six expected packet files. The run is bounded and
its process lifecycle is retained. This is CPU preparation only; the packet
does not establish GPU contact accuracy or camera-update performance.

The CPU-only `scripts/verify_optical_handoff.ps1 -Name <fresh-name>` runs the
mathematics-owned `optical_handoff_feasibility.py` screen against retained
inputs. It fingerprints the source inventory, interpreter and selected
numerical runtime modules, preserves partial results, and records the owned
process lifecycle and before/after input stability. Its default timeout is
300 seconds. It neither compiles a shader nor executes GPU work. Process
success must be distinguished from the report's mathematical verdict. Source
review must close before execution; even a
passing verdict does not establish a runtime implementation or speedup.
The fixed `-PackedField` switch selects the separate uploaded-field screen in
the same entry. Its mode is recorded in the source manifest and process
arguments. Use a separate fresh prefix and obtain source review for that mode;
the earlier canonical pass does not approve its execution or results.

## Native flight captures

Use `scripts/capture_refraction_performance.ps1` with a unique `-Name`, the
intended `-Executable` and explicit `-SceneArguments`. Preserve the complete
scene argument list when comparing builds. The wrapper records window focus
observations, executable and source hashes, application frame telemetry and
logs. It makes one normal foreground request to its own window and does not
synthesize input or unlock the desktop.

Check the operating system focus observations before describing a result as
foreground performance. Application focus telemetry alone is insufficient.
Keep startup, shader preparation, warmup and measured route intervals distinct.
Minimize/restore runs are lifecycle checks, not uninterrupted timing baselines.

`--frame-capture-unfocused-optical` requires
`--frame-capture-refraction-flight` and enables a separate route diagnostic when
desktop focus is unavailable. It retains every focus observation and pending
frame. Its `diagnostic_route_passed` result is separate from ordinary `passed`,
which remains false under this policy. The analyzer always excludes such a run
from focused timing, even if focus later returns. Source and viewport changes
still fail the route check. Use a PowerShell array for `-SceneArguments` when
calling the wrapper directly; a comma-separated string passed through
`pwsh -File` is not an argument array.

The optional `--optical-header-readback` flag is limited to bounded frame
captures. It checks association of primary and stellar result headers with
recorded submissions. Pending readbacks and mismatches remain in the report.
A matching header is not proof that a frame was presented on the monitor.
Ordinary flight does not enable this readback.

Primary payloads can be reused only when every serialized numerical input is
bit-identical, with the same viewport, immutable geometry/frame buffers, output
allocation and actual GPU pipeline. Only the generation token is excluded from
the key. Geometry and frame buffers prohibit in-place GPU uploads; changed
contents therefore receive new buffer identities. Identical frame words skip
the upload. The render command stream copies the current header before stellar
work and composition, preserving ordering with the previous payload. Withdrawal
or unavailable inputs invalidate reuse. This is an idle optimization; changed
views still run the complete primary solver.

Use `--optical-no-primary-reuse` for a same-build comparison. The capture
wrapper's `-TracePrimary` switch logs each encoded primary token and whether it
reused a payload. Those logs describe command encoding, not GPU completion;
combine them with the bounded header readback and frame telemetry when testing.
Frame-generation tokens are retained separately from the view configuration
used to detect movement. Earlier captures that included a stellar token in that
configuration retain their original `view_changed_during_capture` flags; do not
rewrite those artifacts or treat them as qualified stationary benchmarks.

For stellar comparisons, `--optical-no-stellar-cache` selects the original
per-pixel inverse path. Shared stellar calculations are enabled by default.
Keep date, weather, scene, camera and scintillation phase identical for visual
comparisons. Register retained screenshots in `licenses/assets.json` and run
the asset audit before packaging.

## Current scheduling regression checks

The production revision with same-frame primary dispatch and shared stellar
calculations passed `cargo test --workspace --all-targets --locked --offline`
and `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`.
Local logs are retained as `artifacts/refraction-workspace-regression.log` and
`artifacts/refraction-workspace-clippy.log`. Incremental-cache hard-link warnings
reported fallback to file copies; neither command failed. These checks cover
the production revision, not acceptance of the isolated experimental solver.

The later exact-primary-reuse revision passed all 49 desktop tests (two existing
GPU tests remain opt-in), the workspace/all-targets Clippy check with warnings
denied, formatting, the asset audit and software/font notice verification.
Logs are retained as `artifacts/refraction-reuse-desktop-tests.log` and
`artifacts/refraction-reuse-clippy.log`. The desktop test run includes cache-key
invalidation and the separate unfocused motion-policy checks. These results do
not qualify an experimental numerical shader for production.
