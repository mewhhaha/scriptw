#!/usr/bin/env bash
# Builds a local dist `scriptw` compiler binary for benchmarks and long local
# runs: the `dist` Cargo profile (fat LTO, one codegen unit) with jemalloc,
# optionally with Rust PGO. CI release artifacts and Test262 publication still
# use `cargo build --release`. CONTRIBUTING (Dist compiler binary) records the
# cost, the limits and what was compared; docs/rust-rewrite/benchmarks.md has
# the measurements. A binary that links jemalloc must travel with
# THIRD-PARTY-NOTICES.md.
#
# usage: scripts/build-dist.sh [--check] [plain|pgo] [out-dir]
#   plain   one dist build (default)
#   pgo     instrumented dist build, training (scripts/dist-pgo-train.sh), merge
#           with llvm-profdata, then the dist build that uses the profile
#   --check validate the host, toolchain, environment and directories, take
#           the locks, print the plan and stop: nothing is built or deleted
#   out-dir default target/dist-out. The binary lands in <out-dir>/scriptw,
#           next to a copy of THIRD-PARTY-NOTICES.md, its provenance in
#           <out-dir>/scriptw.build-info and the PGO profile in
#           <out-dir>/scriptw-<cksum>.profdata. A run first removes the
#           previous binary, so a failed run leaves none behind: copy a binary
#           that a long run uses to a path of its own first. The shared
#           target/ and its debug, release and dist directories are refused,
#           and so is a directory holding a `scriptw` without a build-info.
#
# Linux x86_64 with lld only: the script hard-codes the host triple and
# `-fuse-ld=lld`. As in a release build, crates/scriptw-cli/build.rs links the
# binary non-PIE; CONTRIBUTING records the executable-ASLR trade-off. A PGO
# build is not reproducible (the profile depends on the training run), so its
# binary cannot resume or publish a Test262 sweep that another binary started.
#
# Environment:
#   SCRIPTW_DIST_FEATURES   cargo feature flags, default "--features jemalloc"
#   SCRIPTW_DIST_TARGET_DIR Cargo target directory, default target/dist-build;
#                   never the shared target/. Each non-plain step deletes the
#                   previous step's target units (about 1.8 GB) and keeps the
#                   host units (1.4 GB: build scripts, proc macros, the
#                   runtime-image producer).
#   SCRIPTW_DIST_JOBS       cargo -j. Default: SCRIPTW_JOBS, which the resource
#                   wrapper sets to 1, or to the CPU count it computes with
#                   --cloud-cpus auto.
#   SCRIPTW_DIST_LLVM_PROFDATA  default: rustup's llvm-tools copy for this
#                   toolchain (`rustup component add llvm-tools`); it must
#                   match rustc's LLVM.
#   SCRIPTW_DIST_BOLT_PREP  1 links the PGO binary with --emit-relocs and
#                   -z keep-text-section-prefix for a later llvm-bolt pass.
#                   Off by default: BOLT is not part of the recipe, and the
#                   relocations add about 19 MB that every CLI run hashes.
#   SCRIPTW_DIST_PGO_APPLICATION_FIXTURES, SCRIPTW_DIST_PGO_SUITE_ROOT
#                   the training inputs; see scripts/dist-pgo-train.sh.
# The script owns the target rustflags: it refuses a pre-set RUSTFLAGS,
# CARGO_ENCODED_RUSTFLAGS, CARGO_BUILD_RUSTFLAGS or
# CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS, which it would otherwise
# drop or which would drop its own flags.
#
# Run it under the resource wrapper with --cloud. The fat-LTO link alone peaks
# at 3.8 GB RSS, which does not fit the 4 GiB local default; a local run needs
# an explicit `--memory-mib 6144` or more, outside the AGENTS.md local default.
#   python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
#     scripts/build-dist.sh pgo
#
# All builds pass --target <host>: the rustflags then reach only the target
# units, so build scripts are neither instrumented nor profile-optimized.
# Because they replace the [target] rustflags of .cargo/config.toml, the lld
# link argument is repeated here.
set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//'; exit 2; }

