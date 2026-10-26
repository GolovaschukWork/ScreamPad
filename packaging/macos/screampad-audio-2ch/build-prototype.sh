#!/bin/bash
set -euo pipefail

# This recipe creates an unsigned, uninstalled inspection artifact only.
if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <clean BlackHole v0.7.1 checkout> <output directory>" >&2
  exit 2
fi

upstream=$(cd "$1" && pwd -P)
output=$2
revision=e2b22aaaba4e507a097131704bf96dabc004d9cf
version=0.7.1.1
package_name="ScreamPadAudio2ch-${version}-unsigned.pkg"
recipe_dir=$(cd "$(dirname "$0")" && pwd -P)
repo_root=$(cd "$recipe_dir/../../.." && pwd -P)

if [[ $(git -C "$upstream" rev-parse HEAD) != "$revision" ]] ||
   [[ -n $(git -C "$upstream" status --porcelain) ]]; then
  echo "Expected a clean BlackHole checkout at $revision" >&2
  exit 1
fi

mkdir -p "$output"
output=$(cd "$output" && pwd -P)
package="$output/$package_name"
if [[ -e "$package" ]]; then
  echo "Refusing to overwrite $package" >&2
  exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/screampad-audio-build.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/source" "$work/build" "$work/payload"
git -C "$upstream" archive "$revision" | tar -xf - -C "$work/source"
(
  cd "$work/source"
  git apply --check "$recipe_dir/blackhole-v0.7.1.patch"
  git apply "$recipe_dir/blackhole-v0.7.1.patch"
)
cp "$repo_root/src-tauri/icons/icon.icns" "$work/source/BlackHole/ScreamPadAudio.icns"

xcodebuild \
  -project "$work/source/BlackHole.xcodeproj" \
  -target BlackHole \
  -configuration Release \
  CONFIGURATION_BUILD_DIR="$work/build" \
  CODE_SIGNING_ALLOWED=NO \
  ONLY_ACTIVE_ARCH=NO \
  ARCHS='arm64 x86_64' \
  build > "$output/xcodebuild.log" 2>&1

bundle="$work/build/ScreamPadAudio2ch.driver"
if [[ ! -d "$bundle" ]]; then
  echo "Expected driver bundle missing; see $output/xcodebuild.log" >&2
  exit 1
fi
ditto --norsrc "$bundle" "$work/payload/ScreamPadAudio2ch.driver"
cat > "$work/payload/ScreamPadAudio2ch.driver/Contents/Resources/ScreamPad-ATTRIBUTION.txt" <<'NOTICE'
ScreamPad Audio 2ch is a modified build of BlackHole by Existential Audio Inc.
Original source: https://github.com/ExistentialAudio/BlackHole
Upstream revision: e2b22aaaba4e507a097131704bf96dabc004d9cf
Modifications and build recipe: packaging/macos/screampad-audio-2ch in the ScreamPad source repository.
BlackHole is licensed under GPL version 3; the upstream LICENSE is included in this bundle.
This unsigned prototype is not a release artifact.
NOTICE
xattr -cr "$work/payload"

COPYFILE_DISABLE=1 pkgbuild \
  --root "$work/payload" \
  --install-location /Library/Audio/Plug-Ins/HAL \
  --identifier io.github.golovaschukwork.screampad.audio.pkg.2ch \
  --version "$version" \
  "$work/$package_name" > "$output/pkgbuild.log" 2>&1

python3 "$recipe_dir/verify-prototype.py" "$work/$package_name"
mv "$work/$package_name" "$package"
shasum -a 256 "$package"
echo "Unsigned inspection package: $package"
