# Testing

The Rust suite covers state transitions, filesystem edge cases, and individual
widgets. The QA scenarios in `tests/qa/scenarios` drive the real GTK
application on a headless X display, send real keyboard and pointer input from
inside the process, and check both what the window shows and what happened on
disk. Both run in one pinned container image.

## Running the suites

```bash
./scripts/quality.sh                          # formatting, Clippy, and every Rust test
./scripts/test-headless.py relevant::module   # a native Rust selection on a private display
./scripts/qa.sh                               # every QA scenario, every mode
./scripts/qa.sh copy-leaves-source            # one scenario, all of its runs
```

### Targeted local validation

Choose checks from the behavior and caller mapping, not from changed-file names
alone. Bounded changes may use a non-empty, relevant selection; broad,
cross-cutting, shared infrastructure, dependency/build/CI/harness changes, or
uncertain coverage require the full pinned quality phases and the full QA run.
Include relevant views, callers, and preference behavior, and add or run the
regression test that proves the change. Record the scope rationale, commands,
results, and intentional omissions in the handoff. After editing, rerun affected
tests rather than unrelated suites.

Run local lint and formatting checks only before pushing, not during the
edit/test loop. For Rust changes, run `./scripts/quality.sh fmt` and
`./scripts/quality.sh clippy` on the final code at that checkpoint; rerun affected
checks if fixes change the code before the push. Even when full Rust tests are
needed during iteration, use `./scripts/quality.sh test` and defer lint/format
phases until pre-push. CI's lint and formatting checks remain unchanged.

The repository owner may explicitly authorize pushing a PR without the normally
required local tests for the current change/push. A general push request,
urgency, silence, or consent for another push does not qualify. Record the
explicit authorization, omitted suites, and known failures or unverified behavior
in the handoff and PR description; never report skipped tests as passing. This
exception does not waive lint/format checks, GUI isolation, or required CI/merge
checks. Renew consent if the authorized scope changes, or run the required tests.

For native Rust selection, `scripts/test-headless.py` starts the private display
and session buses, then appends its arguments to the fixed
`cargo test --all-targets --all-features` command. Use module/name filters:

```bash
./scripts/test-headless.py services::operations::tests
./scripts/test-headless.py basenames_reject_empty_reserved_nested_absolute_and_nul_names
```

These select test execution across targets, not compilation to one target.
Targeted runs can still incur a full test build; Cargo's cache helps subsequent
iterations. There is no automatic changed-code dependency-to-test mapping.

Controller regressions live under `src/app/browser/tests/`, grouped by navigation,
location input, selection, monitor updates, preferences, operations, undo, metadata,
staging, event dispatch, and archive activation. Add cases to the matching owner
rather than to `src/app/browser/tests.rs`, which holds shared source/provider
fixtures. Existing adjacent feature test modules retain their ownership. For
example, `./scripts/test-headless.py app::browser::tests::undo::` selects undo
history/claim regressions; `app::browser::tests::metadata::` selects metadata-fill
and sort-terminal regressions. Keep distinct input routes and lifecycle cases
separate even when they share fixtures.

Local-operation regressions have owners under `src/adapters/local_operations/tests/`
for path resolution, copying, moving, replacement, conflicts, progress, naming,
paste results, undo, deletion, and restore safety. Shared fixtures and the manual
deletion benchmarks remain in the parent `tests.rs`; keep the benchmark names
stable for `scripts/benchmark-delete.sh`. For example,
`./scripts/test-headless.py adapters::local_operations::tests::replacement::`
selects replacement safety cases. Preserve independently named filesystem races,
symlink, non-UTF8, cancellation, and conflict scenarios when sharing setup.

Window regressions have policy owners under `src/ui/window/tests/` for keyboard
classification, sidebar places, bookmarks, devices, and trash. Startup and live
preference cases stay in `preferences`; real key-routing scenarios stay in
`keyboard_dispatch` and `type_to_search`. The `ui::window::` filter covers these
owners and the adjacent window modules. When moving a GTK case, update its
`gtk_test` subprocess name to match its compiled test path and verify that the
child actually executes the case; an unchanged outer test count is not enough.

A filter must collect at least one test; use Cargo's output or a collection
check to verify that it did. `scripts/quality.sh` only accepts `all`, `fmt`,
`clippy`, or `test` and does **not** forward test filters, so it cannot be used
for a targeted test selection.

`./scripts/qa.sh` takes scenario names, or `<scenario>/<run>` for one run, and
refuses a name no scenario file has, listing the names it knows; see
"Scheduling and run costs" for `--skip` and the other options. A selection
that runs no scenario fails. Preserve the verified image provenance and the
`target/e2e-container` and `target/quality-container` caches. CI still runs its
complete gate.

Every GUI or delegated check must clear inherited display variables and use a
private Xvfb and private D-Bus session. `scripts/test-headless.py` and
`scripts/qa.sh` enforce this isolation. Rust tests set `GTK_A11Y=none`,
`NO_AT_BRIDGE=1`, and `STRATA_REQUIRE_GTK_TESTS=1`; QA runs set `GTK_A11Y=none`
and `NO_AT_BRIDGE=1` too, since the process reads its own widget tree. Other
commands must arrange equivalent isolation. A missing isolated display or bus is
a hard failure. Never run against the desktop or an inherited session bus,
silently skip GTK tests, or fall back to the desktop.

### The pinned test image

The runners prefer Podman when available; select an engine explicitly with
`STRATA_CONTAINER_ENGINE=podman` or `docker`. CI explicitly selects Docker; an
image in one engine's store is not visible to the other. Normal runs verify and
reuse the local base image. If missing, they pull the published environment
once, verify its input label and platform, and record it locally. They **never
automatically build images or fetch distribution packages**. A missing
publication fails with an actionable message rather than silently bootstrapping.

When intentionally updating the environment, or before its first publication,
explicitly build the local base once:

```bash
STRATA_CONTAINER_ENGINE=podman python3 scripts/e2e_base.py build
./scripts/qa.sh
```

The recipe is `tests/container/Dockerfile`: a digest-pinned Arch Linux base, the
dated 2026-09-11 Arch package snapshot, GTK 4.22.5, GLib 2.88.3, GStreamer
1.28.7, and Rust 1.98.1. Pacman verifies the snapshot's signed packages. The
image replaces only GTK's versioned shared library with a matching-source build
carrying the accessibility-coordinate patch documented in
[`tests/container/gtk/README.md`](../tests/container/gtk/README.md); an
image-build probe checks the loaded library and rendered/accessibility
coordinates on a private display. `scripts/e2e_bundle.py image-key` hashes
those inputs, so changing them selects a new base; application edits do not.
The runners still compile the checked-out application and retain Cargo's
worktree cache. No host GTK libraries, fonts, desktop sockets, or Rust binaries
are mounted. Python in the image is the system `python3` with PyGObject, for the
coordinate probe and the helpers some scenarios start; there is no Python test
environment.

