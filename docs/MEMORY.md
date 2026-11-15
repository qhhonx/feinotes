# 内存实验记录

起因：[#173](https://github.com/s1dashu/quiet-md/issues/173)。Windows 上启动约 33MB，看过几篇示例后 400MB 以上；另有人说「打开主页就会出现」。

## 结论

| 场景（macOS footprint，MB） | M5 Pro 改前 | M5 Pro 改后 | M1 Pro 改前 | M1 Pro 改后 |
|---|---:|---:|---:|---:|
| 照片库启动 | 230 | 184 | 156 | 110 |
| 照片库主页（8 张 4032×3024 照片卡片） | **1160** | **207** | **1082** | **130** |
| 打开照片文档 | 873 | 222 | 797 | 147 |
| 回到纯文字文档 | 825 | 222 | 749 | 147 |
| 35 秒后 | 377 | 202 | 208 | 127 |
| 指南库启动 | 304 | 221 | 214 | 128 |
| 指南库主页 | 324 | 228 | 239 | 138 |
| 看完全部指南 | 328 | 255 | 236 | 179 |
| 指南库 35 秒后 | 298 | 233 | 222 | 157 |
| 百万字文档编辑、滚动后（M5） | 346 | 281 | — | — |

正式构建（不带 devtools）启动后停在文档页，M5 Pro：照片库 106 → 76，指南库 165 → 109。

主要原因：文档图片和主页卡片缩略图一直按原图分辨率解码。一张 4032×3024 照片在内存里是 48.8MB BGRA，GPU 纹理里再存一份，合计约 98MB；主页卡片上它只画约 190pt 宽。之前的 `DocImages` 只负责把不在屏幕上的图片在 30 秒后释放，屏幕上的图片没有大小上限。

性能：同一份百万字文档的输入、退格、滚动帧时间，改前和改后都在噪声范围内（见实验 4）。

Windows（任务管理器「内存」列，即私有工作集；同一照片库，按键脚本驱动，两轮结果一致，取第二轮）：

| 场景（MB） | 私有工作集 改前 | 改后 | 提交大小 改前 | 改后 | GPU 专用 改前 | 改后 |
|---|---:|---:|---:|---:|---:|---:|
| 启动后 6 秒（主页卡片解码中） | 497 | 35 | 1007 | 172 | 138 | 138 |
| 主页（8 张照片卡片） | **406** | **34** | 914 | 172 | **516** | **146** |
| 打开照片文档 | 129 | 46 | 352 | 195 | 233 | 158 |
| 回到纯文字文档 | 82 | 34 | 305 | 183 | 233 | 158 |
| 35 秒后 | 35 | 33 | 163 | 160 | 138 | 138 |

改前的数字正好对上 issue 描述的「打开主页就 400MB 以上」。改后主页和启动时一样。

## 测量方法

- 脚本：[mem_probe.py](../tools/perf/mem_probe.py) 回放 harness 脚本，每到一个 `shot` 用 `footprint` 量一次进程（macOS）。只用临时文档库。
- 场景：
  - `photos`：[gen_photo_library.py](../tools/perf/gen_photo_library.py) 生成 8 篇各带一张 4032×3024 照片（手机照片尺寸）的文档，加一篇纯文字的 Notes。脚本 [photos.txt](../tools/perf/mem/photos.txt)：启动 → 主页（8 张照片卡片）→ 打开 Photo 1 → 打开 Notes → 等 35 秒（缓存过期）。
  - `guides`：内置指南（`shared-mock/guides/build.py`）。脚本 [guides.txt](../tools/perf/mem/guides.txt)：启动 → 主页 → 依次打开欢迎页和「写作与格式」下的 5 篇 → 打开一篇纯文字指南 → 等 35 秒。
- 列（MB）：
  - `total`：footprint 总数。
  - `pixels`：MALLOC_LARGE 加 untagged VM，即解码后的图片和其他大缓冲。改前还包括已释放、但 malloc 还没还给系统的块；改后大块直接 mmap，所以记在 untagged 里。
  - `small`：MALLOC_SMALL + TINY。
  - `gpu`：IOAccelerator + owned unmapped (graphics)，即纹理和缓冲。
  - `surface`：窗口的 IOSurface（交换链）。
  - `cg`：CG image。
- 机器：
  - M5 Pro：48GB，macOS 26.5。
  - M1 Pro：16GB，macOS 27.2（分类名改成了「Malloc Large」这种写法，脚本已兼容）。
  - 两台都是窗口 1280×800 pt @2x，devtools 构建（离屏渲染）。同一二进制重复跑，各项波动在 ±2MB 以内；`surface` 会在 32/48 之间跳（交换链是 2 张还是 3 张）。devtools 的离屏渲染本身在 M5 上多占约 70MB GPU，所以 devtools 下的启动数字比正式构建高。

```sh
cargo build --release --features devtools
python3 tools/perf/gen_photo_library.py /tmp/q-photos
python3 tools/perf/mem_probe.py tools/perf/mem/photos.txt /tmp/q-photos /tmp/q-run/photos
```

## 实验

数字都是 M5 Pro、devtools 构建、`total` MB。每一列在前一列的基础上累加。

| 场景 | 基线 | 1a | 1b | 2 | 3 | 4 | 5 |
|---|---:|---:|---:|---:|---:|---:|---:|
| photos 启动 | 212 | 214 | 229 | 229 | 199 | 184 | 184 |
| photos 主页 | 1157 | 309 | 262 | 257 | 226 | 211 | 208 |
| photos 照片文档 | 871 | 352 | 331 | 287 | 255 | 225 | 222 |
| photos 35 秒后 | 376 | 261 | 250 | 256 | 219 | 205 | 202 |
| guides 主页 | 335 | 288 | 276 | 276 | — | 228 | 228 |
| guides 看完指南 | 340 | 333 | 334 | 330 | — | 257 | 257 |

1a 按显示尺寸解码；1b 加 JPEG IDCT 缩放；2 上传后丢掉内存里的像素（这一列也包含 IDCT「15% 以内」的修正，照片文档的 331→287 主要来自那个修正）；3 应用图标；4 大块直接 mmap；5 缩略图再小一档。

### 1. 按显示尺寸解码（采用）

`editor::image::picture(path, 长边)` 取代 `img(path).doc_picture()`：

- 缓存键里带上目标像素：长边 × 窗口缩放，1024 以下按 128px、以上按 512px 向上取整，拖动改变图片大小时不会每帧重新解码。
- 后台解码后用 `thumbnail_exact`（面积平均）缩小，再用 `RenderImage::with_scale_factor` 让布局拿到的自然尺寸保持不变。
- 各处的目标尺寸：
  - 正文图片：显示框；
  - 封面：封面框；
  - 主页卡片：卡片里的绘制尺寸；
  - 文件预览：预览框；
  - 灯箱：当前缩放，放到原始尺寸时就是原图。
- 新尺寸解码期间，先用同一张图已有的其他尺寸顶替，不会闪白。同一张图在屏幕上已有别的尺寸时，旧尺寸在下一次 sweep 就释放。
- 动图 GIF 仍走 GPUI 的加载器，保留全部帧和原尺寸。

细节：

- 1a 第一次测时主页仍是 611MB。原因是缩略图没有显式宽度时，`thumb_blocks` 填的是原图宽度（4032），要先夹到正文栏宽 690。
- 1a 之后 `pixels` 仍多约 70MB：是两块已释放的全尺寸 RGB 解码缓冲（各 35MB），留在 malloc 的 large cache 里，`malloc_zone_pressure_relief` 当时没有回收。
- 1b 用 `jpeg-decoder` 的 IDCT 缩放，解码时直接输出 1/2、1/4、1/8，全尺寸缓冲根本不出现，照片的解码也快得多。IDCT 只能整半缩，允许比目标小 15% 以内的档位：2048 的目标用 2016，而不是退回全尺寸。一开始没有这 15%，2048 的目标就退回了全尺寸，用 MallocStackLogging 查到 36MB 的块来自 `jpeg_decoder::compute_image` 才发现。PNG、GIF、CMYK 和 16 位 JPEG 仍走 `image`。新增依赖 `jpeg-decoder 0.3`（MIT/Apache-2.0，关掉 rayon，无传递依赖）。
- 画质：
  - 主页卡片里带文字的截图反而比原来清楚。原来是 GPU 对 4.5 倍大的纹理做双线性缩小，有锯齿。
  - 正文里的指南截图（1600px，显示约 1380px）和原来几乎逐像素一致，平均差 0.02/255。

### 2. 上传 GPU 后丢掉内存里的像素（采用）

GPUI 只在第一次把图片放进 atlas 时读像素，之后每帧只用尺寸。vendor 补丁（PATCHES.md #24）：

- 给 `RenderImage` 加 `hollow()`：同一个 ImageId、同样的尺寸，不带像素。
- 所有窗口都画过这张图之后，缓存在下一次 sweep 把它换成空壳。
- 如果某个窗口的 atlas 里没有它（新窗口第一次画，或 Windows 设备丢失后 atlas 重建），`paint_image` 跳过并记进 `Window::take_missing_images`，下一帧缓存重新解码。
- 动图，以及显示在不汇报帧的窗口里的图片，不做空壳。

用临时日志确认过空壳确实发生（主页 8 张缩略图、照片文档 2016×1512 都换成了空壳）。在 macOS 上，单独测这一项时 footprint 不变：释放的 12MB 留在了 malloc 的 large cache 里（vmmap 显示 `MALLOC_LARGE (empty)`）。实验 4 解决了这一点。Windows 的堆会把 1MB 以上的块直接还给系统，任务管理器「内存」列算的又正是这份 CPU 侧私有内存，所以这一项主要是给 Windows 的。

### 3. 应用图标（采用）

启动就有一块 32MB 的「CG image」。MallocStackLogging 显示它来自 `set_app_icon` → `-[NSApplication setApplicationIconImage:]`：1024px 的 PNG 被当成 1024pt，AppKit 在 Retina 屏上画成 2048² 的位图（32MB）并常驻。把 NSImage 的尺寸设成 256pt（Dock 的最大放大尺寸）后，只剩 2MB。

### 4. 1MiB 以上的块直接 mmap（采用，macOS / Linux）

[big_alloc.rs](../src/big_alloc.rs) 作为全局分配器：1MiB 以上、对齐不超过 4096 的块用 mmap / munmap，其余交给系统分配器。释放时页面立刻还给系统，跟 Windows 堆的行为一样。macOS 的 malloc 和 glibc 都会把释放的大块留着复用，所以图片、解码缓冲、大段文字用完之后 footprint 不降。加上它以后，`pixels` 基本归零，原来的 `release_freed` 只剩小块的作用。

帧时间，百万字文档，`readme-performance.txt`，P50 / P95 ms：

| 构建 | 连续输入 | 连续退格 | 连续滚动 |
|---|---|---|---|
| 基线 | 5.14 / 6.22 | 5.10 / 5.71 | 3.69 / 7.92 |
| 1–3 | 5.36 / 6.18 | 5.38 / 5.88 | 3.23 / 6.47 |
| 1–4 | 5.30 / 6.04 | 5.26 / 5.88 | 3.22 / 6.49 |

测试时机器上还有别的编译任务在跑，三组之间的差异在噪声范围内。

### 5. 缩略图再小一档（采用）

主页卡片按绘制尺寸的 0.75 倍解码（`home.rs` 的 `THUMB`）。画质略软，卡片上看不出区别。因为实验 2 已经去掉了内存里那份，这一项在 macOS 上只省约 3MB（GPU 纹理）。

### 6. PNG 边解码边缩小（采用）

PNG 原来走 `image` 按原图解码，再 `thumbnail_exact`：一张 3000×1600 的图先有 8MB 文件、14MB RGB，两路并行解码（`DECODERS`）时瞬时多出约 50MB。现在 `decode_png` 用 `png` crate 逐行读文件，按目标尺寸做盒式平均（带 alpha 时按 alpha 加权），只留一行原图和一行累加值，文件也不再整个读进内存。隔行扫描的 PNG 和不需要缩小的仍走 `image`。新增直接依赖 `png 0.18`（`image` 本来就用这个版本）。

- 同时解码的路数从固定 2 改为核数的一半（2–4）：每路只占一行原图加缩小后的结果，内存不再随路数明显增长。等待解码的请求按提出的顺序（即绘制顺序，从上到下）取；轮到时如果所在窗口最近一帧已经没画它（滚过去了），就跳过并丢掉这个槽位，下次画到时再排队。
- 卡片出齐的时间（从点开文件夹页或滚动到最后一张解码完成，日志打点）：打开 182–191ms → 180–192ms；两次大幅滚动 160–170 / 121–149ms → 149–168 / 134–145ms（15 核机器，改后 4 路）。
- 解码本身：合成库 120 张 PNG（1.2–6.6MP，共 990MB），两线程按 512 解码，峰值 RSS 68MB → 16MB，耗时 1.40s → 1.50s。
- 应用里（harness 打开 40 张卡片的文件夹页并滚到底，每 0.25 秒采 `footprint`，`FDL_NOPARK`）：峰值 291/331MB → 289–300MB，稳定后 216–225MB → 216–233MB。改前的高点是零星的单次采样（解码瞬间），改后没有了。
- 剩下的峰值和解码无关：同一个库把图缩成 1/20 后峰值也有 220–260MB、稳定 141–152MB。高出稳定值的部分是一块约 80MB 的「Owned physical footprint (unmapped) (graphics)」，有绘制活动时出现，停下一两秒后变成可回收；路径纹理不是它（MSAA 纹理确认是 Memoryless、0 字节，整个运行只建一次），来源还没查清。大图库比小图库稳定值多的约 70MB 是 120 张缩略图的 atlas 纹理（IOAccelerator 31MB → 87–95MB），它们在离开屏幕后 30 秒内保留。

### 没有采用的

- **交换链从 3 张减到 2 张**：2560×1600 时省 16MB。但 120Hz 下 CPU 编码下一帧时会等 drawable，可能掉帧；harness 的帧时间测的是离屏渲染，测不出这种影响，要在真机屏幕上验证才能决定。
- **小块 malloc**：指南库启动时约 40MB。按调用栈汇总后，活着的 malloc 一共 82MB，大部分是启动时的窗口、字体、菜单这类一次性结构，加上 GPUI 每帧的文字布局缓存，没有哪个应用层的持有者特别突出。malloc_history 里看到的那 40MB「键盘布局」，其实是 ICU 数据文件的 mmap（干净页，不计入 footprint）。

## Windows 测量

- 机器：Win11 build 26200，i5-13600KF，RTX 4070 Ti，32GB，1920×1080。
- 构建：`cargo build --release --locked --features devtools`，改前是合并前的 main，改后是本分支。
- 方法：用计划任务在桌面会话里启动应用（SSH 会话里启动的窗口不显示），`FDL_SCRIPT` 只用按键：等 7 秒 → `ctrl-alt-home` 主页 → `ctrl-j` 跳到 Photo 1 → `ctrl-j` 跳到 Notes → 等 40 秒。在第 6、13、21、28、61 秒采样：
  - 私有工作集：性能计数器 `Process\Working Set - Private`，即任务管理器「内存」列；
  - 提交大小：`PrivateMemorySize64`；
  - GPU：`GPU Process Memory` 的 Dedicated / Shared Usage。
- 新文档库第一次打开时直接显示主页，所以第 6 秒已经在主页上。用截图确认过改后的卡片缩略图和正文照片都正常显示。

推测与实测一致：实验 1、2 在 Windows 上同样生效；实验 3、4 只涉及 macOS / Linux。

## 设备丢失

真正的设备丢失（`pnputil /restart-device` 重启显卡）在 Windows 上测不到我们这一层：改前、改后的构建都在第一次设备丢失时崩溃，`directx_renderer.rs` 里交换链 resize 失败后 `render_target_view` 为空，被 `unwrap`。这是 GPUI 原有的问题，和本次改动无关，另行处理。

我们这一层只关心 atlas 被清空后空壳图片能不能恢复。devtools 命令 `forget-uploads` 把缓存里所有空壳图片从窗口的 atlas 删掉并画一帧，效果和 atlas 重建一样：

- 第一版：主页上清空后 2.5 秒，8 张卡片仍是空白。缺失是在第 N 帧绘制时记下的，要等第 N+1 帧才重新解码；主页没有光标闪烁，也没有别的东西请求新的一帧，要等用户动鼠标。
- 修正：`paint_image` 记下缺失时请求下一帧。清空后约 1 秒，8 张卡片和正文照片都已恢复，macOS 和 Windows 上都一样。
