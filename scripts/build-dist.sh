#!/usr/bin/env bash
# Builds the shipped and benchmark `scriptw` compiler binary: the `dist` Cargo
# profile (fat LTO, one codegen unit) with jemalloc, optionally with Rust PGO.
# CONTRIBUTING (Shipped compiler binary) records its cost and the byte-identity
# gate; docs/rust-rewrite/benchmarks.md has the measurements.
#
# usage: scripts/build-dist.sh [plain|pgo] [out-dir]
#   plain   one dist build (default)
#   pgo     instrumented dist build, training (scripts/dist-pgo-train.sh), merge
#           with llvm-profdata, then the dist build that uses the profile
#   out-dir default target/dist-out. The binary lands in <out-dir>/scriptw;
#           the PGO profile in <out-dir>/scriptw-<cksum>.profdata.
#
# Environment:
#   DIST_FEATURES   cargo feature flags, default "--features jemalloc"
#   DIST_TARGET_DIR Cargo target directory, default target/dist-build. Each step
#                   deletes the previous step's target units (about 1.8 GB) and
#                   keeps the host units (1.4 GB: build scripts, proc macros,
#                   the runtime-image producer). Its fat-LTO link peaks at 3.8 GB RSS.
#   LLVM_PROFDATA   default: rustup's llvm-tools copy for this toolchain
#                   (`rustup component add llvm-tools`); it must match rustc's LLVM.
#   DIST_JOBS       cargo -j (default 1, the workspace default)
#
# Run it under the resource wrapper, e.g.
#   python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
#     env DIST_JOBS=4 scripts/build-dist.sh pgo
#
# All builds pass --target <host>: RUSTFLAGS then reach only the target units,
# so build scripts are neither instrumented nor profile-optimized. Because
# RUSTFLAGS replaces the [target] rustflags of .cargo/config.toml, the lld link
# argument is repeated here.
set -euo pipefail

mode="${1:-plain}"
case "$mode" in plain | pgo) ;; *) sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//'; exit 2 ;; esac
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
out="${2:-$repo/target/dist-out}"
mkdir -p "$out"
out="$(cd -- "$out" && pwd)"
target_dir="${DIST_TARGET_DIR:-$repo/target/dist-build}"
features="${DIST_FEATURES-"--features jemalloc"}"
jobs="${DIST_JOBS:-1}"
host="$(rustc -vV | sed -n 's/^host: //p')"
link_flags="-C link-arg=-fuse-ld=lld"

build() { # build <log-name> <extra rustflags>
  echo "== dist build $1 (${2:-no profile flags})"
  # Target units built with other RUSTFLAGS cannot be reused; drop them first
  # so two flavours never share the disk. A plain build resumes its own units.
  [[ $1 == plain ]] || rm -rf "$target_dir/$host"
  # shellcheck disable=SC2086 # features is a list of cargo flags
  env CARGO_TARGET_DIR="$target_dir" RUSTFLAGS="$link_flags ${2:-}" \
    cargo build -j "$jobs" --locked --profile dist --target "$host" -p scriptw-cli --bin scriptw $features \
    > "$out/build-$1.log" 2>&1 || { tail -30 "$out/build-$1.log" >&2; exit 1; }
}

if [[ $mode == plain ]]; then
  build plain ""
  cp "$target_dir/$host/dist/scriptw" "$out/scriptw"
  rm -rf "$target_dir/$host"
  echo "dist binary: $out/scriptw"
  exit 0
fi

sysroot="$(rustc --print sysroot)"
profdata="${LLVM_PROFDATA:-$sysroot/lib/rustlib/$host/bin/llvm-profdata}"
[[ -x $profdata ]] || { echo "error: no llvm-profdata; run 'rustup component add llvm-tools'" >&2; exit 1; }
rustc_llvm="$(rustc -vV | sed -n 's/^LLVM version: \([0-9]*\).*/\1/p')"
profdata_llvm="$("$profdata" --version | sed -n 's/.*LLVM version \([0-9]*\).*/\1/p' | head -1)"
[[ $rustc_llvm == "$profdata_llvm" ]] || { echo "error: llvm-profdata is LLVM $profdata_llvm, rustc is LLVM $rustc_llvm" >&2; exit 1; }

profiles="$out/profraw"
rm -rf "$profiles" "$out"/scriptw-*.profdata
mkdir -p "$profiles"

# 1. Instrumented build.
build gen "-Cprofile-generate=$profiles"
cp "$target_dir/$host/dist/scriptw" "$out/scriptw-instrumented"
rm -rf "$target_dir/$host"

# 2. Training. %m merges every process into one .profraw online.
LLVM_PROFILE_FILE="$profiles/scriptw-%m.profraw" "$repo/scripts/dist-pgo-train.sh" "$out/scriptw-instrumented"
rm -f "$out/scriptw-instrumented"
n=$(find "$profiles" -name '*.profraw' | wc -l)
((n > 0)) || { echo "error: the training wrote no profile" >&2; exit 1; }

# 3. Merge. The name carries the profile checksum: Cargo tracks RUSTFLAGS,
# not the profile's content.
"$profdata" merge -o "$out/merged.profdata" "$profiles"
merged="$out/scriptw-$(cksum < "$out/merged.profdata" | cut -d' ' -f1).profdata"
mv "$out/merged.profdata" "$merged"
rm -rf "$profiles"
echo "merged $n profiles -> $merged"

# 4. Optimized build. rustc only warns when it cannot use the profile.
# --emit-relocs keeps the static relocations (a non-loaded section) so that
# llvm-bolt can reorder functions; keep-text-section-prefix keeps LLVM's
# .text.hot/.text.unlikely groups apart. Neither changes an instruction;
# the relocations add about 19 MB to the file.
build use "-Cprofile-use=$merged -C link-arg=-Wl,--emit-relocs -C link-arg=-Wl,-z,keep-text-section-prefix"
pattern='^(warning|error).*(profile-use|profile format|profdata|profile data|instrumentation profile)'
if grep -qE "$pattern" "$out/build-use.log"; then
  grep -E "$pattern" "$out/build-use.log" | head -3 >&2
  echo "error: rustc did not use the profile" >&2
  exit 1
fi
cp "$target_dir/$host/dist/scriptw" "$out/scriptw"
rm -rf "$target_dir/$host"
echo "dist PGO binary: $out/scriptw"
