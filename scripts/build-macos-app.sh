#!/bin/bash
set -euo pipefail

task_root="$(cd "$(dirname "$0")/.." && pwd)"
package_dir="$task_root/apps/macos"
output_dir="$task_root/dist/macos.noindex"
mkdir -p "$output_dir"

swift build --package-path "$package_dir" --scratch-path "$package_dir/.build" \
    --disable-sandbox --configuration release --product AIMemorandum
binary_dir="$(swift build --package-path "$package_dir" --scratch-path "$package_dir/.build" \
    --disable-sandbox --configuration release --show-bin-path)"

staging_dir="$(mktemp -d "$output_dir/.bundle.XXXXXX")"
trap 'rm -rf "$staging_dir"' EXIT
staging_app="$staging_dir/AIMemorandum.app"
mkdir -p "$staging_app/Contents/MacOS" "$staging_app/Contents/Resources"
cp "$package_dir/Bundle/Info.plist" "$staging_app/Contents/Info.plist"
cp "$binary_dir/AIMemorandum" "$staging_app/Contents/MacOS/AIMemorandum"
chmod 755 "$staging_app/Contents/MacOS/AIMemorandum"
printf 'APPL????' > "$staging_app/Contents/PkgInfo"

swift "$task_root/scripts/generate-app-icon.swift" "$staging_dir/AIMemorandum.iconset"
iconutil --convert icns --output "$staging_app/Contents/Resources/AIMemorandum.icns" \
    "$staging_dir/AIMemorandum.iconset"

# 本机默认 ad-hoc 签名；拥有 Developer ID 后可显式提供该身份，bundle ID 不变。
codesign --force --sign "${MEMO_SIGNING_IDENTITY:--}" --identifier com.aimemorandum.app \
    "$staging_app"
codesign --verify --deep --strict --verbose=2 "$staging_app"

final_app="$output_dir/AIMemorandum.app"
if [ -e "$final_app" ]; then
    mv "$final_app" "$staging_dir/previous-build.app"
fi
mv "$staging_app" "$final_app"
python3 "$task_root/scripts/verify-macos-app.py" "$final_app"
printf '\n应用已生成：%s\n' "$final_app"
