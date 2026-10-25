# Security and supply chain
- Treat local media metadata, filenames, playlists, URLs, and subprocess output as untrusted input.
- Require approval to add/change dependencies, app capabilities, Tauri permissions, secrets, CI credentials, security policy, or updater endpoints.
- Validate URL scheme and destination, redirects, size/time limits, and private-network addresses for media imports; do not implement unrestricted fetching.
- Never construct shell commands by concatenating user input. Avoid shell invocation when structured process arguments exist.
- Never print signing secrets, API tokens, private keys, sensitive paths, or user audio content to logs.
- Keep runtime permissions minimal; make mic capture explicit and user-controllable.
- GitHub PR workflows must never grant untrusted code access to signing credentials.
