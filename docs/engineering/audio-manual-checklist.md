# Audio manual verification checklist

Automated tests use a fake audio API and one silent run on a real device (`cargo test --test audio_device -- --ignored`). The
behaviors below depend on the operating system and the hardware and **cannot be checked by CI**. Run them on each platform
before the audio features are called reliable, and record the result in the table at the end. Until a row is filled in, the
behavior is **NOT VERIFIED** on that platform.

Setup: start the app (`pnpm tauri dev`), import a clip of a few seconds, and play it. Keep the system volume low.

| # | Action while a sound plays or after it stopped | Expected result |
| --- | --- | --- |
| 1 | Unplug the headphones or USB device that is the default output | No crash. The sound stops. Within a few seconds the app plays on the new default output when a sound is triggered again |
| 2 | Plug the device back in | The app keeps working; the next sound plays (on the default output the system picked) |
| 3 | Change the default output in the system sound settings | The next sound plays on the new device; no restart needed |
| 4 | Change the output's sample rate (macOS: Audio MIDI Setup; Windows: Sound, Properties, Advanced) | No crash; the next sound plays at the right pitch and speed |
| 5 | Connect or disconnect a Bluetooth headset that becomes the default output | Same as 3; note any gap or glitch |
| 6 | Put the computer to sleep and wake it | The next sound plays, or the app reports that audio is unavailable and recovers after a retry |
| 7 | Disable every output device (no output at all) | The app shows that no audio output is available and does not crash; enabling a device recovers it |
| 8 | Trigger the same sound 20 times quickly | No crash and no growing memory; at most 32 sounds overlap, extra triggers are ignored |
| 9 | Start a long sound and close the window | The app exits cleanly, with no hanging process |
| 10 | Play a sound from an AAC (m4a) file | Known limitation: it starts a little late (the encoder delay is not trimmed yet); note how long |

## Results

Fill in one row per platform and date. Use PASS, FAIL (with a note) or NOT RUN.

| Platform | Date | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | Tester |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| macOS 14+ arm64 | | | | | | | | | | | | |
| macOS 14+ x64 | | | | | | | | | | | | |
| Windows 11 x64 | | | | | | | | | | | | |
