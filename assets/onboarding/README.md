# 首次启动的开场

`intro.png` 是首次启动时铺满窗口播放的像素动画（`src/onboarding.rs`）：

- 第一行是 16 色调色板（RGB 字节），之后每 225 行是一帧，400×225 个点，12 帧/秒；
- 每帧存的是「调色板序号 × 17」，并且和上一帧做了异或，所以静止的地方几乎不占空间；
- 应用里每换一帧，就把这一帧按最近邻放大 4 倍做成一张贴图交给 GPU，放大后点仍然清晰，每帧只画一张图；来回循环播放，1 秒后中央淡入沿字母发光的「Quiet」，随后引导页直接叠在继续播放的影片上。

素材是本项目用 Seedance 2.5（LibTV，首帧生成视频）生成的 5 秒视频：首帧是官网的草坡图
`site/public/bg.png` 裁成 16:9、去掉抖点后的版本，画面里只有树枝和草轻轻摆动。再用
`tools/onboarding/intro.py` 按 `bg.png` 自己的 16 色有序抖动生成，和官网的像素感一致：

```sh
python3 tools/onboarding/intro.py <视频.mp4> assets/onboarding/intro.png --palette site/public/bg.png --fps 12 --w 400 --h 225
```
