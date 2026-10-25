import type { ComponentProps } from "react";
import { useTranslation } from "react-i18next";
import { formatDuration } from "../../shared/format/duration";
import type { AppError, SoundSummary } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { AudioBanner } from "./AudioBanner";
import { SoundOptions } from "./SoundOptions";
import { useAudio } from "./useAudio";
import { useLibrary } from "./useLibrary";

function failureText(t: ReturnType<typeof useTranslation>["t"], error: AppError | null): string {
  return error ? errorText(t, error) : t("errors.backendUnreachable");
}

type SetOptions = ComponentProps<typeof SoundOptions>["onChange"];

function SoundItem({ sound, onPlay, onOptions }: { sound: SoundSummary; onPlay: (id: number) => void; onOptions: SetOptions }) {
  const { t, i18n } = useTranslation();
  const unavailable = sound.status !== "available";
  return (
    <li className="sound">
      <button
        type="button"
        className="btn btn-primary"
        onClick={() => onPlay(sound.id)}
        disabled={unavailable}
        aria-label={t("sound.play", { name: sound.displayName })}
      >
        {t("sound.playShort")}
      </button>
      <div>
        <p className="sound-name">{sound.displayName}</p>
        <p className="sound-meta">
          {sound.format.toUpperCase()} · {formatDuration(sound.durationMs, i18n.language)}
          {sound.status === "missing" && <span className="sound-warning"> · {t("sound.missing")}</span>}
          {sound.status === "damaged" && <span className="sound-warning"> · {t("sound.damaged")}</span>}
        </p>
      </div>
      <SoundOptions sound={sound} onChange={onOptions} />
    </li>
  );
}

export function LibraryView() {
  const { t } = useTranslation();
  const { list, importState, importSound, setOptions } = useLibrary();
  const { audio, problem, play, stopAll, retry } = useAudio();
  const importing = importState.status === "running";

  return (
    <section className="card" aria-label={t("library.title")}>
      <h2>{t("library.title")}</h2>
      <div className="toolbar">
        <button type="button" className="btn btn-primary" onClick={() => void importSound()} disabled={importing}>
          {importing ? t("import.running") : t("import.button")}
        </button>
        {list.status === "ready" && list.sounds.length > 0 && (
          <>
            <span className="spacer" />
            <span className="muted">{t("library.count", { count: list.sounds.length })}</span>
            <button type="button" className="btn btn-danger" onClick={() => void stopAll()}>
              {t("sound.stopAll")}
            </button>
          </>
        )}
      </div>

      <p className="notice" role="status" aria-live="polite">
        {importState.status === "done" &&
          (importState.alreadyInLibrary
            ? t("import.alreadyInLibrary", { name: importState.name })
            : t("import.done", { name: importState.name }))}
      </p>
      <p className="notice notice-error" role="alert">
        {importState.status === "error" && failureText(t, importState.error)}
      </p>

      <AudioBanner state={audio} onRetry={() => void retry()} />
      <p className="notice notice-error" role="alert">
        {problem !== undefined && failureText(t, problem)}
      </p>

      {list.status === "loading" && <p className="notice" role="status">{t("library.opening")}</p>}
      {list.status === "error" && <p className="notice notice-error" role="alert">{failureText(t, list.error)}</p>}
      {list.status === "ready" &&
        (list.sounds.length === 0 ? (
          <p className="muted">{t("library.empty")}</p>
        ) : (
          <ul className="sound-list" aria-label={t("sound.list")}>
            {list.sounds.map((sound) => (
              <SoundItem key={sound.id} sound={sound} onPlay={(id) => void play(id)} onOptions={setOptions} />
            ))}
          </ul>
        ))}
    </section>
  );
}
