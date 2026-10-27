# How to use ScreamPad with Discord on macOS

1. Open **Settings → Audio Routing** and select your physical microphone. ScreamPad asks macOS for microphone access only after you press **Start Routing**. If access is denied, enable ScreamPad under **System Settings → Privacy & Security → Microphone**.
2. Select a compatible virtual output, such as **ScreamPad Audio 2ch** or an already installed **BlackHole 2ch**. The device must expose both output and input roles. If it is missing, use **Refresh Devices** after installing it.
3. Select your headphones or speakers as **Local monitoring output**, or keep **System output** if it is a physical device. Never choose the virtual output for local monitoring. Local playback does not require a virtual device.
4. Press **Start Routing**. The status must say **Routing active**. This means ScreamPad opened the selected microphone and virtual output; it does not prove Discord has received them.
5. In Discord **Voice & Video**, choose the matching virtual device as **Input Device**. Open **Mic Test**, speak, and press **Play** on an imported sound. The clip should also play through the selected local output; your own microphone is not sent to that output. If Discord suppresses part of the signal, check its input sensitivity, noise suppression and echo cancellation settings during the test.
6. Press **Stop Routing** to close capture and the virtual-output stream. In Discord, return to your normal microphone if you want to keep speaking without ScreamPad. ScreamPad does not change the macOS default microphone or Discord's input setting.

## Optional ScreamPad Audio 2ch package

The current development build does **not** include a signed or notarized driver. On the development Mac, the unsigned `ScreamPadAudio2ch-0.7.1.1-unsigned.pkg` was built from the [pinned BlackHole recipe](../../packaging/macos/screampad-audio-2ch/README.md). If you have built a local package, **Open ScreamPad Audio package in Installer** lets you choose it and opens standard macOS Installer. ScreamPad never installs it silently or requests an administrator password. Verify the package source and identity before accepting Installer's prompts. The unsigned package is not a public release artifact. Existing compatible devices work without this package.

## Current verification boundary

The isolated probe previously produced microphone-only and clip-only audio audible in Discord Mic Test, but voice in combined tests was sometimes audible only during clip segments. The production route needs a new Mic Test and a real remote-recipient call before anyone can claim Discord delivery. Signed distribution, macOS 14 and Intel behavior, and the local-output device-change scenario are **NOT VERIFIED** until recorded tests pass.
