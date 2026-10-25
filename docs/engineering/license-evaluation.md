# License evaluation (not legal advice)

**Project license: TBD.** Do not publish a LICENSE file with a speculative license.

| Candidate | Characteristics | Key consideration |
| --- | --- | --- |
| MIT | Permissive, concise | No express patent grant; permissive downstream reuse |
| Apache-2.0 | Permissive with patent grant | More notice requirements |
| GPL-3.0-only | Copyleft for distributed derivative works | Combined-work/distribution compatibility matters |

FFmpeg by default has LGPL-licensed components, but certain build options/components make the FFmpeg build GPL-covered; `--enable-nonfree` has additional redistribution ramifications. Executable packaging, dynamic linkage, build flags, sources/notices, third-party dependencies, and jurisdiction must be reviewed. See https://ffmpeg.org/legal.html.

**Blocking gate:** release cannot proceed until license selection, dependency SBOM and compliance checklist are approved.