check_only=0
if [[ ${1:-} == --check ]]; then check_only=1; shift; fi
mode="${1:-plain}"
case "$mode" in plain | pgo) ;; *) usage ;; esac
(($# <= 2)) || usage

for var in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS; do
  [[ -z ${!var+set} ]] || die "$var is set; build-dist.sh sets the target rustflags itself (always with lld)"
done
host="$(rustc -vV | sed -n 's/^host: //p')"
[[ $host == x86_64-unknown-linux-gnu ]] || die "build-dist.sh supports only x86_64-unknown-linux-gnu, not '${host}'"
command -v ld.lld > /dev/null || die "no ld.lld on PATH; build-dist.sh always links with lld"
command -v flock > /dev/null || die "no flock on PATH (util-linux)"

repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo"
mkdir -p "$repo/target"
shared_target="$(cd -- "$repo/target" && pwd -P)"
out="${2:-$repo/target/dist-out}"
target_dir="${SCRIPTW_DIST_TARGET_DIR:-$repo/target/dist-build}"
mkdir -p "$out" "$target_dir"
out="$(cd -- "$out" && pwd -P)"
target_dir="$(cd -- "$target_dir" && pwd -P)"
case "$target_dir" in "$repo" | "$shared_target" | /) die "SCRIPTW_DIST_TARGET_DIR must be a dedicated directory, not $target_dir" ;; esac
# A run replaces <out-dir>/scriptw, so never a binary other tools run by path.
case "$out" in
  "$repo" | "$shared_target" | "$shared_target/debug" | "$shared_target/release" | "$shared_target/dist" | /)
    die "the output directory must be a dedicated directory, not $out" ;;
esac
[[ ! -e $out/scriptw || -e $out/scriptw.build-info ]] || die "$out/scriptw has no scriptw.build-info, so this script did not write it"
[[ $out != "$target_dir/$host" && $out != "$target_dir/$host/"* ]] || die "the output directory lies inside the target units that the build deletes"
read -r -a features <<< "${SCRIPTW_DIST_FEATURES-"--features jemalloc"}"
jobs="${SCRIPTW_DIST_JOBS:-${SCRIPTW_JOBS:-1}}"
[[ $jobs =~ ^[1-9][0-9]*$ ]] || die "the cargo job count must be a positive integer, not '$jobs'"
bolt_prep="${SCRIPTW_DIST_BOLT_PREP:-0}"
[[ $bolt_prep == 0 || $bolt_prep == 1 ]] || die "SCRIPTW_DIST_BOLT_PREP must be 0 or 1"
[[ $mode == pgo || $bolt_prep == 0 ]] || die "SCRIPTW_DIST_BOLT_PREP applies only to the pgo mode"

# One run per target and output directory: the steps below delete target units,
# profiles and binaries that a concurrent run would still be using. Children
# (cargo, rustc, the training) inherit the descriptors and keep the locks.
exec 8> "$target_dir/.build-dist.lock"
flock -n 8 || die "another build-dist.sh run holds $target_dir"
if [[ $out != "$target_dir" ]]; then
  exec 9> "$out/.build-dist.lock"
  flock -n 9 || die "another build-dist.sh run holds $out"
fi

if [[ $mode == pgo ]]; then
  profdata="${SCRIPTW_DIST_LLVM_PROFDATA:-$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata}"
  [[ -x $profdata ]] || die "no llvm-profdata at $profdata; run 'rustup component add llvm-tools'"
  rustc_llvm="$(rustc -vV | sed -n 's/^LLVM version: \([0-9]*\).*/\1/p')"
  profdata_llvm="$("$profdata" --version | sed -n 's/.*LLVM version \([0-9]*\).*/\1/p' | head -1)"
  [[ -n $rustc_llvm && -n $profdata_llvm ]] || die "cannot read the LLVM major of rustc ('$rustc_llvm') or llvm-profdata ('$profdata_llvm')"
  [[ $rustc_llvm == "$profdata_llvm" ]] || die "llvm-profdata is LLVM $profdata_llvm, rustc is LLVM $rustc_llvm"
  if [[ -n ${SCRIPTW_DIST_PGO_APPLICATION_FIXTURES:-} ]]; then
    [[ -f $SCRIPTW_DIST_PGO_APPLICATION_FIXTURES/rsc.js ]] || die "SCRIPTW_DIST_PGO_APPLICATION_FIXTURES has no rsc.js"
  fi
