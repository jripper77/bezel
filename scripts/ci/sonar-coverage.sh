#!/usr/bin/env bash
# Coverage reports for the Sonar job of the shared pipeline: ci.yml passes
# this script as `sonar_comando_testes`.
#
# The Sonar job runs on its own runner, in parallel with the quality jobs, so
# it builds and tests again: the cost the github-workflows README accepts
# instead of making the whole pipeline wait. It writes the two reports that
# `sonar_parametros` point at:
#   resultados/lcov.info     Rust: `cargo llvm-cov` over the whole workspace
#   resultados/lcov-ui.info  the studio UI's JS: `npm run test:unit`, with the
#                            `SF:` paths rewritten from apps/bezel-studio/ to the
#                            repository root, where Sonar resolves them
#
# The WebKitGTK and udev headers the build links come from
# `sonar_pacotes_sistema`, and the Rust toolchain (with llvm-tools) from
# `sonar_versao_linguagem`. The two tools below are downloaded only on CI
# (`CI=true`), pinned and checked by sha256 like every tool of the pipeline;
# elsewhere the script uses the ones already installed, so it also runs on a
# developer machine and never installs anything there.
set -euo pipefail
export LC_ALL=C.UTF-8
cd "$(dirname "$0")/../.."

on_ci=false
if [ "${CI:-}" = "true" ]; then on_ci=true; fi

download() { # url, sha256, destination
  curl --proto '=https' --tlsv1.2 -fsSL -o "$3" "$1"
  printf '%s  %s\n' "$2" "$3" | sha256sum -c -
}

if [ "$on_ci" = true ]; then
  tmp=$(mktemp -d)
  # The version and sha256 the quality job (qualidade.yml) checks.
  download \
    "https://github.com/taiki-e/cargo-llvm-cov/releases/download/v0.9.1/cargo-llvm-cov-x86_64-unknown-linux-gnu.tar.gz" \
    b3f68e625481fed9b16444174f3fa5ebcdbde4a1878803a35eabe2dcefcdc41a \
    "$tmp/cargo-llvm-cov.tar.gz"
  tar xzf "$tmp/cargo-llvm-cov.tar.gz" -C "${CARGO_HOME:-$HOME/.cargo}/bin"
  # The node-ui quality job runs Node 24; the runner's own Node may predate
  # the test runner's glob and coverage flags (22.5+). sha256 from
  # https://nodejs.org/dist/v24.18.0/SHASUMS256.txt
  download \
    "https://nodejs.org/dist/v24.18.0/node-v24.18.0-linux-x64.tar.xz" \
    55aa7153f9d88f28d765fcdad5ae6945b5c0f98a36881703817e4c450fa76742 \
    "$tmp/node.tar.xz"
  tar xJf "$tmp/node.tar.xz" -C "$tmp"
  export PATH="$tmp/node-v24.18.0-linux-x64/bin:$PATH"
fi
cargo llvm-cov --version
node --version

mkdir -p resultados
cargo llvm-cov --all-features --workspace --locked \
  --lcov --output-path resultados/lcov.info

(
  cd apps/bezel-studio
  npm ci --ignore-scripts
  npm run test:unit
)

# The test runner writes paths relative to apps/bezel-studio (`SF:src/...`);
# anything else means the report changed shape, and the rewrite would point
# Sonar at files that do not exist.
ui=apps/bezel-studio/coverage/lcov.info
# One awk, not `grep | grep -q`: under pipefail, an early `grep -q` exit can
# SIGPIPE the first grep and turn this check off without a word.
outside=$(awk '/^SF:/ && !/^SF:src\// { print }' "$ui")
if [ -n "$outside" ]; then
  printf '%s: an SF path outside src/:\n%s\n' "$ui" "$outside" >&2
  exit 1
fi
sed 's#^SF:src/#SF:apps/bezel-studio/src/#' "$ui" > resultados/lcov-ui.info
printf 'Rust: %s files, UI: %s files\n' \
  "$(grep -c '^SF:' resultados/lcov.info)" "$(grep -c '^SF:' resultados/lcov-ui.info)"
