#!/usr/bin/env bash
# Build the FLOOR quoter wasm from the public crates:
#   rust/crates/floor-quote-wasm  →  rust/target/wasm-dist/{floor_quote_bg.wasm, floor_quote.js, floor_quote.d.ts, floor_quote_bg.wasm.d.ts}
#
#   bash scripts/build-wasm.sh            # build into rust/target/wasm-dist
#   bash scripts/build-wasm.sh --check    # build, then require the wasm's sha256 to equal the committed one
#                                         # (packages/sdk-solana/wasm/SHA256SUMS)
#
# Then `npm run sync-wasm -w @floor/sdk-solana` copies the output into the package (the package's
# `build-wasm` script runs both).
#
# Needs: the Rust toolchain named in rust/rust-toolchain.toml with the wasm32-unknown-unknown target,
# and wasm-bindgen-cli at the version the crate pins (0.2.128):
#   rustup target add wasm32-unknown-unknown --toolchain 1.98.1
#   cargo install wasm-bindgen-cli --version 0.2.128 --locked
# The Carpenter crates are fetched from carpenter-sdk at the commit `rust/Cargo.toml` names (a sibling checkout is
# used instead while that file names one by path).
#
# Reproducibility: the crate is built from a COPY whose `///` and `//!` lines are removed (wasm-bindgen
# copies an export's doc comment into the JS glue), with every local path remapped
# (`--remap-path-prefix`) to a fixed name, so the bytes do not depend on where the checkout lives.
#
# Exit codes: 2 missing tool or input, 3 wasm-bindgen version mismatch, 4 size budget, 5 a host path
# left in the output, 6 --check mismatch.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
crate="$root/rust/crates/floor-quote-wasm"
out="${FLOOR_WASM_OUT:-$root/rust/target/wasm-dist}"
max="${FLOOR_WASM_MAX_BYTES:-400000}"
check=0
[[ "${1:-}" == "--check" ]] && check=1

toolchain="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$root/rust/rust-toolchain.toml")"
[[ -n "$toolchain" ]] || { echo "build-wasm: no channel in rust/rust-toolchain.toml" >&2; exit 2; }
command -v wasm-bindgen >/dev/null || { echo "build-wasm: wasm-bindgen-cli is not installed (cargo install wasm-bindgen-cli --version 0.2.128 --locked)" >&2; exit 2; }

pin="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' "$crate/Cargo.toml")"
cli="$(wasm-bindgen --version | awk '{print $2}')"
[[ "$cli" == "$pin" ]] || { echo "build-wasm: the crate pins wasm-bindgen $pin, the CLI is $cli" >&2; exit 3; }

# The build copy. Cargo hashes a path dependency by its path RELATIVE to the workspace root when it
# lies inside it, and by its absolute path otherwise, and that hash reaches symbol names and so the
# bytes. So the copy is a workspace that CONTAINS every path dependency: the wasm crate (doc
# comments removed) at its root, the floor crates under deps/floor-sdk/rust and, while the Carpenter
# crates are path dependencies, the sibling checkout's crates under deps/carpenter-sdk.
copy="$root/rust/target/floor-quote-wasm-build"
rm -rf "$copy"
mkdir -p "$copy/src" "$copy/deps/floor-sdk/rust/crates"
cp "$crate/Cargo.lock" "$copy/Cargo.lock"
# `exclude = ["deps"]`: the copied crates keep their own workspaces (and what they inherit from them).
sed -e 's#path = "../floor-quote"#path = "deps/floor-sdk/rust/crates/floor-quote"#' -e 's#^\[workspace\]$#[workspace]\nexclude = ["deps"]#' \
  "$crate/Cargo.toml" > "$copy/Cargo.toml"
for f in "$crate"/src/*.rs; do
  grep -v -E '^[[:space:]]*//[/!]' "$f" > "$copy/src/$(basename "$f")"
done
cp "$root/rust/Cargo.toml" "$root/rust/Cargo.lock" "$copy/deps/floor-sdk/rust/"
cp -R "$root/rust/fixtures" "$copy/deps/floor-sdk/rust/"
for c in floor-types floor-math floor-quote; do
  mkdir -p "$copy/deps/floor-sdk/rust/crates/$c"
  (cd "$root/rust/crates/$c" && tar cf - --exclude=target .) | (cd "$copy/deps/floor-sdk/rust/crates/$c" && tar xf -)
done
carpenter="$root/../carpenter-sdk"
if grep -q 'path = "../../carpenter-sdk' "$root/rust/Cargo.toml"; then
  [[ -d "$carpenter/crates" ]] || { echo "build-wasm: rust/Cargo.toml names a carpenter-sdk checkout beside this repository; none at $carpenter" >&2; exit 2; }
  mkdir -p "$copy/deps/carpenter-sdk/crates"
  cp "$carpenter/Cargo.toml" "$copy/deps/carpenter-sdk/"
  (cd "$carpenter/crates" && tar cf - --exclude=target --exclude='*.md' .) | (cd "$copy/deps/carpenter-sdk/crates" && tar xf -)
fi

cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
remap=(
  "--remap-path-prefix=$cargo_home=/cargo"
  "--remap-path-prefix=$rustup_home=/rustup"
  "--remap-path-prefix=$copy=floor-sdk/rust/crates/floor-quote-wasm"
  "--remap-path-prefix=$copy/deps=."
)

(
  cd "$copy"
  RUSTFLAGS="${remap[*]}" CARGO_TARGET_DIR="$copy/target" \
    cargo "+$toolchain" build --release --locked --target wasm32-unknown-unknown
)
artifact="$copy/target/wasm32-unknown-unknown/release/floor_quote_wasm.wasm"
[[ -f "$artifact" ]] || { echo "build-wasm: expected $artifact" >&2; exit 2; }

rm -rf "$out"
mkdir -p "$out"
# --target web: an ES module whose default export takes the bytes (`initQuote(bytes)`).
wasm-bindgen --target web --out-dir "$out" --out-name floor_quote "$artifact"

wasm="$out/floor_quote_bg.wasm"
bytes="$(wc -c < "$wasm" | tr -d ' ')"
sha="$(sha256sum "$wasm" | awk '{print $1}')"
echo "build-wasm: $wasm  $bytes B  sha256 $sha  (rust $toolchain, wasm-bindgen $cli)"
if (( bytes >= max )); then
  echo "build-wasm: $bytes B is over the $max B budget" >&2
  exit 4
fi
if grep -a -E -o '/(tmp|root|home|build|Users)/[[:alnum:]_./-]*' "$out"/floor_quote_bg.wasm "$out"/floor_quote.js | head -5 | grep -q .; then
  echo "build-wasm: a host path is left in the output:" >&2
  grep -a -E -o '/(tmp|root|home|build|Users)/[[:alnum:]_./-]*' "$out"/floor_quote_bg.wasm "$out"/floor_quote.js | head -5 >&2
  exit 5
fi
rm -rf "$copy"

if (( check )); then
  want="$(awk '{print $1}' "$root/packages/sdk-solana/wasm/SHA256SUMS")"
  if [[ "$sha" != "$want" ]]; then
    echo "build-wasm: --check: built $sha, committed $want" >&2
    exit 6
  fi
  for f in floor_quote.js floor_quote.d.ts floor_quote_bg.wasm.d.ts; do
    cmp -s "$out/$f" "$root/packages/sdk-solana/wasm/$f" || { echo "build-wasm: --check: $f differs from the committed copy" >&2; exit 6; }
  done
  echo "build-wasm: --check: the rebuilt wasm and glue equal the committed ones"
fi
