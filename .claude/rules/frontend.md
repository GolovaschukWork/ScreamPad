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
- Styling follows ADR 0011: Tailwind semantic utilities backed by the tokens in `src/app/index.css`; no raw colors elsewhere (`pnpm test:scripts` enforces it). shadcn primitives live in `src/shared/ui` and are added one at a time; do not add the `shadcn` package or a second component library.
- Component behaviour is tested with Vitest and Testing Library (`pnpm test:ui`, ADR 0012); replace only the IPC boundary (`commands`) in tests, never add fake data or test doubles to production components. Navigation is one configuration in `src/app/navigation.ts` (no router); do not list sections whose feature does not exist.
