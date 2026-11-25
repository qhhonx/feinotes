#!/bin/zsh
# Build Quiet.md for the iOS simulator, install it on a booted simulator and
# launch it (docs/design/mobile.md). Not a release path: no Xcode project,
# ad-hoc signed, debug unless PROFILE=release.
#
#   QUIET_MD_HOME=<temporary library> packaging/ios/simulator.sh [device]
#
# The device is a simulator name or UDID (default: the booted one). Console
# output goes to $LOG (default: target/ios/console.log).
set -e
cd "$(git rev-parse --show-toplevel)"
dev=${1:-booted}
profile=${PROFILE:-debug}
flag=(); [[ $profile == release ]] && flag=(--release)
# Only the app: the workspace's quiet CLI embeds QuickJS, which has no iOS bindings.
IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build -p quiet_md --target aarch64-apple-ios-sim $flag
app=target/ios/$profile/Quiet.md.app
mkdir -p $app
cp target/aarch64-apple-ios-sim/$profile/quiet_md $app/quiet_md
cp packaging/ios/UpstreamSimulator-Info.plist "$app/Info.plist"
codesign --force --sign - $app
xcrun simctl install $dev $app
log=${LOG:-target/ios/console.log}
# simctl passes SIMCTL_CHILD_* to the app; an empty QUIET_MD_HOME would name no library.
[[ -n $QUIET_MD_HOME ]] && export SIMCTL_CHILD_QUIET_MD_HOME=$QUIET_MD_HOME
SIMCTL_CHILD_RUST_BACKTRACE=1 xcrun simctl launch --console-pty --terminate-running-process $dev com.s1dashu.quietmd > $log 2>&1 &
echo "launched; console in $log"