The checkout is mounted at `/workspace`. QA build and Cargo caches live in
`target/e2e-container` (quality's in `target/quality-container`), separately
from native builds, and artifacts are owned by the invoking user. Minimal
generated passwd/group files provide the invoking UID/GID to D-Bus, so one
published environment works across local user IDs without rebuilding it or
mounting the host's account database.
The image includes bubblewrap for sandboxed thumbnail decoding, FFmpeg/ffprobe,
and GTK's GStreamer media backend with the base/good/libav plugins. Rust media
regressions exercise actual normalization, playback, and long-source duration
limits rather than skipping when optional host tools are missing. These packages
come from the dated Arch snapshot. The accessibility patch is test-only and does
not apply or retire the version-specific GTK/GstPlay work in
`packaging/media-runtime/`; that opt-in kit remains unchanged and is not shipped
by this image.

Rootless Podman
runs unmask `/proc/*` inside the test container so bubblewrap can mount its own
private `/proc`; the decoder's sandbox and the container's seccomp policy remain
enabled. Docker's outer seccomp/AppArmor profiles and system-path masks must be
disabled for the nested namespace and mount operations. This applies only to the
disposable test container; Strata still launches its normal bubblewrap decoder
sandbox. Neither engine uses privileged mode or mounts desktop sockets.
Updating the image inputs is an intentional rendering
environment change and requires reviewing the visual baselines.

### Shared environment for formatting, lint, and Rust tests

`./scripts/quality.sh` runs all three quality phases in the same verified build
base. Use `./scripts/quality.sh fmt`, `clippy`, or `test` for an individual phase.
No separate quality image is necessary: the published environment already has
Rust 1.98.1, rustfmt, Clippy, native development libraries, and Xvfb. Quality now
uses that pinned compiler rather than moving `stable`.

CI resolves the public environment to a manifest digest, checks its input labels
and platform, and pulls that exact digest without registry credentials. Each
phase verifies the loaded image again and runs its immutable image ID. An
unavailable unchanged environment fails explicitly instead of installing packages.
For a deliberate recipe-input change relative to the PR base (or previous main
commit), CI may explicitly build that unpublished candidate from the pinned
recipe, without publishing it. This lets environment-update PRs pass before the
trusted-main publisher runs; it does not turn registry outages into repeated
distribution bootstraps. Locally, environment builds still require the explicit
command in "The pinned test image".

Quality's Cargo home and build directory are under `target/quality-container`,
separate from the QA cache and native builds. Its Actions cache is keyed by environment,
Cargo manifests, and source revision, with same-environment/manifest restoration.
Only successful quality builds on main pushes save caches; PR consumers cannot
populate main's cache. These are compilation caches, not evidence of passing tests;
the aggregate gate separately requires all test shards to pass. Cold quality
compilation includes test-only and all-feature dependencies, so the published
dependency layers are not a full quality hit.

The Rust suite runs with `--all-targets --all-features --locked` inside private
Xvfb, with `GTK_A11Y=none`, `NO_AT_BRIDGE=1`, and `STRATA_REQUIRE_GTK_TESTS=1`.
The shared Xvfb server uses `-noreset` so consecutive test processes do not
race a server reset when the last display client exits. GTK initialization
failures cannot silently skip tests. The container's `/tmp`, which holds `HOME` and
the tests' temporary folders, is an executable tmpfs: copy tests need `O_TMPFILE`
there, which an overlay `/tmp` may refuse. Formatting, compiler,
lint, and test failures remain blocking. Lightweight policy/helper jobs retain
their existing runners rather than downloading a large GUI image unnecessarily.

### Rust quality shards

CI's **Quality build and lint** job runs formatting and Clippy, then compiles
`cargo test --locked --all-targets --all-features --no-run` once. It exports only
test executables and a plan, not Cargo caches. `scripts/quality_ci.py` collects
each libtest inventory, including the explicitly ignored tests, and assigns every
entry to one of four shards. Timing hints in `scripts/quality-durations.json`
come from successful GTK child runs in
[run 34560353003](https://github.com/lgse/strata/actions/runs/34560353003).
Tests are balanced longest-first across the shards; unknown tests receive a
one-second weight and always participate. Timing hints are not an allowlist.
The deferred-scroll timing was refreshed to 1.32 seconds from
[run 34811363582](https://github.com/lgse/strata/actions/runs/34811363582).
`ui::search::tests::deferred_scroll_restoration_yields_to_updates_wheel_scrollbar_and_query_reset`
runs in its own libtest process before the other tests assigned to its shard,
not on a dedicated runner. Each process must pass its complete selection before
an aggregate shard receipt is written. Validation rejects a missing, duplicated,
or ignored isolated test; renaming or removing it requires updating `ISOLATED_TEST`.
Sharding does not shorten an individual test.

Each **Rust tests shard N** verifies the checkout revision, application/Rust-test source
fingerprint, image inputs, executable checksums, and the entire libtest inventory
before selecting exact names. Tests share neither a display nor a session bus
with another shard. The existing per-process GTK serialization still applies.
The runner checks libtest's actual selection and final passed/ignored counts
before writing a success receipt; nonzero exits, empty runnable shards, or changed
inventories fail. Existing `#[ignore]` entries stay explicitly accounted for;
sharding neither enables them nor silently ignores additional tests. Unsupported
non-libtest harness inventories fail closed rather than disappearing from coverage.
The QA runner, `tests/qa.rs`, has its own harness (`harness = false`, see
"Scheduling and run costs") that keeps libtest's `--list`, `--ignored`,
`--exact`, and `test result:` formats, so its scenario runs are inventoried as
explicitly ignored tests and its unit tests run in the shards like any other.

Published environments are pulled by the build job's exact manifest digest and
verified again by `quality.sh`. Deliberate unpublished recipe updates transfer
the same locally built environment as an attempt-scoped artifact instead; shards
never rebuild it. **Format, lint, and test** retains the required-check name,
requires successful build/lint and every shard, and verifies no missing, extra,
duplicate, or stale receipts. Failed shards are not retried, and `fail-fast: false`
preserves the other shards' results. Reports and logs are attempt-scoped artifacts.

To reproduce the handoff locally using the same pinned container:

```bash
STRATA_QUALITY_TASK=build ./scripts/quality.sh test
for shard in 0 1 2 3; do
  STRATA_QUALITY_TASK=shard STRATA_QUALITY_SHARD="$shard" ./scripts/quality.sh test
done
python3 scripts/quality_ci.py verify
```

The example runs shards sequentially for convenient local diagnosis; CI runs them
in parallel on separate runners. These environment variables are CI handoff modes,
not test filters; ordinary `./scripts/quality.sh test` still runs the complete
unsharded suite. Rebuild the bundle after changing source or checkout revision.
The two-shard matrix and `SHARDS` constant must be updated together if tuning
fan-out. Each shard has a ten-minute hang bound; timing is otherwise informational.

### Keep Rust test windows off the local desktop

Some Rust tests also create GTK windows when a display is available. Run the
complete suite, or a justified targeted selection, on the same private
Xvfb/private D-Bus infrastructure with:

```bash
./scripts/test-headless.py
./scripts/test-headless.py -- --nocapture
./scripts/test-headless.py relevant_module_or_test_name
```

This requires Xvfb, D-Bus, and AT-SPI on the host. It isolates application
preferences while retaining access to the installed Cargo/Rust toolchains, and
disables accessibility bridging for Rust tests. A startup failure aborts; it
never falls back to the real display or an inherited session bus. Its private
display, buses, and throwaway HOME come from `scripts/headless/`, which
`scripts/benchmark-scroll.py` uses as well.

### Native debugging dependencies

These are installed inside the canonical image; only `scripts/test-headless.py`
and other native debugging need them on the host.

| Purpose | Arch | Debian / Ubuntu |
| --- | --- | --- |
| Headless X server | `xorg-server-xvfb` | `xvfb` |
| Accessibility bus and registry | `at-spi2-core` | `at-spi2-core` |
| Private session bus | `dbus` | `dbus-daemon`, `dbus-bin` |
| Keyboard layouts | `xorg-setxkbmap` | `x11-xkb-utils` |
| Screen capture | `imagemagick` | `imagemagick` |
| Font | `cantarell-fonts` | `fonts-cantarell` |

## QA scenarios (`--qa` mode)

A build with the `qa` Cargo feature (never in releases) accepts
`strata --qa <directory>`: the normal window opens on the directory, then the
process reads one JSON command per line on stdin, runs it on the GTK main loop,
and writes one JSON reply per line on stdout after the main context drains.
Input is real XTEST sent from inside the process, and state is read from the
widget tree by the names `src/ui/accessibility.rs` sets, so no AT-SPI client is
involved.

```bash
./scripts/qa.sh                              # every scenario, every mode
./scripts/qa.sh arrow-keys-move-focus        # one scenario, all of its runs
./scripts/qa.sh arrow-keys-move-focus/columns-ru   # one run of it
STRATA_QA_KEEP_ARTIFACTS=1 ./scripts/qa.sh   # artifacts for passing runs too
```

`scripts/qa.sh` runs `cargo test --features qa --test qa` in the pinned image,
with the mounts, account files, and `target/e2e-container` cache it shares with
`scripts/qa-drive.sh` and `scripts/qa-mutation.sh` (through
`scripts/lib/qa-container.sh`). Because the cache is shared, run them one after
the other, never concurrently. `tests/qa.rs` refuses to start without the
`STRATA_QA_CONTAINER=1` that script sets. A host run needs the owner's explicit
consent for the task and `STRATA_QA_HOST=1`; it is labelled unsupported, is
never validation evidence, and must be recorded in the handoff with the
container run it replaced. The QA runner is not wired into
`scripts/test-headless.py`.

### How a run is set up

Each run of a scenario gets:

- its own Xvfb, at 1440x900x24 and 96 DPI;
- a fresh fixture tree in its own temporary directory under `/tmp`, with fixed
  modification times, materialised from the shared manifest
  `tests/fixtures/harness.json`, which also defines the seeded preferences,
  GTK settings, static environment, and keyboard layouts;
- a throwaway `HOME` and XDG directories, seeded with a complete
  `settings.toml` so no preference is inherited from a default change, and with
  the first-run file-chooser portal offer already answered;
- with `"session_bus":true`, a private D-Bus session with no service
  directories, so nothing is D-Bus-activated behind the run's back; without
  it, a bus address that cannot connect.

Rendering is pinned by the manifest: `GSK_RENDERER=cairo`, software GL,
`GDK_SCALE=1`, the Adwaita icon theme, Cantarell 11, `C.UTF-8`, `UTC`,
animations off, and `reduce_motion` on. Application preferences and file
operations use only the run's temporary HOME/XDG directories and fixture.
Both are under `/tmp` so trash capabilities do not vary with the caller's
`TMPDIR`. System libraries, fonts, and icon assets are provided by the
container; desktop endpoints and user configuration overrides are not
inherited.

The application's `MESA_SHADER_CACHE_DIR` names one directory the runner makes
under `/tmp` and removes when it exits, so the software GL shaders GTK compiles
at start-up are built once per `./scripts/qa.sh` rather than in every run's
fresh `XDG_CACHE_HOME`; helper and `foreign` processes a step starts run
without it. `scripts/qa.sh` mounts a tmpfs on the container's `/tmp`, which
holds every run's fixture, HOME, and runtime directory. `/tmp` is then a mount
point of its own, the one Strata resolves for a path there instead of `/`;
fixtures and HOME share one device, so Trash and its restore work within it,
and `/dev/shm` is another volume. Nothing in `/tmp` outlives the container,
since artifacts and the Cargo cache are under `/workspace/target`. The tmpfs
takes the kernel's default limit, half of RAM; a full run at 24 threads peaked
near 150 MB.

### Scenario files

A scenario is a `.jsonl` file under `tests/qa/scenarios/<feature area>/`, a
folder named in snake_case (lowercase letters and digits joined by single
underscores) without a `test_` prefix, such as
`tests/qa/scenarios/clipboard/copy-leaves-source.jsonl`. The runner finds every
file there itself. Scenario names are lowercase words joined by hyphens, since
`./scripts/qa.sh` reads `_` in a name as `-`, and are unique across folders; the
runner's `every_scenario_file_is_planned_and_well_formed` test checks that, that
each file sits in a well-named folder and loads, that a `mutation` names a patch
in `tests/qa/mutations/`, and that every run of every file is planned as a test.
Its first line is a header:

```json
{"scenario":"arrow-keys-move-focus","modes":["columns"],"prefs":{"type_to_search":false},"layout":"us","mutation":"keyboard-navigation","known_bug":null,"variants":[...]}
```

`modes` runs the file once per view, `mutation` names the patch in
`tests/qa/mutations/` that must make it fail (see "Proving a scenario catches a
regression"), and `known_bug` an issue that makes a failure the expected result.
A run is named `<scenario>/<label>`, its label the mode plus `-<variant>` for a
variant, as in `PASS copy-leaves-source/columns-ru`. The header rejects any key
it does not know.
The optional `quality`
(`{"impact":3,"coverage":3,"criticality":3,"diagnostic":3,"score":25,"note":"..."}`)
rates the scenario: `impact`, `coverage`, `criticality`, and `diagnostic` are
integers from 1 to 5, `score` an integer from 0 to 100, and `note` one
non-empty line without any line break the scripts split headers on
(`\n`, `\r`, U+000B, U+000C, U+001C–U+001E, U+0085, U+2028, U+2029); the
runner rejects any other key, a missing one, or a value out of range, and
checks the score exactly:
`P = 3(impact - 1) + 3(criticality - 1) + 2(diagnostic - 1)` and
`score = (50(coverage - 1)P + 32) // 64`, a mismatch failing the load with the
expected score. The `qa-quality` agent skill (`.agents/skills/qa-quality/`)
defines what the ratings mean.
A `skip` (`"skip":"<reason>"`) keeps a scenario without a `mutation` or
`known_bug` out of the default `./scripts/qa.sh` run, which prints
`SKIPPED <scenario> (run it by name): <reason>` in its place; naming it,
`./scripts/qa.sh <scenario>`, still runs it (see "Skipped scenarios").
A `known_bug` needs `known_bug_step`, the file
line (the header is line 1) of the step expected to fail, never a `layout`,
`flag`, `note`, `slice`, `increment`, `stem`, `parent`, `screenshot`, `restart`,
`http`, or `foreign_stop` step; a failure at any other
step, or an error, still fails the run. The header `layout` is switched to once the
process is ready, so key names always mean US key positions. Optional `variants`
run the file again with their own `prefs`, `layout`, `modes`, and `vars`, their
own `tree` and extra `setup` actions, and extra `teardown` actions taken after
the header's. In a step, `$name` is replaced by a variable (`$$` is a literal
`$`); the built-ins are `$root` (the fixture directory's name, which is also its
pane's), `$fixture` (its absolute path), `$fixture_quoted` (that path as
`pathlib.Path.as_uri` escapes it, without `file://`, for a trash file's `Path=`
or a URI), `$home`, `$data`, `$config`, and `$cache` (the run's HOME and XDG
data, config, and cache directories), `$python` (`python3`, the image's system
interpreter), `$environment` (the directory holding HOME and the XDG
directories), `$view` (`Columns`, `Icons`, `List`), and `$mode`.
An optional `teardown` lists `fs` actions taken once setup has begun, however
the run ends, to restore a mode a step changed. An optional `setup` lists `fs`
actions taken after the tree is written and before the launch, such as an
executable written into the tree or desktop entries and actions under `$data`
and `$config`. An optional `tree` names
a manifest tree or writes one out, where a name such as `{000..199}.txt` stands
for one zero-padded file per number whose text has `{}` replaced by the number,
and a file's text may be a list of parts: strings and
`{"repeat":"record-{},{}\n","from":0,"to":999}`, the text once per number;
`images` (`{"beta/thumb.png":{"width":64,"height":64,"rgb":[230,40,60]}}`)
then adds solid-colour PNG images, and `gtk_settings`
(`{"gtk-enable-animations":"true"}`) rewrites those lines of the manifest's
GTK `settings.ini`.
`env` (`{"PATH":"$environment/launcher-path"}`) adds or replaces variables
in the application's environment, with `$variables` replaced (never one the
runner owns: the display, buses, runtime, HOME and XDG directories, input log,
Mesa shader cache, and state-dump variables), and `"session_bus":true` gives
the run a private session bus with no service directories, which the
application and `dbus_send`, `launch`, and `foreign` steps share; without it
the application gets a bus address that cannot connect. `"portal":true` (with
`session_bus`) starts `strata --qa --portal`, the FileChooser portal backend,
in place of the browser: it has no window until a `portal_open` request opens a
chooser, which `adopt` then drives, and while no chooser is open its state
snapshot is the empty one.
`"fixture_root":"/tmp/strata-e2e-baseline"` builds the tree at that fixed
path, directly in `/tmp`, instead of a fresh one, because the breadcrumb and
context menu of a visual baseline render the path; a directory already there
is a setup error, never reused, and runs that claim a fixed root take turns.
`"cost":{"columns":2.4,"list":0.3}` gives each run, by its label, its measured
relative cost, which decides when it starts; `scripts/qa_cost.py` writes it
(see "Scheduling and run costs").
Every later line is one step; a step with `"modes":["icons","list"]` runs only
in those views; a step with `"variants":["row-space"]` runs only in those
header variants; a step with `"when":PREDICATE` runs only if the predicate
holds just before it (the runner asks the process with `{"holds":PREDICATE}`);
and a step with `"if":"returned"` or `"unless":"returned"` runs only if an
earlier `expect` (or `flag`) kept as `"as":"returned"` held, or did not, so a
scenario can collect failures and recover. A list (`"if":["a","b"]`) needs
every one to have held, and `"unless":["a","b"]` skips the step if any did; an
`expect` or `flag` that keeps a flag and is itself skipped this way keeps it
false, so a retry loop unrolls into guarded attempts.
A `note` keeps what an accessor reads: later steps say `$names`, which becomes
the value itself where a string is exactly `$names` (so a list stays a list)
and its text elsewhere (a list as Python prints it). A note may only be used
after the step that keeps it, may not hide a variable, and any other unknown
`$name` fails the scenario before it runs.

### Steps

| Step | Runs in | Meaning |
| --- | --- | --- |
| `{"key":"ctrl+x","repeat":2}` | process | US key positions; a tap comes up 20 ms after it goes down |
| `{"type":"nested-notes"}` | process | types the text, one key per character |
| `{"click":TARGET,"button":1,"count":1,"mods":["shift"],"at":[4,"50%"]}` | process | a click, a right click (`button` 3), or a double click (`count` 2), with the modifiers held; `"release_mods_first":true` releases the modifiers before the button comes up; `"left_bound":"browser"` (also on `move`) keeps the aim's left edge right of a remembered box, as `max(bounds.x, browser_left) + 12` |
| `{"select":{"entry":"readme.md"}}` | process | clicks until the entry is selected |
| `{"select_with_keyboard":"todo.txt","message":"..."}` | process | Home, then Down (Right in Icons) until the entry has focus |
| `{"open_menu":TARGET,"until":PREDICATE,"message":"..."}` | process | clicks, and clicks again each second the menu stays closed while the target is not checked; `"open_states":["checked","pressed"]` counts either state as open |
| `{"adopt":"PNG conversion test","timeout_ms":20000,"message":"..."}` | process | a portal run's chooser: waits for a showing application window with this title, and reads state, resolves targets, and logs input relative to it from then on |
| `{"reveal_control":{"role":"spin button","name":"Text size in pixels"}}` | process | finds the control presented (`"{name} control"`), then while its scroll pane does not show all of it turns the wheel 16 pixels inside the pane's right edge at its middle height, `min(12, max(1, int(distance / height) * 3))` clicks toward it (`"reachable {name} control"`), and settles it |
| `{"reveal":{"role":"button","name":"Modified date format"}}` | process | finds the control presented; unless the window and its scroll pane show it, turns the wheel three clicks at the window's centre up to twelve times, toward it, settling it once shown, then waits for it to be on screen (GTK's bridge declines the `scroll_into_view` before the loop) |
| `{"keep":[0,0,240,80],"as":"foreign"}` | process | keeps a box on the X screen for `kept` targets; the runner sends it for a `foreign` drag source's button |
| `{"activate":{"role":"toggle button","name":"..."},"message":"..."}` | process | the widget's default action, no pointer input; a role target without `within` may be scrolled away; one with `within`, or a `{"dialog_button":"Close dialog"}`, is looked up on screen first |
| `{"holds":PREDICATE}` | process | `true` or `false` in `result`, never a failure |
| `{"remember":{"field":true},"as":"filter"}` | process | keeps the focused field; any other target keeps its box, for `left_bound` and `moved`, and its widget, for `replaced` and a `remembered` target, apart from the fields, so a field lookup never resolves it |
| `{"note":"entries","dir":"$root","as":"names"}` | process | an accessor's value for later `$names`: `directory`, `pane`, `panes`, `entries`, `selected`, `all_selected`, `matches`, `focused`, `focused_pane`, `visible` (the rows wholly inside the pane's scroll viewport), `menu_items` (the open menu's items, empty with none), `editable_fields` (how many editable text fields are on screen, a number); `from`/`through` slice a list as `names[names.index(a):names.index(b) + 1]`, then `"middle":3` keeps `names[len // 2:len // 2 + 3]`, `"plus":["todo.txt"]` makes it `sorted(set(names) \| {...})`, `"sorted":true` sorts it, and `"pick":-1` keeps one name, `names[-1]`; `bounds` with `"of":TARGET` is its box as Python prints `screen_bounds()`, for a message; `"until_found":true` (with `message`, `timeout_ms`) polls until it is not null; `middle_visible` with `unless_first` is the reversing-scroll scenario's `middle_entry`; `scroll_target` with `"of":TARGET` is the pane's scroll value plus `max(of.y + of.height - viewport.y + 1, 1)` |
| `{"note":"number","expr":NUMBER,"as":"fitted"}`, `{"note":"visible_top","as":"rows"}` | process | a number worked out now (see `compare`); with `until_found` and `"until":[{"op":">","than":300}]` it polls until the number itself is within the bounds and keeps the value that poll read; the deepest pane's rows whose top edge lies inside its scroll viewport (a list, so `pick` and `middle` apply) |
| `{"increment":"first","as":"next"}`, `{"stem":"source","as":"stem"}` | runner | a noted name with its trailing number one higher, `f"folder-{n + 1:03}"`; a noted name's `Path(name).stem` |
| `{"clipboard_set":"selection-cleared"}`, `{"clipboard":"999","message":"..."}` | process | opens a second connection to the display and owns the clipboard with the text (for a scenario that copies a table, should one return); reads the clipboard through that connection (or a new one), closes it, and fails unless the text matches, exactly or as `{"starts_with":...,"contains":...,"longer_than":5}` |
| `{"slice":"rows","middle":3,"pick":-1,"as":"last"}` | runner | part of a noted list, so one snapshot serves several values: `middle` then `pick`, as a note shapes a list |
| `{"keys_until":PREDICATE,"keys":["Home","Left"],"times":2}` | process | presses the keys in turn for at most `times` rounds, stopping once the predicate holds right after a round, as `for _ in range(2): ...; if node.has_state(...): break`; `"check_first":true` checks before each round instead, as `for _ in range(30): if ...: break; press(...)`, and `times` may be a noted list, whose length it then is; `"await_focus_change":{"timeout_ms":5000,"message":"..."}` waits after each round for the focused widget to change, and `"exhausted":"..."` fails with that message when every round ran without the predicate holding, as a loop's `else: raise AssertionError(...)` |
| `{"move":TARGET,"at":[...]}` | process | moves the pointer to the target |
| `{"move_by":[2,0]}` | process | moves the pointer by an offset from where it is |
| `{"wheel":TARGET,"clicks":3,"direction":"down","at":[...]}` | process | turns the wheel over the target |
| `{"wheel":TARGET,"clicks":10,"until":PREDICATE,"tries":40}` | process | turns the wheel only while the predicate does not hold, checking before each turn: at most `tries` turns with no pause, or else (with `message`, `timeout_ms`) polling as a wait whose condition scrolls, failing on timeout |
| `{"key_down":"ctrl"}`, `{"key_up":"Escape"}` | process | one modifier or key, no tap and no pause |
| `{"during":PREDICATE,"ms":1200,"every_ms":50,"message":"..."}` | process | checks every `every_ms` (default 50) for `ms` and fails as soon as the predicate does not hold, as through a sustained hover |
| `{"capture":TARGET,"inset":3,"as":"before"}` | process | keeps the screen's pixels in the target's box, inset on every side, for `repainted`; any part off the screen is black, and so it is for `repainted` |
| `{"measure_text":"label","as":"resting"}` | process | `text_position` over a kept box from a fresh screen grab: the luma-weighted mean row of the ink and its contrast; fails as `the source label disappeared` when there is no ink, and is an error when the box leaves the screen (so for `drop_samples`) |
| `{"drop_samples":"label","resting":"resting","dragging":"dragging","escape":true,"as":"released"}` | process | the drag-completion release: Escape down and up when asked, then button 1 up, with no pauses, then grabs of the kept box 60, 120, and 180 ms later; each must come within 240 ms (failing with the samples printed), sit within 1.5 pixels of `resting`, and the last must have 1.25 times `dragging`'s contrast; the release instant is kept for `elapsed` |
| `{"drag":{"from":TARGET,"to":TARGET},"steps":16,"release":true,"mods":[]}` | process | a drag; `from` starts on its first rendered image, else its centre except a point-like `from` (`kept`, `point`, `pane`, `background`, `window`), which aims where it always does; `release:false` keeps the button held |
| `{"drag":{"from":TARGET,"from_at":[...],"to":TARGET,"to_at":[...]},"after_press":PREDICATE,"message":"..."}` | process | a drag between two points: the predicate is checked once after the press; a mismatch lets go and fails the step; `after_press` may instead list `{"wait":PREDICATE,"message":...}` and `{"expect":PREDICATE,"message":...}` steps, run in order, as a callback that waits and then asserts. Without `from_at`, the target is read before the source |
| `{"button_down":1}`, `{"button_up":1}` | process | one button press or release where the pointer is; `button_up` also ends an abandoned drag |
| `{"set_scrollbar":"horizontal","value":0}` | process | sets the value of the first scroll bar with that orientation; no pointer input |
| `{"sleep":600}` | process | sleeps 0.6 s; the main loop keeps running |
| `{"resize":{"width":420,"height":300}}` | process | resizes the main window; without `height`, its current height |
| `{"settle":TARGET,"timeout_ms":15000,"as":"entered"}` | process | waits for the bounds to repeat three polls and remembers them, under `as` too when given |
| `{"wait":PREDICATE,"timeout_ms":15000,"message":"...","samples":10}` | process | stops the run on timeout, with the widget tree; `samples` polls in a row must hold (a wait polls about every 50 ms); `"settling":TARGET` settles the target before every poll |
| `{"click_result_rows":{"selected":true},"mods":["ctrl"]}` | process | each of `result_rows` (the `Search results` rows, or else every list item) whose states match when its turn comes, clicked in DFS order |
| `{"expect":[PREDICATE,...],"message":"..."}` | process | reports mismatches and continues; `message` is the assertion message; with `timeout_ms` it polls until all hold first, and with `hold_ms` too they must hold that long in a row, else hold at the last look, as `_settles(condition, timeout, hold)`; `"as":"name"` keeps whether it held for a later step's `if` or `unless` |
| `{"query":"state"}`, `"tree"`, `"prefs"` | process | snapshot, widget tree, or `settings.toml` as JSON in `result` |
| `{"wait":PREDICATE,"never":PREDICATE,"never_message":"..."}` | process | the wait still fails if `never` held at any poll, the last included |
| `{"during":[{"check":PREDICATE,"message":"..."},...],"ms":7000,"every_ms":40}` | process | `during` over several assertions in order, each failing with its own message |
| `{"note":"entries","until_found":true,"until_names":NAMES,"as":"names"}` | process | polls until the list matches and keeps the list that poll read |
| `{"each":"names","as":"name","derive":{"caption":{"number_plus":320,"suffix":"×180"}},"step":STEP}` | runner | the process step once per item of a noted list, `$name` the item and each `derive` variable the last number in the item's stem plus `number_plus` between an optional `prefix` and `suffix` |
| `{"restart":true}` | runner | stops and starts the application: SIGTERM to its process group, 5 s, then SIGKILL; it starts again with the same environment, fixture, display, and input log, under the US layout, and a fresh log |
| `{"restart":{"directory":"$fixture/target","env":{"RUST_LOG":"trace"},"stdout":"closed","stderr":"null"}}` | runner | stops the application and starts `strata <directory>` itself: in that directory, with those variables added, and with standard output and error a pipe whose reading end is closed or `/dev/null`, as `Popen(stdout=PIPE)` closed at once or `DEVNULL`; the process sets them up itself (`STRATA_QA_STREAMS`) before anything writes, keeping its replies on their own descriptor |
| `{"launch":["$fixture/requested-missing"],"percent_path":true}` | runner | `subprocess.run([strata, *arguments], cwd=fixture, check=True, timeout=30)` with a helper's environment: another `strata`, which hands its arguments to the running one over the session bus and exits; `percent_path` makes `%XX` raw bytes |
| `{"portal_open":"Multiple selection test","options":{"current_folder":"$fixture","multiple":true,"filters":[["PNG images",[[1,"image/png"]]]]}}` | runner | waits up to 20 s for the backend's bus name, then sends `OpenFile` (60 s D-Bus timeout) from a worker thread without waiting for the answer; a request still unanswered when the run ends gets `Request.Close` once the final state is read |
| `{"portal_response":{"done":false},"message":"..."}`, `{"portal_response":{"done":true,"status":0,"names":[...],"paths":[...],"keep":"output","png":{...},"discard":true},"timeout_ms":20000,"message":"..."}` | runner | `assert not response.done()`; or waits for the answer and checks its status, the chosen files' names as a set or paths in order, keeps the first path as a note, checks it as `png` checks a file, and then (`discard`) removes its download directory however that went, as `finally: shutil.rmtree(output.parent)` |
| `{"http":"<base64>","as":"url"}`, `{"http_requests":1,"message":"..."}` | runner | `image_server(body)`: an HTTP/1.0 server on 127.0.0.1 answering every GET with the body as `image/png`, its URL kept as a note, until the run's final state is read; `assert len(requests) == 1, message` |
| `{"foreign":{"paths":["$fixture/a"],"move_only":true},"as":"foreign","message":"..."}`, `{"foreign_label":"Foreign drag finished","message":"..."}`, `{"foreign_stop":true}` | runner | `tests/qa/fixtures/file_drag_source.py` run unchanged through `tests/qa/foreign_drag_source.py`, which prints its button's screen box and each new label on stdout: waits for the box and keeps it in the process under `as` for a `kept` drag source; waits for the label; `terminate(source.popen)` |
| `{"zipcrypto":{"path":"password.zip","password":"zipsecret","member":"some.txt","contents":"...","deflated":true},"as":"collision"}` | runner | writes a ZipCrypto archive (a random 11-byte header, raw deflate at level 9) and finds a CRC collision: the first numeric password below 4096 whose decrypted header passes `zipfile`'s check, kept as a note; `contents` may be `{"repeat":"text","count":64}` |
| `{"fail":"the compress dialog never accepted the archive name"}` | runner | fails with the message, as a retry loop's last resort |
| `{"parent":"output","as":"output_dir"}` | runner | a noted path's `Path(name).parent` |
| `{"dbus_send":["--print-reply","--dest=...","/path","iface.Method","array:string:file://..."]}` | runner | `dbus-send --session ...` on the run's private bus (`session_bus`), with a helper's environment, failing as `subprocess.run(..., check=True, timeout=30)` |
| `{"log":{"count":"browser sandbox started"},"as":"initial"}`, `{"log":{"count":"...","at_most":2},"timeout_ms":0}` | runner | reads the application's log with its colour codes stripped: `count` occurrences of a text, `matches` (the numbers a pattern's first group finds, `re.findall`; `pick` keeps one), or `contains` (kept as a flag); with `as` it keeps the value as a note, without it waits until the count (or how many numbers) is `equals`, `at_least`, or `at_most` a number or a noted list's length, plus `plus` |
| `{"proc":{"gone":"$original"}}`, `{"proc":{"parent_is_app":"$pid"},"timeout_ms":0}`, `{"proc":{"kill":"$pid"}}` | runner | waits until none of the noted pids runs (`/proc/<pid>` is gone); checks `PPid:` in a pid's `/proc/<pid>/status` is the application's; sends it SIGKILL at once, refusing any pid that is not the application's child |
| `{"layout":"ru"}` | runner | `setxkbmap` and a group lock on the run's Xvfb |
| `{"flag":PREDICATE,"as":"enabled"}` | runner | keeps whether the predicate holds now, never a failure, for later steps' `if` and `unless`; with `timeout_ms` it polls until the predicate holds or time runs out |
| `{"fs":{"path":"archive/todo.txt","exists":true},"timeout_ms":15000,"message":"..."}` | runner | waits for `exists`, `text`, `names`, `is` (`file`, `dir`, `symlink`), `empty`, or `unchanged` (equal to a remembered listing); `timeout_ms` 0 is a plain assertion |
| `{"fs":{"path":"archive/todo.txt","write":"existing\n"}}`, `{"fs":{"path":".","remember":"original"}}` | runner | acts at once: writes a file, or keeps `fixture.listing()` under a name |
| `{"fs":{"path":"archive/folder-{000..159}","mkdir":true}}`, `{"fs":{"path":".","populate":TREE}}` | runner | `Path.mkdir()` (`"parents":true` for `mkdir(parents=True)`), once per number of a range in the last component; writes a tree as the header's `tree` does, and pins the times below `path` |
| `{"fs":{"path":"full/{000..179}.txt","write":"{}\n"}}`, `{"fs":{"path":"archive","chmod":"555"}}` | runner | one `write_text` per number of a range, `{}` in the text standing for it; `Path.chmod(0o555)` |
| `{"fs":{"path":"archive/report.md","copy_of":"documents/report.md"}}`, `"size_of"` | runner | waits until the file has the other file's bytes, or its size |
| `{"fs":{"path":".","remember_names":"original"}}`, `{"fs":{"path":".","names_from":"original","minus":["todo.txt"],"plus":[...]}}` | runner | keeps `set(fixture.names(path))`; waits until the names are that set less `minus` plus `plus` |
| `{"fs":{"path":"drop-volume","volume":"/dev/shm"}}` | runner | the cross-volume fixture: a fresh `strata-drop-volume-*` directory in `/dev/shm` (`"prefix":"strata-no-trash-"` names it otherwise, and `"with":{"doomed.txt":"doomed"}` writes files into it before it is linked), which must lie on another device than the fixture, linked at `path` and removed when the run ends, or by `{"fs":{"path":"no-trash","remove_volume":true}}`, which leaves the link dangling |
| `{"fs":{"path":"new file (1)","symlink":"missing"}}`, `{"fs":{"path":"run-me","program":"true"}}` | runner | `Path.symlink_to(target)`; `shutil.copy2(shutil.which("true"), path)`: the program's bytes, mode, and times |
| `{"fs":{"path":"$config/strata/settings.toml","has":{"contains":["\"$fixture/docs2\""],"lacks":[...]}}}` | runner | waits until the text contains every part and none of the others; an unreadable file contains nothing |
| `{"fs":{"path":".","names_without":"clicked"}}`, `{"fs":{"path":"no-trash/doomed.txt","can_trash":false}}` | runner | waits until no name contains the part, as `not any("clicked" in name ...)`; until GIO's `access::can-trash` is that value |
| `{"fs":{"path":"todo.txt","remove":true}}`, `{"fs":{"path":"documents","rename":"archive/documents"}}` | runner | `Path.unlink()`, or `shutil.rmtree` for a directory; `Path.rename(root / ...)`; both refuse a path whose parent, with links resolved, lies outside the fixture, the run's `$environment`, and its scratch directory |
| `{"fs":{"path":"$environment/launches","lines":{"at_least":1}}}`, `{"fs":{"path":"a.png","keep_bytes":"original"}}`, `"bytes_of":"original"` | runner | waits until the file has that many lines, exactly or at least (a missing file has none; an unreadable or non-UTF-8 one fails the step at once, as `read_text()` raises), as `len(path.read_text().splitlines())`; keeps `path.read_bytes()` under a name; waits until the bytes are those kept, failing at once on a read error |
| `{"screenshot":"hover.png"}` | runner | captures the Xvfb root; saved with the run's artifacts when it keeps them |
| `{"baseline":"columns-view"}` | runner | captures the Xvfb root with ImageMagick's `import` and compares it with `tests/qa/baselines/gtk-<major>.<minor>/columns-view.png` for the GTK the runner links: a pixel differs when a channel moves by more than 24, and more than 0.5% of differing pixels, a size change, or a missing image fails with a summary; the run's artifacts keep `<name>.actual.png`, and on a mismatch `<name>.expected.png` and `<name>.diff.png`. It writes a baseline only under `STRATA_QA_UPDATE_BASELINES=1` (see "Visual baselines"). Its failure starts with that summary, so the step carries no `message` |
| `{"fs":{"path":"photo.png","image":{"width":1600,"height":900,"rgb":[0,128,0]}}}`, `{"fs":{"path":"sound.wav","ffmpeg":["-f","lavfi",...]}}` | runner | a solid-colour PNG, written in place in one write (written in pieces, the listing would see it grow and replace its row), or `ffmpeg -v error -y <ffmpeg...> <path>` |
| `{"fs":{"path":"small.png","thumbnail":{"cache":"$cache/thumbnails/large","width":256,"height":128,"rgb":[0,0,255],"mode":"700"}}}`, `{"fs":{"path":"photos-a/model.3mf","cached_in":"$cache/thumbnails/large","pixel":{"at":[0,0],"rgb":[220,40,80]}}}`, `{"fs":{"path":"photos-a","all_cached_in":"$cache/thumbnails/large","except_suffix":".stl"}}` | runner | writes a freedesktop cached thumbnail for the file (a solid-colour PNG named after the MD5 of its URI, with `Thumb::URI` and `Thumb::MTime` text chunks, in one write, `mode` setting the cache directory's); `cached_in` reads the path's cached thumbnail instead for `exists`, `is`, `pixel`, `keep_mtime`, and `mtime_of`; waits until every entry but those ending in `except_suffix` has one |
| `{"fs":{"path":"a.png","pixel":{"at":[0,0],"rgb":[...]}}}`, `"keep_mtime":"stored"`, `"mtime_of":"stored"`, `"executable":true`, `"mtime":1000000000` or `{"local":[2001,9,9,1,46,40]}` | runner | `Image.open(path).convert("RGB").getpixel(at)`; keeps `st_mtime_ns` and waits until it is that again; `st_mode & 0o111 == 0o111`; `int(st_mtime)`, a local time read in the runner's zone as `datetime(...).timestamp()` |
| `{"fs":{"path":"unknown.bin","hex":"0001..."}}`, `"base64":"..."`, `"append":"text"`, `"raw_photo":true`, `"pdf_image":{"width":400,"height":200,"rgb":[...]}`, `"magick":["-limit","thread","1","$fixture/small.png"]` | runner | bytes in one write; `open("a").write`; a minimal raw camera photo; a one-page PDF of a solid image at 72 dpi; ImageMagick (`magick`, else `convert`) with the path last |
| `{"fs":{"path":"a.tar.gz","tar_gz":{"mode":"755","mtime":1000000000,"members":[["a.txt","text"]]}}}`, `"zip_members":[["todo.txt","todo.txt"]]`, `"tar_members":[...]`, `"is_zip":true` | runner | a gzipped tar as `tarfile.open(path, "w:gz")` with `TarInfo` modes and times; the archive's member names are exactly these in order, each holding the bytes of the named fixture file; `zipfile.is_zipfile` |
| `{"fs":{"path":"$config/mimeapps.list","default_app":{"for":"todo.txt","is":"x.desktop"}}}`, `{"fs":{"path":"$environment/opened-files","opened":["$fixture/todo.txt"]}}` | runner | the first desktop file `[Default Applications]` lists for the content type GIO gives the file; the file's lines, read with `Gio.File.new_for_commandline_arg`, are as many as the paths and include each |
| `{"fs":{"path":"action.toml","toml":{"run.args":["a"]}}}`, `"toml_like":"saved"`, `{"fs":{"path":"record","json":{"action":"keep"}}}`, `"json_set":{"paths":[...]}` | runner | values at dotted keys of the parsed TOML; the parsed TOML equals that of bytes `keep_bytes` kept; values at top-level JSON keys, or each list compared as a set |
| `{"fs":{"path":"$environment/terminal-launches.log","launches":{"count":3,"within":"$fixture"}}}`, `{"launches":{"first_dir":"$fixture/documents"}}`, `{"fs":{"path":"$data/Trash/info","replace_lines":{"suffix":".trashinfo","prefix":"DeletionDate=","with":"..."}}}` | runner | the stub terminal launcher's `cwd<TAB>--dir=...` lines as `test_terminal` asserts on them; rewrites matching lines of every file with the suffix |
| `{"fs":{"path":"documents/bad%E8name.txt","write":"x","percent_path":true}}` | runner | the path's `%XX` escapes stand for raw bytes, for names that are not UTF-8 |
| `{"fs":{"path":"payload.bin","random":67108864}}`, `"size":67108864`, `{"fs":{"path":".","glob_none":".strata-compression-*"}}` | runner | `write_bytes(os.urandom(size))` in one write; `st_size == size`; no name in the directory matches the pattern, `*` any run of characters, as `not list(root.glob(pattern))` |
| `{"fs":{"path":"$config/gtk-3.0/bookmarks","lines_equal":[...]}}`, `"last_line":"..."` | runner | `path.read_text().splitlines()` equals the list, a missing file having none; its last line is that |
| `{"fs":{"path":"settings.toml","toml_prefix":{"sidebar_order":["desktop","home"]}}}`, `"toml_like":"original","toml_keys":[...]` | runner | the list at the dotted key starts with these, as `tomllib.loads(...)["sidebar_order"][:2] == [...]`; only those keys of the TOML kept by `keep_bytes` and now compare equal |
| `{"fs":{"path":"todo.txt.zip","zip_member":["todo.txt","todo.txt"]}}`, `"png":{"size":[23,17],"at":[5,5],"min":[241,0,0],"max":[255,9,9]}` | runner | the zip's member holds the fixture file's bytes, as `archive.read(name) == path.read_bytes()`; the path ends in `.png`, is a PNG of that size, and the RGB pixel's channels lie within bounds |
| `"tar_gz":{"mode":"644","mtime":0,"members":[["a.txt",{"repeat":"x","count":50000}]],"flip_from_end":8}` | runner | a member's text repeated, and the compressed byte that far from the end flipped, as `damaged[-8] ^= 0xFF` breaks the gzip trailer's CRC |
| `{"fs":{"path":"a.3mf","zip":[["3D/3dmodel.model","text"],["thumb.png",{"image":{...}}]]}}`, `{"fs":{"path":"big.stl","truncate":134217729}}`, `{"fs":{"path":"page.pdf","from_repository":"tests/fixtures/documents/text.pdf"}}` | runner | a stored zip of the entries in order, as `zipfile.ZipFile(path, "w").writestr` (an entry may be `{"text":...,"date_time":[2001,9,9,1,46,40],"mode":"100755"}`, a `ZipInfo` with that time and `external_attr`); an `image` with a range in its name (`photo-{0..5}.png`) writes one file per number, `width_step` wider each, each followed by its `thumbnail` when given; `path.open("wb").truncate(size)`; the repository file's bytes in one write, as `write_bytes(... .read_bytes())` |

The thumbnail, archive, byte, append, line-rewriting, generated-file, and
record `fs` kinds, like `remove` and `rename`, refuse a path outside the
fixture, the run's `$environment`, and its scratch directory. A cached
thumbnail's name and `Thumb::URI` use the path escaped as
`pathlib.Path.as_uri` escapes it.

`fs` paths are relative to the fixture directory unless absolute, so
`$data/Trash/files` is the run's isolated trash. Every timeout defaults to 15
seconds. A missing directory counts
as `empty`, as `not (path.exists() and any(path.iterdir()))` does. A `message`
on `select_with_keyboard` or `activate` names the step in its failure.

A click holds the button 100 ms; a double click's two clicks are 90 ms apart,
inside GTK's 400 ms double-click time, and a drag after a selecting click waits
600 ms so it is no double click. The pointer approaches a point in two steps, 6
pixels off and then on it, so GTK sees enter and motion rather than a jump. A target is never a screen point; it is resolved from
the widget's bounds in its native surface plus that surface's X origin, which
covers popovers, and is waited for up to 15 seconds.
The pointer aims only once that box comes through a painted frame unchanged: a
row a listing has just added or replaced can report a box before its first
allocation lands. A tapped key comes
up 20 ms after it went down, with the X server
timing the release (XTEST's delay), never after waiting for a frame: the
server autorepeats a key held past 660 ms, and a busy app could hold a frame
that long after a press that started a navigation.

### Targets

| Target | Resolves to |
| --- | --- |
| `{"entry":"todo.txt","dir":"archive"}` | the entry row by name in the pane for `dir`; any pane without `dir` |
| `{"result":"beta/match-note.txt"}` | a filter result by its path |
| `{"result_row":-1,"containing":"match-note"}`, `{"result_row":0,"name":"match-note.txt"}` | the `Search results` rows or else every list item: one by its index in DFS order (negative counts back), among those whose name contains the part or is the name, as `[row for row in rows if "match-note" in row.name][-1]` |
| `{"role":"button","name":"drafts","nth":-1}`, `{"role":"list item","name_ends_with":["/a","/b"]}` | the `nth` match of role and name (negative counts back); `name_ends_with` keeps names ending with any of the parts, as `node.name.endswith(f"/{name}")` |
| `{"focused_widget":true}`, `{"remembered":"focused"}` | the focused widget; the widget `remember` kept under the name |
| `{"role":"check menu item","name":"Folders first"}` | the first widget in the window with that role and name |
| `{"role":"button","name":"Next video","within":{"preview":true}}` | a role and name inside another target: the search starts at another target; without `name` any name matches, as `entry.find(role="panel")`, and `description` must match too when given; `"states":["selected"]` (also `focused`, `checked`, `pressed`, `sensitive`, `horizontal`, `vertical`) as `find(..., states={...})`, and `"name_matches":"^\\d+:\\d\\d$"` as `find_all(name_matches=...)`, a regular expression searched in the name |
| `{"role":["check box","toggle button","switch"],"name":"Confirm"}` | `next(node for node in find_all(name=...) if node.role in {...})`: a role list matches any of them |
| `{"role":"*","states":["selected"],"within":{"preview":true}}`, `{"role":"list item","has":{"role":"label","name":"top.txt"}}`, `{"role":"button","name":"Settings","rendered":false}` | `preview.find(states={"selected"})`: `*` is any role; a match that has a rendered widget the inner role target finds at or below it, as `archive_row` picks the row holding a label; `find(..., rendered=False)`, presented but perhaps scrolled away, which `state`, `description`, `activate`, and `remember` then read as it is |
| `{"row":"readme.md"}` | the first rendered list item with the name, else the first table cell |
| `{"other_window":"Strata"}` | a showing application window with this title other than the one the run started with, as a reveal opens a second browser window |
| `{"toplevel":"Copy to"}`, `{"field":true,"within":{"toplevel":"Copy to"}}` | an application window by its title, outside the browser window; inside it, the focused editable text, which `remember` keeps as a field |
| `{"sidebar":"Home"}`, `{"header":"Choose sort field"}` | the sidebar button with that name; the column-header button with that name |
| `{"menu_item":"New File"}` | the menu item with that name, in the open menu if there is one |
| `{"dialog_button":"Close"}` | the button with that name in the open dialog |
| `{"field":true}`, `{"field":"filter"}` | the focused editable text, or the one remembered as `filter` |
| `{"background":true,"dir":...}` | a blank point in the pane's entry list, so it takes no `at` |
| `{"pane":"archive"}` | the pane; aims at a blank point in it unless `at` is given |
| `{"viewport":null}` | the scroll viewport around a pane's entry list |
| `{"entry":"todo.txt","part":"label"}`, `"part":"icon"` | the entry row's name label; its icon image |
| `{"window":true}`, `{"window":true,"y_of":TARGET}` | the browser window; with `y_of`, the point's height is that target's centre, as `(window.x + window.width - 40, pane.center[1])` |
| `{"preview":true}` | the quick preview drawer |
| `{"search_result":"todo.txt"}` | a `Search results` row by name, or else any list item with it |
| `{"named":"Apply to All","within":{"dialog":true}}`, `{"dialog":true}` | a widget of any role with that name, inside another target; the open dialog |
| `{"container":"archive"}` | the list or grid inside a pane |
| `{"parent":TARGET}`, `{"ancestor":"scroll pane","of":TARGET}` | the target's parent; its nearest ancestor with the role |
| `{"kept":"edge"}` | a box `settle` or `remember` kept, or a point `remember` kept, from before the window changed |
| `{"point":[X,Y]}` | a point put together from targets, as `(pane.x + 2, container.y + 2)`; each coordinate is `{"of":TARGET,"at":OFFSET}` (its centre without `at`), `{"mid":[A,B]}` (`(a + b) // 2`), `{"max":[A,B]}`, or `{"extent":TARGET}` (its width or height taken as a screen coordinate, as `park_pointer` moves to `(width - 4, height - 4)`), each with an optional `"plus":40`, in whole pixels |
| `{"entry":"todo.txt","part":"metadata"}` | the row's first named label other than its name |

`at` is `[x, y]` from the target's top-left corner, each in pixels (negative
counts back from the far edge), a share such as `"80%"` or `"2/3"` (rounded
down, as `width * 2 // 3`), a share plus pixels such as `"2/3+3"`,
`"start-6"` or `"end+60"` for pixels from the near or far edge that may lie
outside the target, or `"end-1/6"` for a share back from the far edge; without
it the pointer aims at the centre. A share has no lower bound, so `"1/6"` of a row under 6 pixels tall is 0. Role names are AT-SPI's, with `push button` spelled
`button`, a plain box `panel`, a group `grouping`, a scale `slider`, and a password entry `password text`, as GTK's AT-SPI bridge
reports them; names are the labels `src/ui/accessibility.rs` records (an entry row's
is its list item's, and `set_labelled_by` joins its labels' names), a label's
own text, or children's names for buttons and menu items; a menu button's inner
toggle takes the menu button's name, as GTK names it, and a button with a text
label is named by that label, through the `labelled-by` relation GTK gives it,
before any label set on the button, unless `accessibility::clear_labelled_by`
dropped the relation or `set_labelled_by` replaced it. A list view's row is
selected as its selected state flag says, as GTK reports it. Tooltips never name a
target: a control that needs a name, such as an icon-only button, gets
`accessibility::set_label` with its tooltip text. The `tree` query prints both
for every widget, which is the quickest way to write a target. `absent` holds
only when the target is not found or not on screen; a lookup that breaks, such
as an unrealized surface, is an error.

### Predicates

Predicates read the window and the run's files:

| Predicate | Holds when |
| --- | --- |
| `{"directory":"archive"}`, `{"panes":[...]}`, `{"pane":"archive"}` | the focused pane's directory, else the deepest pane's; the panes' names, left to right; the focused pane's name |
| `{"entries":NAMES,"dir":...}`, `{"selected":NAMES,"dir":...}` | the pane's entry names; its selected names, in visual order |
| `{"entry":"a.txt","dir":...}`, `{"entry_gone":"a.txt"}` | the entry is listed; it is no longer listed |
| `{"focused":"a.txt"}`, `{"focused_pane":true}` | the focused entry's name; the focused pane; a name, `true` for any, or null |
| `{"dialog":"Unable to rename item"}` | the open dialog's name, `true` for any, or null for none |
| `{"menu_items":NAMES}`, `{"menu_items":{}}`, `{"menu_items":null}` | the open menu's items; `{}` is any open menu, null none |
| `{"preview":"beta source"}`, `{"preview":null}` | the open preview shows the text; no preview is open |
| `{"matches":NAMES,"dir":...}` | the search results, or else the pane's entries |
| `{"field":{"of":"rename","name":"Rename","text":"a","caret":4,"selection":[0,4],"focused":true}}` | a field node's `text`, caret offset, selection (null for none), and `focused` state |
| `{"editor_within_viewport":{"of":"rename"}}` | the remembered inline editor stays inside the browser viewport, as the long-name caret scenario checks |
| `{"pref":{"key":"browser_mode","value":"icons"}}` | the saved preference in `settings.toml`, parsed from TOML; null when absent; `"contains":"size"` instead holds for a list holding the string or a string containing it, and `"one_of":[1,2,3,4]` holds for any of the values |
| `{"state":{"target":TARGET,"selected":true,"focused":false,"checked":false,"pressed":true,"sensitive":true}}` | the widget has each state given; an active switch is `pressed` as well as `checked`, and `selected` is what `accessibility::set_selected` recorded, as for a tab, as GTK's bridge reports them (a list box row and a notebook tab report their own selection); `selectable` (a list box row that takes the selection), `editable` (an editable entry or text view), `focusable` (a widget that can take focus), and `busy` (what `accessibility::set_busy` recorded) too; a description is the recorded one, a picture's alternative text, or for a menu button's inner toggle the menu button's, as GTK describes it |
| `{"view_mode":"Icons"}`, `{"window_width":420}` | the first pane's presentation; the main window's width |
| `{"present":TARGET}`, `{"absent":TARGET}` | the target is, or is not, found on screen |
| `{"moved":{"target":TARGET,"direction":"up"}}` | bounds against the last `settle`; also `down`, `left`, `right`, or `none`; `none` with `"within":1` allows that much drift on both axes and keeps the size, as `assert_layout` |
| `{"any":[PREDICATE,...]}` | `a or b` |
| `{"not":PREDICATE}` | `not ...` |
| `{"all_selected":NAMES}` | every pane's selection, left to right |
| `{"focused":{"one_of":[...]}}`, `{"focused_in_matches":true}` | the focused entry is one of the names; it is one of the matches |
| `{"preview":true}`, `{"peek":true}` | a preview is open; a folder peek is open |
| `{"on_screen":TARGET}` | the target lies within the window's height |
| `{"blank":{"target":TARGET,"at":[...]}}` | the point lands on nothing inside the target but panels and fillers |
| `{"visible":NAMES,"dir":...}` | the rows wholly inside the pane's scroll viewport |
| `{"scroll_value":{"at_least":N},"dir":...}` | the first scroll bar showing in the pane's viewport, `scroll_bar.numeric_value >= N` |
| `{"band_selected":{"from":"start","to":"end"}}` | the visible rows whose name box ends above the kept point `to` and overlaps the band from kept `from` are all selected, and there is one |
| `{"focus_within":TARGET}` | the target or a widget inside it has focus |
| `{"resized":{"target":TARGET,"since":"sidebar","width":0}}` | the width against a kept box: exactly that much wider, or `{"min":30}` |
| `{"contains_point":{"target":TARGET,"point":TARGET,"axis":"x"}}` | `bounds.x <= x < bounds.x + bounds.width` for the point's coordinate |
| `{"footer_fill":{"ends":["001.txt","011.txt"],"total":180}}` | both ends listed, cleared, and on screen, and a label described as `"<total - cleared> of <total> items selected"` |
| `{"repainted":{"since":"before"}}` | `visibly_hovered`: a fresh grab of a capture's box differs from it outside the pointer's square, 2 pixels before the box's centre to 32 after |
| `{"contrast_below":{"of":"dragging","to":"resting","share":0.8}}` | `dragging_contrast < resting_contrast * 0.8` for two `measure_text` results |
| `{"elapsed":{"since":"released","ms":600}}` | `time.monotonic() - released >= 0.6` |
| `{"diagnostics_mention":["could not","permission denied"]}` | any of them, case-insensitively, in the accessible names and descriptions of every application window |
| `{"all":[PREDICATE,...]}` | `a and b` |
| `{"editable_fields":1}` | `len(window.find_all(role="text", states={"editable"}))` |
| `{"description":{"target":{"menu_item":"Copy"},"text":"Ctrl+C"}}` | `node.description == "Ctrl+C"`, or a pattern such as `{"contains":...}` |
| `{"mentions":{"target":{"dialog":true},"text":"Customize Folder"}}`, `"words":"doesn't support Trash"` | `"Customize Folder" in dialog.dump()`: some name or description in the subtree contains it; with `"role":"label"`, the name of some rendered widget with that role does, as `any("x" in node.name for node in dialog.find_all(role="label"))`; with `words`, some rendered widget's `f"{name} {text}"` has them with any whitespace between, as `re.search(r"doesn't\s+support\s+Trash", ...)`; with `equals`, some rendered widget's name with its whitespace collapsed is exactly that, as `" ".join(node.name.split()) == text` |
| `{"presented":{"role":"label","name":"Ctrl+Alt+M"}}` | `window.find(..., rendered=False) is not None` |
| `{"file":{"path":"$fixture/documents/notes.txt","exists":false}}`, `{"file":{"path":"$fixture/todo.txt","mode_bits":{"mask":"100","set":true}}}` | whether the path exists without following a final symlink, as the runner's `fs` `exists` checks it; or `bool(path.stat().st_mode & 0o100)`; read by the process so `any` and `all` can join it with window state |
| `{"focused_widget":"Custom color…"}` | the focused widget's name, exactly or as `{"contains":"Shortcuts"}`; null when nothing has focus |
| `{"inside":{"target":TARGET,"of":{"container":"rename-target"},"axis":"y"}}` | the target has width and height and lies inside the other's box, on both axes or just `axis` |
| `{"editor_within_window":{"of":"rename","pane":"$root"}}` | the narrow-window creation check: a remembered field wider than one pixel, starting at or right of `max(pane.x, 0)` and ending inside the window |
| `{"replaced":{"target":{"entry":"rename-target"},"since":"listed"}}` | the target now resolves to another widget than the one `remember` kept under `since`, as a reload rebuilds a listing's rows; for the waits in "Waits for the app's own timers" |
| `{"pixel":{"target":{"role":"image","within":{"result":"beta/thumb.png"}},"rgb":[230,40,60]}}` | `screenshot.getpixel(bounds.center)` equals the colour |
| `{"accent_pixels":{"target":{"field":"filter"},"inset":2,"more_than":100}}` | more than that many pixels in the target's box, inset on every side, are exactly the accent of the theme `settings.toml` names in `data/themes/catalog.toml` |
| `{"any_result_row":{"focused":true}}` | `any(row.has_state("focused") for row in result_rows(strata))`; `selected` too |
| `{"hit_path":{"mark":"search: txt","hits":{"todo.txt":"todo.txt"},"name":"todo.txt"}}` | `footer_hit`: of the panels beside the footer mark, the first whose rendered labels, joined, are one of the hits' paths shows `hits[name]`; a `name` the hits do not list is an error |
| `{"compare":{"left":NUMBER,"op":"<=","right":NUMBER}}` | `heading.x + heading.width <= button.x`: `op` is `<`, `<=`, `==`, `!=`, `>=`, or `>`; a `NUMBER` is a literal, `{"of":TARGET,"get":"width"}` (`x`, `y`, `width`, `height`, `right`, `bottom`, `center_x`, `center_y` in whole pixels, or `value`, a slider's `numeric_value`), `{"pref":"text_size","default":0}`, `{"count":{"role":"image","description":"LaTeX equation","within":TARGET,"rendered":true,"states":["selected"]}}` (`len(find_all(...))`), or `sum`, `minus`, `times`, `divide`, `floor_divide` (Python's `/` and `//`), `ceil`, and `abs` over them; a target not on screen is a mismatch, so a wait keeps polling |
| `{"panes_with":{"target":{"role":"button","name":"Refresh (F5)"},"names":["documents"]}}` | `[name for name in pane_names() if pane(name).find(...)]`: the panes, left to right, in which the role target is on screen |
| `{"largest_image":{"within":{"preview":true},"wider_than":40,"size":[160,80]}}`, `"exceeds":[160,80]` | of the rendered images inside the target wider than `wider_than`, the largest by area (the first of equals) exists, is exactly `size`, or is larger than `exceeds` on either axis |
| `{"children":{"comm_prefix":"xdg-terminal-ex","count":0}}` | how many of the application's child processes have a command name with the prefix |
| `{"list_sections":{"within":{"dialog":true},"section":"Recommended Applications","names":NAMES}}` | the selectable list items under each unselectable heading row, named by the row's first named child |
| `{"text":{"target":TARGET,"text":"batch-rename"}}` | `node.text`, the text interface: a label's or entry's text, a text view's buffer |
| `{"some_window":{"title":"Strata","holds":PREDICATE}}`, `{"windows":{"title":"Strata","count":6}}` | `any(... for window in application.find_all(role="frame", name="Strata"))`: the predicate read in each showing application window with the title as its own tree; how many there are |
| `{"child_names":{"of":{"parent":{"sidebar":"Home"}},"role":"button","names":NAMES}}` | `[row.name for row in node.children if row.role == "button"]`, the target's visible children |
| `{"focus_outside":TARGET}`, `{"focused_widget_is":TARGET}` | the focused widget has a name and is neither the target nor inside it; the focused widget is the target |
| `{"alive":"rename"}`, `{"subtree_text":{"target":{"dialog":true},"contains":"...","strip_newlines":true}}` | `node.alive` for a widget `remember` kept: it still belongs to a window; `" ".join(node.text for _, node in dialog.walk() if node.text)` contains the part, after `.replace("\n", "")` with `strip_newlines` |
| `{"name":{"target":TARGET,"text":"0:05"}}`, `{"editable_text":"todo"}`, `{"visible_top":NAMES}` | `node.name == "0:05"` (or a pattern; with `"rendered":false` the target is looked up presented, as `find(..., rendered=False).name`); `any(node.text == "todo" for node in window.find_all(role="text", states={"editable"}))`; the rows `note visible_top` lists |

`NAMES` is an exact list in visual order or
`{"contains":[...],"excludes":[...],"count":4,"at_least":2,"ends_with":[...]}`,
with `"first"` (`names[0] == ...`), `"first_above"` (`names[0] > "010.txt"`),
`"any_from"` (some name `>= "040.txt"`), and
`"starts_with":[...]` (`names[:2] == [...]`),
`"toggled":{"names":[...],"against":[...]}` (each listed exactly when
`against` does not list it), `"in_order":[...]` (each listed, first
occurrences in that order), `"set":[...]`
(`set(names) == {...}`), `"occurrences":{"a.txt":2}`
(`names.count("a.txt") == 2`), and `"sorted_casefold_from":1`
(`names[1:] == sorted(names[1:], key=str.lower)`), comparing names as Python compares strings;
`panes` takes one too. `moved` takes `"since":"entered"` to compare with a box
`settle` or `remember` kept under that name, and with direction `none`,
`"y_within":2` allows that much vertical drift and ignores the rest, as
`abs(restored.y - y) <= 2`. A `field` check also takes `showing` (the field is
mapped), and `text` also `{"lowercase_contains":...}`. A `field` check reads the
field remembered as `of`, else the first on screen with that accessible `name`
(never both), else the focused one; each property it gives must match, `text` exactly or as
`{"starts_with"|"ends_with"|"contains":...}` and `caret` exactly or as
`{"min":1,"max":72}`. Caret and selection come from `gtk::Editable`, which a
`GtkEntry` such as the inline rename editor delegates to its `GtkText`, so they
are the offsets AT-SPI's text interface reports.

### Accessible names are product surface

The runner finds an entry because Strata names it. Those names live in
`src/ui/accessibility.rs` and exist for screen readers first: an entry row is
labeled with its name and described as `Folder` or `File`, a pane is labeled
with its directory and described with its presentation, the focusable surface
of an empty, unreadable or loading pane is labeled with its directory and
described with its status text, menu items carry the
menu role with the accelerator in the description, and modal dialogs carry the
dialog role and a name. When a scenario cannot identify a control, the fix is
to name it in the application, not to reach around the accessibility layer.
Locate controls by role and name, never by a literal screen coordinate, and
synchronize on conditions (`wait`, `fs` with a timeout), never on `sleep`
where a condition can be read. Check the filesystem as well as the window for
every file operation.

### Keyboard layouts

A `{"layout":"ru"}` step switches the run's Xvfb to a layout from the
manifest while the application runs: `setxkbmap -display` loads the layouts,
and their XKB variants when the layout names them, and an XKB group lock
selects the active one, as a layout switch shortcut does. The header's
`layout` is switched to once the process is ready. A `{"layout":"us"}` step,
and the end of a run under another layout, unlock the group and every
modifier and restore the US layout without variants; a keyboard not back in
that state fails the run. Never
change the desktop's layout, and never run `setxkbmap` without `-display`.

Key names mean US key positions: under `ru`, `{"key":"ctrl+c"}` presses the
key that types с, and `type` types what those keys produce. The `altgr`
modifier presses the US right Alt key, which layouts with AltGr turn into it:
under `cn-altgr-pinyin`, `altgr+1` types the macron dead key. The runner starts
no input method daemon. GTK's built-in input method composes dead keys in text
fields, so a pending Pinyin tone mark stands in for an input method's
unfinished composition when a scenario checks which keys reach it.

### Scheduling and run costs

`tests/qa.rs` makes every run of every scenario a test of its own, named
`<scenario>/<run>` as in its `RUN` line (`copy-leaves-source/columns-ru`), so
the runs of one scenario spread over the threads, each with its own Xvfb and
process. Its harness, `tests/qa/harness.rs` (`harness = false`), takes
libtest's command line and output but starts the tests in cost order, not
alphabetically: costliest first, a run without a `cost` before all of them
(it may be costly), and equal costs by name. It runs `--test-threads` tests at
once, one per CPU by default. A run with a `fixture_root` never starts while
another such run is going; the next run free to start takes the thread
instead of waiting. A run's cost outside its steps (Xvfb, the launch up to
the first reply, and shutdown) is about 0.4 s at 24 threads, so it takes no
display lock and no 50 ms exit poll: concurrent Xvfb starts begin at
different display numbers, skip one with a lock file or socket, and try the
next when another server takes theirs first, as `-displayfd` shows; an
exiting application, Xvfb, or bus is awaited on its pidfd; and `strata --qa`
checks that its window is ready after each frame it paints. With `--exact`,
which `./scripts/qa.sh` adds when it names anything, a scenario name selects
all of its runs and never another scenario it prefixes, and
`<scenario>/<run>` selects one run; without it a filter is a
substring, as in libtest. `./scripts/qa.sh --skip <scenario>` (or
`<scenario>/<run>`, in either spelling) passes the harness's own
`--skip-exact`, which leaves out that scenario or run and never another
scenario it prefixes; any other `--skip` is libtest's substring, or a whole
name once a scenario is named. Nothing is captured: as each test finishes, the
harness prints its result line with the time it took,
`test <name> ... ok <12.345s>` (or `FAILED <12.345s>`), and the run's `PASS`
or `KNOWN BUG` line, each line whole, and at the end the failure details and
libtest's `test result:` summary. `--list` prints the selected tests in cost
order, which is the order they are offered in; a run with a `fixture_root`
may start later while another such run is going. `--help` lists the options;
libtest's `--report-time`, `--shuffle`, and `--shuffle-seed` are accepted and
change nothing, as cost order wins.

