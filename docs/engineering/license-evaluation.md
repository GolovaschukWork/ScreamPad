# License and distribution review (not legal advice)

**Project license: GPL-3.0-only.** The owner selected GPLv3 on 2026-10-26. The repository's existing [LICENSE](../../LICENSE) contains the GPL version 3 text; project metadata uses the [SPDX identifier `GPL-3.0-only`](https://spdx.org/licenses/GPL-3.0-only.html). This choice does not grant an automatic "or later" option.

GPL selection does not complete the dependency or distribution audit. Before a public build, inventory direct and transitive Rust, npm and bundled binary licenses; record required notices, source availability and how a user obtains corresponding source for the shipped artifacts. Review any licenses with additional conditions or uncertain compatibility rather than assuming that a package installs cleanly because its code builds.

FFmpeg by default has LGPL-licensed components, but certain build options/components make the FFmpeg build GPL-covered; `--enable-nonfree` has additional redistribution ramifications. Executable packaging, dynamic linkage, build flags, sources/notices, third-party dependencies, and jurisdiction must be reviewed. See https://ffmpeg.org/legal.html.

[BlackHole's publisher](https://github.com/ExistentialAudio/BlackHole#can-i-integrate-blackhole-into-my-app) says GPL-3.0 applications may integrate BlackHole; other applications must request a separate license. That guidance makes a GPL route worth evaluating, but ScreamPad has not selected, bundled, signed or distributed a BlackHole package. Keep installer consent, notices, source obligations, update/uninstall behavior and any publisher-specific conditions in the platform review.

**Blocking gate:** release cannot proceed until the dependency SBOM, FFmpeg configuration and distribution compliance checklist are approved. Product-license selection itself is complete.
