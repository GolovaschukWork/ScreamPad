# Verification expectations
- Every behavior change requires tests at the lowest useful layer; prioritize domain/audio import/routing cases.
- Test errors, cancellation, corrupted media, database rollback, device disappearance, and stream restart where relevant.
- Run available checks; clearly mark missing platform hardware or certificates as NOT VERIFIED.
- A passing Linux runner does not establish Windows/macOS compatibility; CI must build intended targets.
- For UI work cover keyboard flow, state transitions, reduced motion and contrast as relevant.
- Do not change snapshots solely to make failures disappear without reviewing diffs.
