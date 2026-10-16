# ALASHI founder demo

A 120-second English edit of Amir's supplied interview, with the approved RTS map and a circular camera inset. The opening preserves his complete introduction as a Tomorrow School student. The source recording and full interview stay local; only the selected words and cut times are included here. Generated video outputs stay local and are excluded from Git.

Audience: Magzhan Madiyev. The edit explains the owner's job in plain terms: give agents work and a budget, inspect the result, then learn from a shared strategy. Sharing improvements is a proposed product rule. The network benefit is a goal, not a measured adoption result.

The map uses scripted assignments and no live model calls. Six workers can run at once; each verifier waits for its three reports. No game state or model performance is inferred from the founder video. The 10,000-token allowance is an example. The separate 2,634-token test is observed usage across five model calls and failed its quality check. No savings percentage is supported. See [the measurement report](../owner-film/TOKEN_MEASUREMENT.md).

Files:

- `ALASHI-demo-english.mp4`: local build output, excluded from Git.
- `index.html`: player with chapter buttons and a phone-size preview.
- `edit.json`: source cut times and explanatory overlays.
- `source-alignment.json`: timing for the selected source words.
- `captions.ass`: English captions burned into the video.
- `render_map.py`: deterministic map renderer using the existing terrain and atlas.
- `build_video.py`: cuts, caption generation, circular inset and audio normalization.
- `check_demo.py`: task dependency and edit checks.
- [Direction and animation review criteria](DIRECTION.md).

## Rebuild

Requires Python with Pillow and FFmpeg with libx264 and libass. The independent media checker uses PyAV. Set `ALASHI_FFMPEG` if FFmpeg is not on the expected path. The renderer uses Arial on macOS.

```sh
python3 render_map.py --out /private/tmp/alashi-presenter/map.mp4
python3 build_video.py --source /path/to/DJI_20261016172156_0089_D.MP4
python3 check_demo.py
python3 verify_media.py
```

The source file is user-supplied footage, not a dependency downloaded by the script. The first Russian take was used as context but its speech is not in the English film. No synthetic voice or new model requests are used for the final presentation.
