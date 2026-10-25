---
paths:
  - "src/**/*.ts"
  - "src/**/*.tsx"
  - "src/**/*.css"
---
# Frontend rules
- Use feature-based modules. `shared/` must not import from `features/`.
- Strict TypeScript; avoid `any`, broad type assertions, and duplicated backend DTOs.
- Generated IPC types are contracts; handle loading, empty, error, and disconnected-device states.
- Use semantic controls, keyboard navigation, visible focus, and ARIA where needed.
- Use design tokens; do not scatter raw color/spacing values in feature components.
- Apply responsive desktop layouts for narrow windows and high-DPI displays.
- Prefer testable pure functions and isolated components over feature-wide state coupling.
- No user-facing string literals in components: use translation keys with both English and Ukrainian entries; backend errors map `AppError.kind` to `errors.<kind>` (ADR 0008).