The runner's own unit tests live in `tests/qa/scenario_tests.rs` and
`tests/qa/step_tests.rs`, and each must be listed in the `unit_tests!` macro in
`tests/qa/unit_tests.rs`: with the harness replaced, `#[test]` and
`#[cfg(test)]` there are stripped without a warning and never run, so the
`no_test_attribute_hides_a_runner_test` test rejects them in `tests/qa.rs` and
`tests/qa/*.rs`.

A header's optional `cost` maps every run label of the scenario, and nothing
else, to a positive number: the run's median wall-clock time over several full
runs divided by the median of all runs' medians, so 1.0 is a typical run,
rounded to one decimal place and at least 0.1. The runner rejects a `cost`
that misses a run or names another. Refresh the costs after adding scenarios
or runs, or when the order no longer fits:

```bash
python3 scripts/qa_cost.py run                               # three full runs, then every header's cost
python3 scripts/qa_cost.py run --repeat 5
python3 scripts/qa_cost.py write target/qa-cost/run-*.log    # again from saved qa.sh logs
```

`run` times full `./scripts/qa.sh` runs at the default thread count, naming
every scenario so a skipped one is timed too, and keeps their logs in
`target/qa-cost/`. A failed run's time counts like a passing one's, and
nothing is written unless every planned run has a time. Only the `cost` value
of each header line changes, added last where there is none; every other byte
of the files stays. The order matters more than the numbers: a new run without
a cost starts early until the next refresh.

