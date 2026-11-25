# Third-party components and assets

Quiet.md is distributed under the [MIT license](LICENSE). The following third-party code and assets ship with the application.

## Rust dependencies

This table lists 770 packages from `cargo metadata` and their declared licenses, including MPL-2.0. Consult each component's license text for its terms.

Run `scripts/licences.sh` to regenerate the table from the full dependency list.

GPUI and its Zed dependencies (the `gpui*`, `collections`, `refineable`, `derive_refineable`, `sum_tree`, `scheduler`, `http_client`, `ztracing` and `ztracing_macro` packages at versions 0.1.0 / 0.2.2 in the table) come from [Zed](https://github.com/zed-industries/zed) under Apache-2.0. The pinned snapshot lives in `vendor/zed/crates`, with the full license in `vendor/zed/LICENSE-APACHE`. Local patches in `vendor/zed/patches` mark changes with `quiet.md:` comments. See [Vendored patches](vendor/PATCHES.md).

| Package | Version | License |
|---|---|---|
| accesskit | 0.24.1 | MIT OR Apache-2.0 |
| accesskit_atspi_common | 0.19.1 | MIT OR Apache-2.0 |
| accesskit_consumer | 0.38.0 | MIT OR Apache-2.0 |
| accesskit_macos | 0.26.3 | MIT OR Apache-2.0 |
| accesskit_unix | 0.22.1 | MIT OR Apache-2.0 |
| accesskit_windows | 0.34.0 | MIT OR Apache-2.0 |
| addr2line | 0.27.1 | Apache-2.0 OR MIT |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| aes | 0.8.4 | MIT OR Apache-2.0 |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| aligned | 0.4.3 | MIT OR Apache-2.0 |
| aligned-vec | 0.6.4 | MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| arbitrary | 1.5.0 | MIT OR Apache-2.0 |
| arg_enum_proc_macro | 0.3.4 | MIT |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 |
| as-slice | 0.2.1 | MIT OR Apache-2.0 |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 |
| ashpd | 0.13.13 | MIT |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 |
| async-channel | 2.5.0 | Apache-2.0 OR MIT |
| async-executor | 1.14.0 | Apache-2.0 OR MIT |
| async-fs | 2.2.0 | Apache-2.0 OR MIT |
| async-io | 2.6.0 | Apache-2.0 OR MIT |
| async-lock | 3.4.2 | Apache-2.0 OR MIT |
| async-net | 2.0.0 | Apache-2.0 OR MIT |
| async-process | 2.5.0 | Apache-2.0 OR MIT |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 |
| async-signal | 0.2.14 | Apache-2.0 OR MIT |
| async-task | 4.7.1 | Apache-2.0 OR MIT |
| async-trait | 0.1.92 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| atspi | 0.29.0 | Apache-2.0 OR MIT |
| atspi-common | 0.13.0 | Apache-2.0 OR MIT |
| atspi-proxies | 0.13.0 | Apache-2.0 OR MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| av-scenechange | 0.14.1 | MIT |
| av1-grain | 0.2.5 | BSD-2-Clause |
| avif-serialize | 0.8.9 | BSD-3-Clause |
| backtrace | 0.3.77 | MIT OR Apache-2.0 |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bit-set | 0.10.0 | Apache-2.0 OR MIT |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT |
| bit_field | 0.10.3 | Apache-2.0/MIT |
| bitflags | 1.3.2 | MIT/Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| bitstream-io | 4.10.0 | MIT/Apache-2.0 |
| block | 0.1.6 | MIT |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block-padding | 0.3.3 | MIT OR Apache-2.0 |
| block2 | 0.5.1 | MIT |
| block2 | 0.6.2 | MIT |
| blocking | 1.7.0 | Apache-2.0 OR MIT |
| bon | 3.10.2 | MIT OR Apache-2.0 |
| bon-macros | 3.10.2 | MIT OR Apache-2.0 |
| built | 0.8.1 | MIT |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| calloop | 0.14.5 | MIT |
| calloop-wayland-source | 0.4.1 | MIT |
| cbc | 0.1.2 | MIT OR Apache-2.0 |
| cbindgen | 0.28.0 | MPL-2.0 |
| cc | 1.9.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| cfg_aliases | 0.2.2 | MIT |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| cipher | 0.4.4 | MIT OR Apache-2.0 |
| cocoa | 0.25.0 | MIT OR Apache-2.0 |
| cocoa-foundation | 0.1.2 | MIT OR Apache-2.0 |
| cocoa-foundation | 0.2.0 | MIT OR Apache-2.0 |
| codespan-reporting | 0.12.0 | Apache-2.0 |
| color | 0.3.3 | Apache-2.0 OR MIT |
| color_quant | 1.1.0 | MIT |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| console_error_panic_hook | 0.1.7 | Apache-2.0/MIT |
| const-random | 0.1.18 | MIT OR Apache-2.0 |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 |
| convert_case | 0.10.0 | MIT |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 |
| core-foundation | 0.10.0 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 |
| core-graphics | 0.24.0 | MIT OR Apache-2.0 |
| core-graphics-helmer-fork | 0.24.0 | MIT OR Apache-2.0 |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 |
| core-text | 21.0.0 | MIT OR Apache-2.0 |
| core_detect | 1.0.0 | MIT/Apache-2.0 |
| core_maths | 0.1.1 | MIT |
| cosmic-text | 0.19.0 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| crunchy | 0.2.4 | MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| ctor | 1.0.14 | Apache-2.0 OR MIT |
| darling | 0.24.1 | MIT |
| darling_core | 0.24.1 | MIT |
| darling_macro | 0.24.1 | MIT |
| data-url | 0.3.2 | MIT OR Apache-2.0 |
| deranged | 0.5.9 | MIT OR Apache-2.0 |
| derive_more | 2.1.1 | MIT |
| derive_more-impl | 2.1.1 | MIT |
| derive_refineable | 0.1.0 | Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| dirs | 5.0.1 | MIT OR Apache-2.0 |
| dirs | 7.0.0 | MIT OR Apache-2.0 |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 |
| dispatch | 0.2.0 | MIT |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dlib | 0.5.3 | MIT |
| document-features | 0.2.12 | MIT OR Apache-2.0 |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| dwrote | 0.11.5 | MPL-2.0 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| either | 1.19.0 | MIT OR Apache-2.0 |
| embed-resource | 3.0.12 | MIT |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| endi | 1.1.1 | MIT |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 |
| enumn | 0.1.14 | MIT OR Apache-2.0 |
| equator | 0.4.2 | MIT |
| equator-macro | 0.4.2 | MIT |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| etagere | 0.2.15 | MIT/Apache-2.0 |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT |
| exr | 1.74.2 | BSD-3-Clause |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fax | 0.2.7 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| fearless_simd | 0.7.0 | Apache-2.0 OR MIT |
| filedescriptor | 0.8.3 | MIT |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| fixed_decimal | 0.7.3 | Unicode-3.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| float-cmp | 0.9.0 | MIT |
| float-ord | 0.3.2 | MIT / Apache-2.0 |
| flume | 0.12.0 | Apache-2.0/MIT |
| foldhash | 0.2.0 | Zlib |
| font-types | 0.11.3 | MIT OR Apache-2.0 |
| font-types | 0.12.7 | MIT OR Apache-2.0 |
| fontconfig-parser | 0.5.8 | MIT |
| fontdb | 0.23.0 | MIT |
| foreign-types | 0.5.0 | MIT/Apache-2.0 |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| freetype-sys | 0.20.1 | MIT |
| futf | 0.1.5 | MIT / Apache-2.0 |
| futures | 0.3.34 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| gethostname | 1.1.0 | Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| gif | 0.14.2 | MIT OR Apache-2.0 |
| glifo | 0.4.0 | Apache-2.0 OR MIT |
| glob | 0.3.4 | MIT OR Apache-2.0 |
| globset | 0.4.20 | Unlicense OR MIT |
| gloo-timers | 0.3.0 | MIT OR Apache-2.0 |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib |
| gpu-alloc | 0.6.2 | MIT OR Apache-2.0 |
| gpu-alloc-ash | 0.7.1 | MIT OR Apache-2.0 |
| gpu-alloc-types | 0.3.1 | MIT OR Apache-2.0 |
| gpui | 0.2.2 | Apache-2.0 |
| gpui-macros | 0.2.2 | Apache-2.0 |
| gpui_collections | 0.2.2 | Apache-2.0 |
| gpui_derive_refineable | 0.2.2 | Apache-2.0 |
| gpui_http_client | 0.2.2 | Apache-2.0 |
| gpui_media | 0.2.2 | Apache-2.0 |
| gpui_perf | 0.2.2 | Apache-2.0 |
| gpui_refineable | 0.2.2 | Apache-2.0 |
| gpui_semantic_version | 0.2.2 | Apache-2.0 |
| gpui_sum_tree | 0.2.2 | Apache-2.0 |
| gpui_util | 0.2.2 | Apache-2.0 |
| gpui_util_macros | 0.2.2 | Apache-2.0 |
| grid | 0.18.0 | MIT |
| guillotiere | 0.7.0 | MIT/Apache-2.0 |
| h2 | 0.4.20 | MIT |
| half | 2.7.1 | MIT OR Apache-2.0 |
| harfrust | 0.5.2 | MIT |
| hash32 | 0.3.1 | MIT OR Apache-2.0 |
| hashbrown | 0.14.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hashlink | 0.12.2 | MIT OR Apache-2.0 |
| heapless | 0.9.3 | MIT OR Apache-2.0 |
| heck | 0.4.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| hkdf | 0.12.4 | MIT OR Apache-2.0 |
| hmac | 0.12.1 | MIT OR Apache-2.0 |
| html5ever | 0.27.0 | MIT OR Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| hyper | 1.12.0 | MIT |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT |
| hyper-util | 0.1.21 | MIT |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 |
| icu_locale_fallback | 2.4.0 | Unicode-3.0 |
| icu_locale_fallback_data | 2.4.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_plurals | 2.4.0 | Unicode-3.0 |
| icu_plurals_data | 2.4.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 |
| ident_case | 1.0.1 | MIT/Apache-2.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.25.10 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| imagesize | 0.14.0 | MIT |
| imgref | 1.12.3 | CC0-1.0 OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| inout | 0.1.4 | MIT OR Apache-2.0 |
| interpolate_name | 0.2.4 | MIT |
| inventory | 0.3.25 | MIT OR Apache-2.0 |
| is-docker | 0.2.0 | MIT |
| is-wsl | 0.4.0 | MIT |
| itertools | 0.14.0 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| jpeg-decoder | 0.3.2 | MIT OR Apache-2.0 |
| js-sys | 0.3.106 | MIT OR Apache-2.0 |
| katex-rs | 0.3.0 | MIT |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 |
| kurbo | 0.11.3 | Apache-2.0 OR MIT |
| kurbo | 0.13.1 | Apache-2.0 OR MIT |
| kv-log-macro | 1.0.7 | MIT OR Apache-2.0 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 |
| lebe | 0.5.3 | BSD-3-Clause |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libfuzzer-sys | 0.4.13 | (MIT OR Apache-2.0) AND NCSA |
| libloading | 0.8.9 | ISC |
| libm | 0.2.16 | MIT |
| libredox | 0.1.25 | MIT |
| libsqlite3-sys | 0.40.0 | MIT |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| litrs | 1.0.1 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| loop9 | 0.1.5 | MIT |
| lyon_geom | 1.0.19 | MIT OR Apache-2.0 |
| lyon_path | 1.0.19 | MIT OR Apache-2.0 |
| mac | 0.1.1 | MIT/Apache-2.0 |
| mac-notification-sys | 0.6.15 | MIT/Apache-2.0 |
| mach2 | 0.6.0 | BSD-2-Clause OR MIT OR Apache-2.0 |
| malloc_buf | 0.0.6 | MIT |
| markup5ever | 0.12.1 | MIT OR Apache-2.0 |
| markup5ever_rcdom | 0.3.0 | MIT OR Apache-2.0 |
| maybe-rayon | 0.1.1 | MIT |
| md-5 | 0.10.6 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| memoffset | 0.9.1 | MIT |
| metal | 0.29.0 | MIT OR Apache-2.0 |
| mime | 0.3.17 | MIT OR Apache-2.0 |
| mime_guess | 2.0.5 | MIT |
| minimal-lexical | 0.2.1 | MIT/Apache-2.0 |
| minisign-verify | 0.3.0 | MIT |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT |
| naga | 30.0.2 | MIT OR Apache-2.0 |
| naga-types | 30.0.2 | MIT OR Apache-2.0 |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 |
| new_debug_unreachable | 1.0.6 | MIT |
| no_std_io2 | 0.9.4 | Apache-2.0 OR MIT |
| nom | 8.0.0 | MIT |
| noop_proc_macro | 0.3.0 | MIT |
| notify-rust | 4.19.0 | MIT OR Apache-2.0 |
| ntapi | 0.4.3 | Apache-2.0 OR MIT |
| num | 0.4.3 | MIT OR Apache-2.0 |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-bigint-dig | 0.9.1 | MIT/Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 |
| num-derive | 0.4.2 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-iter | 0.1.46 | MIT OR Apache-2.0 |
| num-rational | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| num_cpus | 1.17.0 | MIT OR Apache-2.0 |
| objc | 0.2.7 | MIT |
| objc-foundation | 0.1.1 | MIT |
| objc-sys | 0.3.5 | MIT |
| objc2 | 0.5.3 | MIT |
| objc2 | 0.6.5 | MIT |
| objc2-app-kit | 0.2.2 | MIT |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-data | 0.2.2 | MIT |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-image | 0.2.2 | MIT |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-encode | 4.1.0 | MIT |
| objc2-foundation | 0.2.2 | MIT |
| objc2-foundation | 0.3.2 | MIT |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-metal | 0.2.2 | MIT |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-quartz-core | 0.2.2 | MIT |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-security | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-uniform-type-identifiers | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-user-notifications | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc_exception | 0.1.2 | MIT |
| objc_id | 0.1.1 | MIT |
| object | 0.40.0 | Apache-2.0 OR MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| oo7 | 0.6.0 | MIT |
| open | 5.4.4 | MIT |
| option-ext | 0.2.0 | MPL-2.0 |
| ordered-channel | 1.2.0 | MIT OR Apache-2.0 |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| pastey | 0.1.1 | MIT OR Apache-2.0 |
| pathfinder_geometry | 0.5.1 | MIT/Apache-2.0 |
| pathfinder_simd | 0.5.6 | MIT OR Apache-2.0 |
| pbkdf2 | 0.12.2 | MIT OR Apache-2.0 |
| peniko | 0.6.1 | Apache-2.0 OR MIT |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| phf | 0.11.3 | MIT |
| phf | 0.13.1 | MIT |
| phf | 0.14.0 | MIT |
| phf_codegen | 0.11.3 | MIT |
| phf_codegen | 0.14.0 | MIT |
| phf_generator | 0.11.3 | MIT |
| phf_generator | 0.13.1 | MIT |
| phf_generator | 0.14.0 | MIT |
| phf_macros | 0.13.1 | MIT |
| phf_macros | 0.14.0 | MIT |
| phf_shared | 0.11.3 | MIT |
| phf_shared | 0.13.1 | MIT |
| phf_shared | 0.14.0 | MIT |
| pico-args | 0.5.0 | MIT |
| pin-project | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| piper | 0.2.5 | MIT OR Apache-2.0 |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| png | 0.17.16 | MIT OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| polling | 3.11.0 | Apache-2.0 OR MIT |
| pollster | 0.2.5 | Apache-2.0/MIT |
| polycool | 0.4.0 | MIT OR Apache-2.0 |
| postage | 0.5.0 | MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| precomputed-hash | 0.1.1 | MIT |
| presser | 0.3.1 | MIT OR Apache-2.0 |
| prettyplease | 0.3.0 | MIT OR Apache-2.0 |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| profiling | 1.0.18 | MIT OR Apache-2.0 |
| profiling-procmacros | 1.0.18 | MIT OR Apache-2.0 |
| pulp | 0.22.3 | MIT |
| pulp-wasm-simd-flag | 0.1.1 | MIT |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| qoi | 0.4.1 | MIT/Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quick-xml | 0.37.5 | MIT |
| quick-xml | 0.41.0 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rand | 0.8.8 | MIT OR Apache-2.0 |
| rand | 0.9.5 | MIT OR Apache-2.0 |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| rand_core | 0.9.5 | MIT OR Apache-2.0 |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 |
| rangemap | 1.8.0 | MIT/Apache-2.0 |
| rapidhash | 4.5.1 | MIT OR Apache-2.0 |
| rav1e | 0.8.1 | BSD-2-Clause |
| ravif | 0.13.0 | BSD-3-Clause |
| raw-cpuid | 11.6.0 | MIT |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 |
| rayon | 1.12.0 | MIT OR Apache-2.0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 |
| read-fonts | 0.37.0 | MIT OR Apache-2.0 |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 |
| reborrow | 0.5.5 | MIT |
| redox_syscall | 0.5.18 | MIT |
| redox_users | 0.4.6 | MIT |
| redox_users | 0.5.3 | MIT |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| refineable | 0.1.0 | Apache-2.0 |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 |
| resvg | 0.46.0 | Apache-2.0 OR MIT |
| rgb | 0.8.53 | MIT |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 |
| roxmltree | 0.21.2 | MIT OR Apache-2.0 |
| rquickjs | 0.14.0 | MIT |
| rquickjs-core | 0.14.0 | MIT |
| rquickjs-sys | 0.14.0 | MIT |
| rsqlite-vfs | 0.2.0 | MIT |
| rusqlite | 0.43.0 | MIT |
| rustc-demangle | 0.1.28 | MIT/Apache-2.0 |
| rustc-hash | 1.1.0 | Apache-2.0/MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| rustybuzz | 0.20.1 | MIT |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| same-file | 1.0.6 | Unlicense/MIT |
| scheduler | 0.1.0 | Apache-2.0 |
| schemars | 1.2.3 | MIT |
| schemars_derive | 1.2.3 | MIT |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| screencapturekit | 0.2.8 | MIT OR Apache-2.0 |
| screencapturekit-sys | 0.2.8 | MIT OR Apache-2.0 |
| seahash | 4.1.0 | MIT |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_bytes | 0.11.19 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 |
| serde_spanned | 1.1.2 | MIT OR Apache-2.0 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |
| sha1_smol | 1.0.1 | BSD-3-Clause |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| simd_helpers | 0.1.0 | MIT |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 |
| simplecss | 0.2.2 | Apache-2.0 OR MIT |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| skrifa | 0.40.0 | MIT OR Apache-2.0 |
| skrifa | 0.44.0 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.16.3 | MIT OR Apache-2.0 |
| smol | 2.0.2 | Apache-2.0 OR MIT |
| smol_str | 0.3.7 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| spin | 0.9.9 | MIT |
| spin | 0.10.1 | MIT |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 |
| sqlite-wasm-rs | 0.6.3 | MIT |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| string_cache | 0.8.9 | MIT OR Apache-2.0 |
| string_cache_codegen | 0.5.4 | MIT OR Apache-2.0 |
| strsim | 0.11.1 | MIT |
| strum | 0.28.0 | MIT |
| strum_macros | 0.28.0 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| sum_tree | 0.1.0 | Apache-2.0 |
| svg_fmt | 0.4.5 | MIT/Apache-2.0 |
| svgtypes | 0.16.2 | Apache-2.0 OR MIT |
| swash | 0.2.10 | Apache-2.0 OR MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 |
| sysinfo | 0.31.4 | MIT |
| taffy | 0.15.0 | MIT |
| tao-core-video-sys | 0.2.0 | MIT |
| tauri-winrt-notification | 0.8.1 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| tendril | 0.4.3 | MIT/Apache-2.0 |
| termcolor | 1.4.1 | Unlicense OR MIT |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 |
| tiff | 0.11.3 | MIT |
| time | 0.3.55 | MIT OR Apache-2.0 |
| time-core | 0.1.9 | MIT OR Apache-2.0 |
| tiny-keccak | 2.0.2 | CC0-1.0 |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tinystr | 0.8.5 | Unicode-3.0 |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |
| tokio | 1.53.3 | MIT |
| toml | 0.8.23 | MIT OR Apache-2.0 |
| toml | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 0.6.11 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.22.27 | MIT OR Apache-2.0 |
| toml_edit | 0.25.17+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_write | 0.1.2 | MIT OR Apache-2.0 |
| toml_writer | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| uds_windows | 1.2.1 | MIT |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-linebreak | 0.1.5 | Apache-2.0 |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| usvg | 0.46.0 | Apache-2.0 OR MIT |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| uuid | 1.28.0 | Apache-2.0 OR MIT |
| v_frame | 0.3.9 | BSD-2-Clause |
| value-bag | 1.14.1 | Apache-2.0 OR MIT |
| value-bag-serde1 | 1.14.1 | Apache-2.0 OR MIT |
| value-bag-sval2 | 1.14.1 | Apache-2.0 OR MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| vello_common | 0.3.0 | Apache-2.0 OR MIT |
| vello_cpu | 0.3.0 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| vswhom | 0.1.0 | MIT |
| vswhom-sys | 0.1.3 | MIT |
| waker-fn | 1.2.0 | Apache-2.0 OR MIT |
| walkdir | 2.5.0 | Unlicense/MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 |
| wayland-backend | 0.3.17 | MIT |
| wayland-client | 0.31.15 | MIT |
| wayland-cursor | 0.31.14 | MIT |
| wayland-protocols | 0.32.13 | MIT |
| wayland-protocols-plasma | 0.3.12 | MIT |
| wayland-protocols-wlr | 0.3.12 | MIT |
| wayland-scanner | 0.31.11 | MIT |
| wayland-sys | 0.31.11 | MIT |
| web-sys | 0.3.106 | MIT OR Apache-2.0 |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| webview2-com | 0.38.2 | MIT |
| webview2-com-macros | 0.8.1 | MIT |
| webview2-com-sys | 0.38.2 | MIT |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| wgpu | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-core | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-core-deps-apple | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-core-deps-emscripten | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-core-deps-wasm | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-core-deps-windows-linux-android | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-hal | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-naga-bridge | 30.0.2 | MIT OR Apache-2.0 |
| wgpu-types | 30.0.2 | MIT OR Apache-2.0 |
| which | 8.0.6 | MIT |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| windows | 0.57.0 | MIT OR Apache-2.0 |
| windows | 0.61.3 | MIT OR Apache-2.0 |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-capture | 1.5.0 | MIT |
| windows-collections | 0.2.0 | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.57.0 | MIT OR Apache-2.0 |
| windows-core | 0.61.2 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.2.1 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.57.0 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.57.0 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.1.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.2.0 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-registry | 0.5.3 | MIT OR Apache-2.0 |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 |
| windows-result | 0.1.2 | MIT OR Apache-2.0 |
| windows-result | 0.3.4 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.4.2 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.48.0 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.48.5 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows-threading | 0.1.0 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| windows-version | 0.1.7 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.48.5 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.48.5 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.48.5 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| winnow | 0.7.15 | MIT |
| winnow | 1.0.4 | MIT |
| winreg | 0.56.0 | MIT |
| wio | 0.2.2 | MIT/Apache-2.0 |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| writeable | 0.6.4 | Unicode-3.0 |
| x11 | 2.21.0 | MIT |
| x11rb | 0.13.2 | MIT OR Apache-2.0 |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 |
| xcb | 1.7.1 | MIT |
| xcursor | 0.3.11 | MIT |
| xim-ctext | 0.3.0 | MIT |
| xim-parser | 0.2.2 | MIT |
| xkbcommon | 0.8.0 | MIT |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib |
| xml-rs | 0.8.29 | MIT |
| xml5ever | 0.18.1 | MIT OR Apache-2.0 |
| xmlwriter | 0.1.0 | MIT |
| y4m | 0.8.0 | MIT |
| yazi | 0.2.1 | Apache-2.0 OR MIT |
| yeslogic-fontconfig-sys | 6.0.1 | MIT |
| yoke | 0.8.4 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zbus | 5.19.0 | MIT |
| zbus | 6.3.0 | MIT |
| zbus-lockstep | 0.5.2 | MIT |
| zbus-lockstep-macros | 0.5.2 | MIT |
| zbus_macros | 5.19.0 | MIT |
| zbus_macros | 6.3.0 | MIT |
| zbus_names | 4.3.5 | MIT |
| zbus_utils | 6.1.0 | MIT |
| zbus_xml | 5.2.1 | MIT |
| zcheapstr | 1.1.0 | MIT |
| zed-font-kit | 0.14.1-zed | MIT OR Apache-2.0 |
| zed-scap | 0.0.8-zed | MIT |
| zed-xim | 0.4.0-zed | MIT |
| zeno | 0.3.3 | Apache-2.0 OR MIT |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zruntime | 0.2.3 | MIT |
| ztracing | 0.1.0 | Apache-2.0 |
| ztracing_macro | 0.1.0 | Apache-2.0 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-inflate | 0.2.54 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |
| zvariant | 5.15.1 | MIT |
| zvariant_derive | 5.15.1 | MIT |
| zvariant_utils | 4.2.0 | MIT |

## Ported source

The whiteboard (`crates/core/src/board/` and `src/board/`) ports code from the projects below to Rust. Each file identifies its upstream source.

| Project | Version | License | Port |
|---|---|---|---|
| [Excalidraw](https://github.com/excalidraw/excalidraw) | `3a317b5` (0.18.0) | MIT | Element model, geometry, editing algorithms, undo, clipboard and file formats, tools and UI logic |
| [rough.js](https://github.com/rough-stuff/rough) | 4.6.4 | MIT (Preet Shihn) | `rough/` generation, sketch strokes and all fill methods, plus Park–Miller RNG in `random.rs` |
| [hachure-fill](https://github.com/pshihn/hachure-fill) | 0.5.2 | MIT (Preet Shihn) | Scanlines in `rough/fillers.rs` |
| [points-on-curve](https://github.com/pshihn/bezier-points) | 0.2.0 / 1.0.1 | MIT (Preet Shihn) | Bézier sampling and simplification in `rough/curve.rs` |
| [points-on-path](https://github.com/pshihn/points-on-path) | 0.2.1 | MIT (Preet Shihn) | Path sampling in `rough/path_data.rs` |
| [path-data-parser](https://github.com/pshihn/path-data-parser) | 0.1.0 | MIT (Preet Shihn) | SVG path parsing and normalization in `rough/path_data.rs` |
| [perfect-freehand](https://github.com/steveruizok/perfect-freehand) | 1.2.0 | MIT (Steve Ruiz) | Stroke outlines in `freehand.rs` |
| [fractional-indexing](https://github.com/rocicorp/fractional-indexing) (via Excalidraw's bundled copy) | — | CC0 | `fractional_index.rs` (also the 数据表's row order, `table/order.rs`) |
| [nanoid](https://github.com/ai/nanoid) | — | MIT (Andrey Sitnik) | ID alphabet only, in `random.rs` |

Excalidraw license notice:

```
MIT License

Copyright (c) 2020 Excalidraw

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

rough.js, hachure-fill, points-on-curve, points-on-path, path-data-parser, perfect-freehand and nanoid use the same MIT terms. Copyright holders are listed in the table.

## Bundled JavaScript

| Asset | License |
|---|---|
| `assets/mermaid/mermaid.min.js.zlib` (Mermaid 11.17.2, `dist/mermaid.min.js` from the npm package, zlib-compressed; macOS and Windows builds only) | MIT (2014-2022 Knut Sveidqvist). See the adjacent `LICENSE`. Unmodified. The bundle includes its npm dependencies (among them d3, dagre-d3-es, cytoscape, KaTeX, marked, roughjs, langium and chevrotain, and DOMPurify) under MIT, ISC, BSD-3-Clause and Apache-2.0 terms, DOMPurify under Apache-2.0 or MPL-2.0; their license comments stay in the file. It runs only in an off-screen system web view to draw diagrams (src/mermaid.rs). |

## Windows runtime

The Windows installer includes an unmodified Microsoft Visual C++ Runtime, `vcruntime140.dll`, copied from the Visual Studio x64 Redistributable directory. Microsoft's redistribution terms govern this DLL separately from Quiet.md's MIT license. The source repository does not contain the binary. See [Visual Studio 2022 Redistribution](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution).

## Fonts

| Asset | License |
|---|---|
| `assets/fonts/katex/` (20 KaTeX fonts) | SIL Open Font License / MIT. See `LICENSE` in the same directory. Nine bold or italic faces have updated OS/2 `fsSelection` and `head.macStyle` bits. The originals were marked Regular, which caused Windows and Linux to select italics as upright faces. The glyphs are unchanged. |
| `assets/fonts/excalidraw/` (board fonts from Excalidraw `3a317b5`, `packages/excalidraw/fonts/`) | SIL Open Font License 1.1, full text and copyright notices in the adjacent `OFL.txt`. Excalifont (2024 Excalidraw, Ján Filípek / DizajnDesign), Virgil (2011 Your Own Font Foundry), Nunito Medium (Nunito Project Authors), Lilita One (2011 Juan Montoreano), Cascadia Code (2020 Microsoft), four Assistant weights (Assistant Project Authors / Source Sans Pro Authors), and Xiaolai SC (2020 LXGW, Seto Nozomi). Excalidraw removed non-CJK characters from Xiaolai and adjusted the positioning of Hangul and some CJK glyphs. |
| `assets/fonts/emoji/NotoColorEmoji.ttf` (Noto Color Emoji 2.057, CBDT bitmap, from googlefonts/noto-emoji `v2026-09-24-unicode18_0`, `2D/fonts/`, embedded on Linux only) | SIL Open Font License 1.1 (2022 Google Inc.). Unmodified. See adjacent `OFL.txt`. |
| `assets/fonts/excalidraw/ComicShanns-Regular.ttf` (Comic Shanns Mono 1.3.0) | MIT (Shannon Miwa et al.). See adjacent `LICENSE-ComicShanns.txt`. |
| 霞鹜文楷 LXGW WenKai 1.530 (Regular, Medium; github.com/lxgw/LxgwWenKai release `v1.530`), downloadable document font, not in the packages | SIL Open Font License 1.1 (2021-2026 LXGW). Unmodified. Served from download.quiet.md with its license as `fonts/lxgw-wenkai-LICENSE.txt`. |
| 朱雀仿宋 Zhuque Fangsong 0.212 (Regular; github.com/TrionesType/zhuque release `v0.212`, a technical-preview release), downloadable document font, not in the packages | SIL Open Font License 1.1 (2023 Zhejiang JadeFoci Technology Co., Ltd., TrionesType). Unmodified. Served from download.quiet.md with its license as `fonts/zhuque-fangsong-LICENSE.txt`. |

Upstream distributes the Excalidraw board fonts as WOFF2 subsets by Unicode range. The project uses fontTools to convert and merge each family's subsets into one TrueType file. Coverage and outlines were compared glyph by glyph. Outlines, advance widths, kerning and OpenType features are preserved. The name tables for Nunito, Comic Shanns, Cascadia and Xiaolai match the family names in Excalidraw CSS. Xiaolai is stored as `Xiaolai-Regular.ttf.zlib` (14 MB, 21 MB uncompressed). Shipped packages leave it out and download it from download.quiet.md on first CJK use (docs/FONTS.md), with its license as `fonts/xiaolai-LICENSE.txt`; it is inflated then. Upstream's Liberation Sans 1.05 (GPL with font exception) is omitted. System Arial supplies compatible widths.

## Icons

| Asset | License |
|---|---|
| `assets/editor-icons/` (173 editor and document icons) | 24 icons, the block types (block handle, slash menu, 转换为) and the colour pickers' `palette`, are Nucleo UI Outline 18 (see the adjacent `LICENSE-nucleo`), under the same commercial Nucleo license as `assets/icons/` and **likewise excluded from the MIT license**; 14 of them are Nucleo icons not used elsewhere, which brings the project to 75 distinct Nucleo icons of the 100 the license allows. 61 are from Lucide (ISC, see the adjacent `LICENSE-lucide`): 55 unchanged, 6 adapted or from an earlier Lucide version. The other 88 were drawn by the project (MIT): `callout`, `divider`, `board` and `doc-table` to the Nucleo-matching rules in docs/ICONS.md, each opening with a comment saying so, the rest to Lucide's 24 px grid and 2 px stroke. Each icon’s source is listed in [docs/ICONS.md](docs/ICONS.md). |
| `assets/icons/` (69 interface icons, including `doc-md.svg`, which adds MD lettering to a Nucleo icon, and the insert menu’s `whiteboard.svg` from Nucleo UI Essential Outline 18 under the same license) | Nucleo UI Outline / Fill 18; 69 files drawing on 61 distinct Nucleo icons, each listed with its Nucleo name in [docs/ICONS.md](docs/ICONS.md). The project holds a commercial license permitting up to 100 icons in an open-source project. **These icons are excluded from the MIT license.** If you fork the project, verify your own license coverage or replace the icons. You may not extract them to create an icon library or pack. See the [Nucleo license](https://nucleoapp.com/license). |
| `assets/icons/folder.svg`, `assets/icons/folder-open.svg`, `assets/icons/disclosure.svg`, `assets/icons/import.svg`, `assets/icons/globe.svg`, `assets/icons/doc-table.svg`, `assets/icons/board.svg`, `assets/icons/doc-blocks.svg`, and the ten data-table field-type icons `assets/icons/ft-*.svg` (`ft-attach`, `ft-check`, `ft-date`, `ft-link`, `ft-lock`, `ft-multi`, `ft-number`, `ft-person`, `ft-select`, `ft-text`) | Drawn by the project and distributed under MIT; each SVG opens with a comment saying so. These 18 are counted separately from the 69 Nucleo icons. |
| `assets/icons/excalidraw/` (96 board toolbar, property panel, menu and footer icons, including six active-tool `*-filled.svg` variants) | From Excalidraw `packages/excalidraw/components/icons.tsx` at 3a317b5 (MIT, © Excalidraw), mostly adapted from tabler-icons (MIT, © Paweł Kuna). SVG headers identify upstream icons. Rendering preserves the upstream SVG except for replacing `var(--icon-fill-color)` with `currentColor`, converting root CSS rotation to an SVG transform, and masking the white group/ungroup handles for monochrome drawing. These are separate from the Nucleo set. |
| `assets/branding/app-icon.png`, `assets/branding/app-icon-dev.png` | Owned by the project. The preview icon has a dev ribbon generated by `tools/branding/dev_icon.py`. |

## Covers and illustrations

The 26 covers and their thumbnails in `assets/covers/` and the project-drawn illustrations in `assets/illus/` (three empty-state illustrations, and under `kinds/` the five colored kind icons of the New menu) are distributed under MIT.

## Built-in theme values

`assets/themes/` contains four complete CSS themes (Craft, Feishu, Notion and Obsidian), each specifying the current value of every property. Individual color, font size and spacing values are not subject to copyright. The theme files are distributed under MIT. The names identify the styles and imply no affiliation with the respective companies.

The design system in `src/design/` takes its colour, radius and spacing values from [shadcn/ui](https://github.com/shadcn-ui/ui) (MIT, Copyright (c) 2023 shadcn): the Neutral theme and the Nova and Vega styles. No shadcn code is included; the components are written for GPUI. The edge of its popovers and cards takes its values from [affectUI](https://github.com/flohoeller/affectui)'s clean style (MIT, Copyright (c) 2026 Florian Höller (adfects)), read at commit `e627e5b`; no affectUI code is included either.

## Bundled documents

The project wrote the Getting Started sample documents in `shared-mock/library/` and distributes them under MIT.
