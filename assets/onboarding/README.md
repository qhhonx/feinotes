# First-launch animation

`src/onboarding.rs` plays `intro.png` across the window on first launch.

- The first row holds a 16-color RGB palette. Each subsequent group of 225 rows encodes a 400 × 225 frame. Playback runs at 12 frames per second.
- Each pixel stores its palette index multiplied by 17 and XORed with the previous frame's value. Static areas take very little space.
- On each frame change, the app scales the image 4× with nearest-neighbor sampling and sends a single texture to the GPU. The pixels stay crisp as the animation loops forward and backward. After one second, “Quiet” fades in at the center with a glow along its letters. Onboarding then appears over the continuing animation.

The project generated the five-second source video with Seedance 2.5 in LibTV using a starting frame. That frame is the hillside from `site/public/bg.png`, cropped to 16:9 with the dithering removed. Only the branches and grass move gently. The encoder uses ordered dithering with the original website image's 16-color palette to match its pixel art.

```sh
python3 tools/onboarding/intro.py <video.mp4> assets/onboarding/intro.png --palette site/public/bg.png --fps 12 --w 400 --h 225
```