### Replies and the state snapshot

A reply is `{"ok","app","changed","criticals","result","failures","error"}`.
`changed` lists the `app` keys that differ from the previous reply, and any GLib
warning or critical in `criticals` fails the run. `app` has exactly these keys:

```json
{
  "view_mode": "Columns",
  "directory": "strata-qa-fixture-ab12CD",
  "panes": [
    {"name": "strata-qa-fixture-ab12CD", "selected": ["archive"], "focused": null},
    {"name": "archive", "selected": [], "focused": "todo.txt"}
  ],
  "focused": "todo.txt",
  "focused_pane": "archive",
  "dialog": null,
  "menu": null,
  "preview": null,
  "field": null,
  "matches": ["todo.txt"]
}
```

- `view_mode`: the first pane's presentation (`Columns`, `Icons`,
  `List`), or null with no pane on screen.
- `directory`: the focused pane's directory, else the deepest pane's; `focused_pane`: the focused pane's name.
- `panes`: the panes left to right; per pane its name, its selected names in
  visual order, and the name of its entry holding focus or null. As through
  AT-SPI, a focused widget that is not mapped, such as a row a list keeps
  while it is scrolled away, holds no focus.
- `focused`: the name of the entry holding focus.
- `dialog`: the open dialog's name, or null.
- `menu`: the items of the open menu, or null when none is open.
- `preview`: every non-empty accessible name and text inside the open preview
  drawer in tree order, or null when it is closed.