fi

# The source state is read before anything is built, recorded in the
# build-info and read again at publication: a commit or an edit made during
# the run is flagged there instead of being attributed to this binary.
tracked=(Cargo.toml Cargo.lock .cargo rust-toolchain.toml crates vendor scripts)
source_state() {
  echo "source_commit=$(git rev-parse HEAD)"
  echo "source_changed_tracked_files=$(git status --porcelain --untracked-files=no -- "${tracked[@]}" | wc -l)"
  echo "source_tracked_diff_sha256=$(git diff HEAD --binary -- "${tracked[@]}" | sha256sum | cut -d' ' -f1)"
  echo "source_untracked_sha256=$(git ls-files --others --exclude-standard -z -- "${tracked[@]}" | sort -z | xargs -0r sha256sum | sha256sum | cut -d' ' -f1)"
}
initial_source_state="$(source_state)"

profiles="$out/profraw"
cargo_cmd=(cargo build -j "$jobs" --locked --profile dist --target "$host" -p scriptw-cli --bin scriptw "${features[@]}")
if ((check_only)); then
  echo "check passed: mode $mode, host $host, jobs $jobs, BOLT prep $bolt_prep"
  echo "  target dir $target_dir (locked), out dir $out (locked)"
  echo "  ${cargo_cmd[*]}"
  [[ $mode == plain ]] || echo "  llvm-profdata $profdata (LLVM $profdata_llvm); training fixtures ${SCRIPTW_DIST_PGO_APPLICATION_FIXTURES:-none}"
  exit 0
fi

