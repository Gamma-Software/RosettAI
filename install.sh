#!/usr/bin/env bash
set -euo pipefail

repo_url="https://github.com/Gamma-Software/RosettAI"

fail() {
  printf 'rai install: %s\n' "$*" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) target="aarch64-apple-darwin" ;;
  Darwin:x86_64) target="x86_64-apple-darwin" ;;
  Linux:aarch64 | Linux:arm64) target="aarch64-unknown-linux-gnu" ;;
  Linux:x86_64) target="x86_64-unknown-linux-gnu" ;;
  *) fail "no prebuilt rai release for $(uname -s)/$(uname -m)" ;;
esac

release_url=$(curl --fail --location --silent --show-error --retry 3 \
  --output /dev/null --write-out '%{url_effective}' "$repo_url/releases/latest") \
  || fail "cannot find the latest release"
case "$release_url" in
  "$repo_url/releases/tag/"*) tag=${release_url##*/} ;;
  *) fail "unexpected latest-release URL: $release_url" ;;
esac
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "invalid release tag: $tag"

archive="rai-$tag-$target.tar.gz"
download_url="$repo_url/releases/download/$tag"
work_dir=$(mktemp -d) || fail "cannot create a temporary directory"
staged_binary=""
cleanup() {
  if [[ -n "$staged_binary" ]]; then
    rm -f -- "$staged_binary"
  fi
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

curl --fail --location --silent --show-error --retry 3 \
  --output "$work_dir/SHA256SUMS" "$download_url/SHA256SUMS" \
  || fail "cannot download SHA256SUMS"
curl --fail --location --silent --show-error --retry 3 \
  --output "$work_dir/$archive" "$download_url/$archive" \
  || fail "cannot download $archive"

expected=$(awk -v name="$archive" '$2 == name {print $1}' "$work_dir/SHA256SUMS")
[[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] \
  || fail "SHA256SUMS has no unique valid entry for $archive"
if command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$work_dir/$archive" | awk '{print $1}')
elif command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$work_dir/$archive" | awk '{print $1}')
else
  fail "shasum or sha256sum is required"
fi
[[ "$actual" == "$expected" ]] || fail "SHA-256 mismatch for $archive"

tar -xzf "$work_dir/$archive" -C "$work_dir" rai \
  || fail "cannot extract rai from $archive"
[[ -f "$work_dir/rai" && ! -L "$work_dir/rai" && -x "$work_dir/rai" ]] \
  || fail "release archive contains no executable rai binary"

if [[ -n "${RAI_INSTALL_DIR:-}" ]]; then
  install_dir=$RAI_INSTALL_DIR
else
  existing=$(command -v rai || true)
  case "$existing" in
    "$HOME/.cargo/bin/rai" | "$HOME/.local/bin/rai") install_dir=${existing%/*} ;;
    *) install_dir="$HOME/.local/bin" ;;
  esac
fi
mkdir -p -- "$install_dir" || fail "cannot create $install_dir"
install_dir=$(cd -- "$install_dir" && pwd -P) || fail "cannot access $install_dir"
staged_binary=$(mktemp "$install_dir/.rai-install.XXXXXX") \
  || fail "cannot write to $install_dir"
cp -- "$work_dir/rai" "$staged_binary" || fail "cannot stage rai"
chmod 755 "$staged_binary" || fail "cannot make rai executable"
mv -f -- "$staged_binary" "$install_dir/rai" || fail "cannot install rai"
staged_binary=""

printf 'Installed rai %s at %s/rai\n' "$tag" "$install_dir"
hash -r 2>/dev/null || true
if [[ "$(command -v rai || true)" != "$install_dir/rai" ]]; then
  printf 'Add %s to your PATH to use this installation.\n' "$install_dir"
fi
