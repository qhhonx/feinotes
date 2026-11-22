#!/bin/sh
set -eu

# Match the Rust namespace and optimization level to the Swift configuration.
configuration=${CONFIGURATION:-Debug}
case "$configuration" in
    Product|TestFlight) FEI_IOS_PRODUCT_CLOUD=1 ;;
    *) FEI_IOS_PRODUCT_CLOUD=0 ;;
esac
case "$configuration" in
    TestFlight) FEI_TESTFLIGHT_BETA=1 ;;
    *) FEI_TESTFLIGHT_BETA=0 ;;
esac
export FEI_IOS_PRODUCT_CLOUD FEI_TESTFLIGHT_BETA
case "$configuration" in
    UpstreamPreview) FEI_IOS_UPSTREAM_PREVIEW=1 ;;
    *) FEI_IOS_UPSTREAM_PREVIEW=0 ;;
esac
export FEI_IOS_UPSTREAM_PREVIEW
export CARGO_PROFILE_DEV_DEBUG=0

rust_features=cloudkit-sync
if [ "${FEI_MOBILE_QA:-0}" = 1 ]; then
    case "$configuration" in
        Product|TestFlight) echo 'QA fixtures are forbidden in production configurations'; exit 1 ;;
    esac
    rust_features=cloudkit-sync,mobile-qa
fi

profile=${RUST_PROFILE:-debug}
case "$profile" in
    debug) set -- ;;
    release) set -- --release ;;
    *) echo 'Unsupported Rust build profile'; exit 1 ;;
esac
if [ "$configuration" = TestFlight ]; then
    [ "$profile" = release ] || { echo 'TestFlight requires the release Rust library'; exit 1; }
fi
# The acceptance checkout can retain this generated dependency between builds.
sh "$SRCROOT/prepare-gpui.sh"
cargo_bin="${CARGO:-$HOME/.cargo/bin/cargo}"
"$cargo_bin" build --locked --manifest-path "$SRCROOT/Cargo.toml" \
    --features "$rust_features" --target "$RUST_TARGET" "$@"