- `field`: the focused editable field as
  `{"name","text","caret","selection","focused"}` (`selection` is
  `[start, end]` or null), or null.
- `matches`: the search results, or else the entries, of the deepest pane.

The snapshot, every target, and the input log's origin belong to the browser
window the run started with; a floating chooser, which becomes the active window while it is
open, is reached through a `toplevel` target.

The runner checks every step with the process's own parser before launching,
so a typo fails the scenario as a harness error. Setup, launch, layout,
artifact, and teardown problems, a reply that never arrives, and a process that
exits are errors too: they fail even a `known_bug` run, which only scenario
steps can satisfy.

A failed run writes `target/qa-artifacts/<scenario>/<mode>[-<variant>]/`: a
screenshot of the Xvfb root, the widget tree (headed by the window's focus,
marked when it sits on a widget that is not mapped), the command and reply
transcript, the application log, the input log, the fixture listing, the
Xvfb log, `state.json` (the `app` snapshot after the last step, taken once two
reads 250 ms apart agree, at most five seconds), and `result.json` (the run's
scenario, run, mode, errors, and each failed step with its line). Set
`STRATA_QA_KEEP_ARTIFACTS=1` to keep them for passing runs too.

### Driving a session by hand

`scripts/qa-drive.sh` keeps one `strata --qa` session open in the pinned
image, for checking a change interactively before (or instead of) writing a
scenario. `start` starts a detached container with the same engine, image,
mounts, account files, and `target/e2e-container` cache as `scripts/qa.sh`
(both take them from `scripts/lib/qa-container.sh`), makes the
build `qa.sh` makes (`cargo test --features qa --test qa --no-run`), seeds a
fresh HOME, XDG directories, and fixture from `tests/fixtures/harness.json` as
the runner does, starts Xvfb on `:90`, and launches the application in the
fixture with the runner's environment; it prints the launch reply. A start
that fails at any point removes its container. Each later subcommand is a
short `docker exec` (or `podman exec`) that returns at once:

