import { useTranslation } from "react-i18next";
import { formatDuration } from "../../shared/format/duration";
import type { AppError, SoundSummary } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { useLibrary } from "./useLibrary";

function failureText(t: ReturnType<typeof useTranslation>["t"], error: AppError | null): string {
  return error ? errorText(t, error) : t("errors.backendUnreachable");
}

function SoundItem({ sound }: { sound: SoundSummary }) {
  const { t, i18n } = useTranslation();
  return (
    <li>
      <strong>{sound.displayName}</strong> · {sound.format.toUpperCase()} · {formatDuration(sound.durationMs, i18n.language)}
      {sound.status === "missing" && <em> · {t("sound.missing")}</em>}
      {sound.status === "damaged" && <em> · {t("sound.damaged")}</em>}
    </li>
  );
}

export function LibraryView() {
  const { t } = useTranslation();
  const { list, importState, importSound } = useLibrary();
  const importing = importState.status === "running";

  return (
    <section>
      <p>
        <button type="button" onClick={() => void importSound()} disabled={importing}>
          {importing ? t("import.running") : t("import.button")}
        </button>
      </p>

      <p role="status" aria-live="polite">
        {importState.status === "done" &&
          (importState.alreadyInLibrary
            ? t("import.alreadyInLibrary", { name: importState.name })
            : t("import.done", { name: importState.name }))}
      </p>
      <p role="alert">{importState.status === "error" && failureText(t, importState.error)}</p>

      {list.status === "loading" && <p role="status">{t("library.opening")}</p>}
      {list.status === "error" && <p role="alert">{failureText(t, list.error)}</p>}
      {list.status === "ready" &&
        (list.sounds.length === 0 ? (
          <p>{t("library.empty")}</p>
        ) : (
          <>
            <p>{t("library.count", { count: list.sounds.length })}</p>
            <ul aria-label={t("sound.list")}>
              {list.sounds.map((sound) => (
                <SoundItem key={sound.id} sound={sound} />
              ))}
            </ul>
          </>
        ))}
    </section>
  );
}
