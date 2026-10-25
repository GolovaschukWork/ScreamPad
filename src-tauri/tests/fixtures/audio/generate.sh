#!/usr/bin/env bash
# Regenerates the tiny audio fixtures used by the probe and import tests. Requires ffmpeg and python3.
# The files are committed; this script only documents how they were made. A 440 Hz sine tone, 0.25 s, 8 kHz, mono
# (the Ogg file is stereo because ffmpeg's built-in Vorbis encoder supports two channels only).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

SINE="sine=frequency=440:duration=0.25:sample_rate=8000"
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SINE" -ac 1 -c:a pcm_s16le tone.wav
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SINE" -ac 1 -c:a flac tone.flac
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SINE" -ac 1 -c:a libmp3lame -b:a 32k tone.mp3
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SINE" -ac 2 -c:a vorbis -strict -2 tone.ogg
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$SINE" -ac 1 -c:a aac -b:a 24k tone.m4a

# A container with a video track and an audio track: not a plain audio file.
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=black:s=16x16:d=0.25:r=4" -f lavfi -i "$SINE" \
  -c:v mpeg4 -c:a aac -b:a 24k -shortest video_with_audio.mp4

# Damaged inputs.
head -c 700 tone.flac > truncated.flac
head -c 40 tone.wav > truncated_header.wav
python3 - <<'PY'
import random
random.seed(7)
open("garbage.bin", "wb").write(bytes(random.randrange(256) for _ in range(4096)))
open("not_audio.wav", "w").write("<html><body>this is not a wave file</body></html>\n" * 20)
PY