```bash
./scripts/qa-drive.sh start --tree '{"notes":{"plan.txt":"plan\n"}}' --pref browser_mode=list
./scripts/qa-drive.sh send '{"select":{"entry":"notes"}}' '{"query":"state"}'
./scripts/qa-drive.sh screenshot menu.png      # target/qa-drive/menu.png
./scripts/qa-drive.sh restart --pref text_size=large
./scripts/qa-drive.sh exec 'ls -A; cat "$XDG_CONFIG_HOME/strata/settings.toml"'
./scripts/qa-drive.sh log
./scripts/qa-drive.sh stop                      # removes the container
```

`send` writes each process command on the application's stdin and prints its
one-line reply, waiting 90 s, or the command's `timeout_ms` plus 30 s; it
reports a process that exited with its log tail. Replies are matched to
commands by number, so the reply to a command whose wait gave up is skipped
when it comes, never printed as a later command's. Runner steps (`fs`, `layout`,
`screenshot`, `restart`, and the others marked "runner" above) are not process
commands: use the subcommands, or a scenario. `--tree` takes a tree as the
manifest writes one (a name, or JSON, or `@file`), `--pref KEY=VALUE` changes a
seeded preference (the value is JSON, or else a string), `--env NAME=VALUE`
adds a variable, and `--bus` starts the runner's private session bus in place
of the address that cannot connect. `exec` runs a shell command in the
fixture with the session's HOME, XDG directories, display, runtime directory,
and bus. `restart` closes the application's stdin, signals its group if it
has not quit within ten seconds (the runner's `EXIT_TIMEOUT` and
`TERMINATE_GRACE`), rewrites the given
preferences in its saved `settings.toml`, and launches it again on the same
fixture and HOME. Every exchange is appended to
`target/qa-drive/transcript.jsonl` in the artifacts' transcript format, and the
input log, build log, screenshots, and (after `stop`) application log land in
`target/qa-drive/`, which `start` empties. There is one session per checkout,
and it shares the build cache, so never run it while `scripts/qa.sh` runs.
The values it copies from `tests/qa.rs` (the QA environment, XDG layout,
Xvfb arguments, bus configuration, and timeouts) are checked by
`scripts/test_qa_drive.py`.

