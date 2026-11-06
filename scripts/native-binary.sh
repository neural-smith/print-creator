#!/usr/bin/env bash
# Shared native executable for Linux packaging. Source this from another script.
# The combined build compiles once and exports RPC_NATIVE_BINARY to both packagers.
# Individual package scripts build on demand when no executable was supplied.
ensure_native_binary() {
  local root="$1"
  if [[ -z "${RPC_NATIVE_BINARY:-}" ]]; then
    command -v cargo >/dev/null || { echo 'Missing cargo (or set RPC_NATIVE_BINARY).' >&2; return 1; }
    [[ -f "$root/Cargo.lock" ]] || (cd "$root" && cargo generate-lockfile)
    (cd "$root" && cargo build --release --locked --features gui) || return
    local build_root="${CARGO_TARGET_DIR:-$root/target}"
    if [[ "$build_root" != /* ]]; then build_root="$root/$build_root"; fi
    RPC_NATIVE_BINARY="$build_root/release/print-creator"
  fi
  [[ -f "$RPC_NATIVE_BINARY" && -x "$RPC_NATIVE_BINARY" ]] || {
    echo "Missing executable: $RPC_NATIVE_BINARY" >&2
    return 1
  }
  RPC_NATIVE_BINARY="$(realpath "$RPC_NATIVE_BINARY")"
  export RPC_NATIVE_BINARY
}
