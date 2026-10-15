#!/bin/bash
set -euo pipefail

task_root="$(cd "$(dirname "$0")/.." && pwd)"
source_app="$task_root/dist/macos.noindex/AIMemorandum.app"
user_apps="$HOME/Applications"
installed_app="$user_apps/AIMemorandum.app"
python3 "$task_root/scripts/verify-macos-app.py" "$source_app"
mkdir -p "$user_apps"

if [ -e "$installed_app" ]; then
    existing_id="$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$installed_app/Contents/Info.plist")"
    if [ "$existing_id" != com.aimemorandum.app ]; then
        printf '目标位置已有其他应用，未覆盖：%s\n' "$installed_app" >&2
        exit 1
    fi
fi

staging_dir="$(mktemp -d "$user_apps/.aimemorandum-install.XXXXXX")"
trap 'rm -rf "$staging_dir"' EXIT
ditto "$source_app" "$staging_dir/AIMemorandum.app"
python3 "$task_root/scripts/verify-macos-app.py" "$staging_dir/AIMemorandum.app"
if [ -e "$installed_app" ]; then
    mv "$installed_app" "$staging_dir/previous.app"
fi
mv "$staging_dir/AIMemorandum.app" "$installed_app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$installed_app"
python3 "$task_root/scripts/verify-macos-app.py" "$installed_app"
printf '\n已安装：%s\n' "$installed_app"
printf '直接双击应用，或在 Spotlight 中搜索 AIMemorandum。\n'