# A failed or interrupted run removes its partial outputs and, in pgo mode,
# the flavoured target units. A plain build keeps its units so that a rerun
# resumes. A SIGKILL skips this; the next run starts by removing the same files.
cleanup() {
  local status=$?
  rm -rf "$out/scriptw-instrumented" "$profiles" "$out/.merged.profdata" \
    "$out/.scriptw.partial" "$out/.scriptw.build-info.partial"
  if ((status != 0)); then
    if [[ $mode == pgo ]]; then rm -rf "${target_dir:?}/${host:?}"; fi
    echo "build-dist.sh failed; $out holds no binary" >&2
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cleanup
rm -f "$out/scriptw" "$out/scriptw.build-info"

build() { # build <log-name> [rustflag...]
  local name=$1
  shift
  local flags=(-C link-arg=-fuse-ld=lld "$@")
  echo "== dist build $name (${*:-no profile flags})"
  # Target units built with other rustflags cannot be reused; drop them first
  # so two flavours never share the disk. A plain build resumes its own units.
  [[ $name == plain ]] || rm -rf "${target_dir:?}/${host:?}"
  # The encoded form keeps paths with spaces intact.
  env CARGO_TARGET_DIR="$target_dir" \
    CARGO_ENCODED_RUSTFLAGS="$(IFS=$'\x1f'; printf '%s' "${flags[*]}")" \
    "${cargo_cmd[@]}" > "$out/build-$name.log" 2>&1 || { tail -30 "$out/build-$name.log" >&2; exit 1; }
}

# Moves the last build's binary and its build-info into <out-dir> under their
# final names only once both are complete, then drops the target units.
# build_info_lines holds the mode's extra key=value lines.
publish() { # publish <rustflags description>
  cp "$target_dir/$host/dist/scriptw" "$out/.scriptw.partial"
  local unchanged=true identity
  [[ "$(source_state)" == "$initial_source_state" ]] || unchanged=false
  if [[ $unchanged == false ]]; then
    echo "warning: the source state changed during the build; scriptw.build-info flags it" >&2
  fi
  # The embedded source fingerprint and revision, as the binary reports them.
  identity="$("$out/.scriptw.partial" compiler-identity)" || die "the built binary cannot report its compiler identity"
  {
    echo "recipe=scripts/build-dist.sh $mode"
    echo "built_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "$initial_source_state"
    echo "source_unchanged_during_build=$unchanged"
    echo "compiler_identity=$identity"
    echo "rustc=$(rustc -V)"
    echo "host=$host"
    echo "cargo_profile=dist"
    echo "features=${features[*]}"
    echo "target_rustflags=-C link-arg=-fuse-ld=lld $1"
    printf '%s\n' "${build_info_lines[@]}"
    echo "binary_sha256=$(sha256sum < "$out/.scriptw.partial" | cut -d' ' -f1)"
  } > "$out/.scriptw.build-info.partial"
  cp "$repo/THIRD-PARTY-NOTICES.md" "$out/THIRD-PARTY-NOTICES.md"
  mv "$out/.scriptw.partial" "$out/scriptw"
  mv "$out/.scriptw.build-info.partial" "$out/scriptw.build-info"
  rm -rf "${target_dir:?}/${host:?}"
}

if [[ $mode == plain ]]; then
  build plain
  build_info_lines=("pgo_profile=none")
  publish ""
  echo "dist binary: $out/scriptw (provenance: $out/scriptw.build-info)"
  exit 0
fi

mkdir -p "$profiles"

# 1. Instrumented build.
build gen "-Cprofile-generate=$profiles"
cp "$target_dir/$host/dist/scriptw" "$out/scriptw-instrumented"
rm -rf "${target_dir:?}/${host:?}"

# 2. Training. %m merges every process into one .profraw online.
LLVM_PROFILE_FILE="$profiles/scriptw-%m.profraw" "$repo/scripts/dist-pgo-train.sh" "$out/scriptw-instrumented"
rm -f "$out/scriptw-instrumented"
n=$(find "$profiles" -name '*.profraw' | wc -l)
((n > 0)) || die "the training wrote no profile"

# 3. Merge. The previous profile stays until this one exists. Each non-plain
# build starts from empty target units, so Cargo cannot reuse units built with
# another profile; the checksum in the name keeps the flags of different
# profiles distinct for external caches such as sccache and names the profile
# in the build-info.
"$profdata" merge -o "$out/.merged.profdata" "$profiles"
merged="$out/scriptw-$(cksum < "$out/.merged.profdata" | cut -d' ' -f1).profdata"
mv "$out/.merged.profdata" "$merged"
for old in "$out"/scriptw-*.profdata; do [[ $old == "$merged" ]] || rm -f "$old"; done
rm -rf "$profiles"
echo "merged $n profiles -> $merged"

# 4. Optimized build. rustc only warns when it cannot use the profile.
# With SCRIPTW_DIST_BOLT_PREP=1, --emit-relocs keeps the static relocations (a
# non-loaded section) so that llvm-bolt can reorder functions, and
# keep-text-section-prefix keeps LLVM's .text.hot/.text.unlikely groups apart.
# Neither changes an instruction.
use_flags=("-Cprofile-use=$merged")
if ((bolt_prep)); then
  use_flags+=(-C link-arg=-Wl,--emit-relocs -C link-arg=-Wl,-z,keep-text-section-prefix)
fi
build use "${use_flags[@]}"
pattern='^(warning|error).*(profile-use|profile format|profdata|profile data|instrumentation profile)'
if grep -qE "$pattern" "$out/build-use.log"; then
  grep -E "$pattern" "$out/build-use.log" | head -3 >&2
  die "rustc did not use the profile"
fi
build_info_lines=(
  "pgo_profile=$merged"
  "pgo_profile_sha256=$(sha256sum < "$merged" | cut -d' ' -f1)"
  "pgo_training_script_sha256=$(sha256sum < "$repo/scripts/dist-pgo-train.sh" | cut -d' ' -f1)"
  "pgo_application_fixtures=${SCRIPTW_DIST_PGO_APPLICATION_FIXTURES:-none}"
  "pgo_suite_root=${SCRIPTW_DIST_PGO_SUITE_ROOT:-test262/vendor/test262}"
  "bolt_prep=$bolt_prep"
)
publish "${use_flags[*]}"
echo "dist PGO binary: $out/scriptw (provenance: $out/scriptw.build-info)"