### Input log

With `STRATA_INPUT_LOG=<path>` the process appends every synthetic event it
sends, one per line; the runner sets it for each run and saves the file as
`input.log` whenever it writes artifacts. Coordinates are X root coordinates minus
the main window's X surface origin, so they are relative to the window (a portal
run's, to the adopted chooser):

```text
key <keycode> down|up
motion <x> <y>
pointer <x> <y> button <n> down|up
```

`pointer` is a button event at the pointer's position, with the wheel as
buttons 4 and 5, and keycodes are the X server's. `motion` lines record each
XTEST motion, including both steps of the approach and every drag point. The
position a `pointer` line carries is the final pointer position before that
button event.

### Visual baselines

The scenarios in `tests/qa/scenarios/visual_baselines/` compare a small set of
stable states with the images in `tests/qa/baselines/gtk-4.22` through a
`baseline` step: one canonical fixture in each view, the airy Icons spacing, a
hovered Icons tile, a
selection with focus, an open context menu, a confirmation dialog, and the
default Columns overflow. Local and CI runs use this one rendering profile;
other GTK versions have no baselines, so a run linking one fails rather than
silently accepting a different renderer. These scenarios claim the fixed
`fixture_root` `/tmp/strata-e2e-baseline`, because the breadcrumb and context
menu render the full path; runs that claim it take turns.

A capture matches when no more than 0.5% of pixels differ by more than 24 in
any channel, which absorbs the subpixel antialiasing that software rendering
varies between runs. On a mismatch the run's artifacts keep the expected,
actual, and diff images.

Baselines are never accepted automatically. When a change is intended,
regenerate them deliberately:

```bash
STRATA_QA_UPDATE_BASELINES=1 ./scripts/qa.sh columns-view-baseline
```

`scripts/qa.sh` forwards the variable; with it, each `baseline` step writes
its capture as the committed image and passes, and without it a run never
writes one. Name the baseline scenarios you mean to regenerate, then review
the new images and commit them with the change, so the difference is visible
in the pull request.

### Proving a scenario catches a regression

`scripts/qa-mutation.sh` applies one deliberate defect at a time from
`tests/qa/mutations`, rebuilds, and checks that the scenario whose header names
it as its `mutation` fails:

```bash
./scripts/qa-mutation.sh                      # every scenario with a mutation
./scripts/qa-mutation.sh clipboard            # the scenario a patch belongs to
./scripts/qa-mutation.sh copy-leaves-source   # a scenario by its name
```

Each patch breaks a single critical workflow: drop onto a folder, Ctrl+V
paste, arrow-key selection, single-click activation, the appearance menu,
filtered Space preview, filter-row updates, popover outside-wheel, the
long-name editor staying in the viewport, a live click-mode preference, cut
deferring its move to the paste, Return in the location editor, Keep Both
selecting the numbered copy, Enter, click-away and Escape in the inline
editor, a new entry colliding with an existing name, the rename error dialog,
Delete moving to the trash, and Shift+click ranges. Each patch belongs to
exactly one scenario.

The chosen scenarios first run once on the unmodified tree, and every run of a
scenario must pass, or its patch is not tried. Under the patch, the mutation
counts as caught only when at least one run fails a scenario step, no run
could not run or lost its result line, and `qa.sh` exits nonzero; a patch
that no longer builds runs nothing and is reported as missed. Logs land in
`target/qa-mutation/baseline/` and `target/qa-mutation/mutations/<patch>/`,
and `target/qa-mutation/summary.md` has one row per patch. The script restores
only the files the patch touched, refuses a patch whose files have uncommitted
changes, and refuses to run with `STRATA_QA_UPDATE_BASELINES=1`. It exits 0
when every mutation is caught. It shares the build cache with `qa.sh`, so
never run it alongside another QA command. Run it after changing the runner,
and when adding a scenario for a workflow that does not have a mutation yet.

### Skipped scenarios

A scenario whose pass or fail the app leaves to chance, so neither
`known_bug` nor a pass can be relied on, carries a `skip` reason until the app
is fixed; it stays in the suite and runs when named. No scenario is skipped
at present.

### Waits for the app's own timers

A QA run sends input faster than the retired Python suite did, whose AT-SPI
lookups took tens of milliseconds. Where that pace lets the app's own timers
fire in another order than they always did at the slower pace, the scenario
waits for what that pace guaranteed, and is listed here. The run counts below
for the Python suite were measured before it was removed:

- `same-folder-copy-numbered-duplicate` (folder): waits for the documents
  column the keyboard cursor mirrors after 75 ms before Ctrl+D. Over 24
  sequential Python runs the duplicate always ends selected in the root column
  alone.
- `paste-targets-selected-directory` (Columns): waits for the archive column
  the cursor mirrors before Ctrl+V. Over 24 Python runs the pasted file always
  ends selected and focused in that column.

- `escape-preserves-name-remaining` (existing-folder) and
  `rename-and-undo-restores` (folder): wait for the folder's column the
  cursor mirrors before F2. Over 24 sequential Python runs each ended in the
  same state every time, with that column open beside the folder wherever the
  test stayed in its parent.

These expose an app race: Ctrl+D, Ctrl+V, or F2 within 75 ms of the arrow key
that moved the cursor onto a folder leaves a stale mirrored column, and a paste
then leaves the pasted file neither selected nor focused.

- `rename-and-undo-restores`: settles the renamed row before Ctrl+Z. The
  directory monitor can list the new name, and the listing select and focus
  it, before the rename's own completion publishes its undo and ends the
  operation; a Ctrl+Z in between is dropped. The app shows nothing a run can
  read to tell that the undo is ready (Undo has no menu item or action
  state), so the wait is the settle. Under the same parallel load, 3 of 60
  QA runs without it lost the Ctrl+Z and none of 60 with it did (6 of 116
  and 0 of 84 over every probe). Over 24 Python runs of both kinds Ctrl+Z
  always restored the original.
- `new-folder-with-selection-groups` (Columns): waits for the renamed
  `grouped` row to count its two items before clicking it. The row first
  shows the optimistic rename, and a click on it before the listing catches
  up leaves focus in the parent column (16 of 24 QA runs without the wait,
  none of 36 with it); over 24 Python runs the click always focused the
  folder's column.
- `tenxer-d-without-trash-confirms`: waits for the `no-trash` link to be
  listed before walking the cursor onto it, which over 24 Python runs the
  fixture's own setup time always allowed.
- `narrow-window-keeps-last-column`: settles the `folder` column before
  comparing its right edge with the preview's. Once the 900-pixel window first
  fits the column, the column strip scrolls it clear of the preview a moment
  later; QA's pace compares in between (the column ended at 802 against the
  preview's 600, then at 600). Over 24 sequential Python runs the comparison
  always held.
- `empty-directory-focus-and-tab-order` (pointer): once the new `empty` row
  is listed after F5, waits up to a second for the listing to replace that
  row once more (it always did within a second over 36 probe runs) before
  clicking it; a click on the row the listing replaces mid-press is
  cancelled, and 4 of 12 QA runs without the wait lost it. Over 24 sequential
  Python runs of each pointer case the click always opened `empty`.
- `invalid-archive-reports-damage`: waits for the error detail label once the
  progress dialog has turned into `Unable to complete operation`, before
  asserting it; the label can show a moment after the dialog takes its new name,
  and one QA run in about fifty checked in between. Over 24 sequential Python
  runs of all seven cases the assertion always held at once.

## In CI

### QA in CI

The **QA scenarios** job in `ci.yml` runs every QA scenario on non-draft pull
requests and manual dispatches, and a failing scenario fails it. It waits for
**Quality build and lint**, pulls (or loads) the same pinned build environment
the Rust shards use, restores main's quality Cargo cache, and runs
`./scripts/qa.sh` with `STRATA_E2E_CARGO_CACHE=target/quality-container`.
`--features qa` equals `--all-features`, so that cache already holds every
dependency built in the same image at the same paths and only `strata` and the
runner compile. The job then checks the `qa` build contains
`usage: strata --qa`, the prefix of `USAGE` in `src/qa.rs` that the release
workflow rejects in release binaries; `scripts/test_qa_mutation.py` requires
both checks' pattern to prefix `USAGE`. A failed run uploads
`target/qa-artifacts` as `qa-artifacts-<attempt>`. CI does not run
`scripts/qa-mutation.sh`.

`scripts/qa.sh` accepts `STRATA_E2E_CARGO_CACHE`, a directory under `target/`
holding `cargo/` and `build/` (default `target/e2e-container`); leave it unset
locally. To read a failure, start with the job log (`PASS` and failed lines
name each run), then the artifacts: `qa-artifacts-<attempt>` has the per-run
bundle described in "Replies and the state snapshot".

### Persistent published environments

`Publish pinned E2E environments` (`e2e-images.yml`) publishes two GHCR
packages from trusted main when the image inputs, Cargo manifests, or
`.dockerignore` change, or on manual dispatch:

- `ghcr.io/lgse/strata-e2e-runtime`: the pinned GUI/font environment without
  Rust.
- `ghcr.io/lgse/strata-e2e-build`: Rust plus the runtime and compiled locked Cargo
  dependencies. A stub application warms the dependencies; it and its
  fingerprints are removed, and no tested application binary is published.

Tags include SHA256 input identifiers, architecture, UID, and GID. Runtime inputs
are independent of application source; the build identifier additionally includes
`Cargo.toml`, `Cargo.lock`, and `.dockerignore`. Existing publications are
checked before rebuilding. A separate `base-<environment-inputs>-linux-amd64-1001-1001`
tag in the build package is the reusable environment that `scripts/e2e_base.py`
pulls and CI's quality and QA jobs run. It is published once per environment
input set and is not retagged on ordinary Cargo changes. GHCR storage is
independent of Actions cache eviction; retain tags needed by supported
revisions rather than treating them as disposable per-run artifacts.

`install-packages.sh` configures only the dated HTTPS Arch Linux Archive mirror
and requires Pacman's package-signature policy. It refreshes the signed `core`
and `extra` databases once, in the `package-indexes` stage, then performs a
full snapshot upgrade and requested package installation as one transaction;
the runtime, patched-GTK, and toolchain stages inherit those exact indexes, and
none refreshes from a moving mirror.

Only the main-only publisher has `packages: write` and registry login credentials.
PR CI pulls anonymously and cannot publish shared images. After the first
publication, **make both packages public in GHCR package settings** and rerun
the publisher. Its anonymous-access check fails descriptively until they are
readable without credentials. Do not give fork PRs package-write credentials.
A pull request that changes `tests/container/` builds its unpublished
candidate environment in **Quality build and lint** and hands it to the shards
and the QA job as an attempt-scoped artifact; the publisher publishes it once
the change reaches main.

### Maintaining behavioral coverage

Before consolidating tests, identify the retained behavioral owner for every
assertion. Fewer test functions or scenario runs do not establish a runtime
improvement. Preserve functional layout, input-routing, filesystem-safety,
live-preference and lifecycle regressions, even when they share fixtures.
Record task-specific inventories and consolidation decisions in the issue or PR,
not in a committed audit report.
