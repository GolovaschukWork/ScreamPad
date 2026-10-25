import { useTranslation } from "react-i18next";
import type { AudioFailure } from "../../shared/ipc/bindings";
import type { AudioState } from "./useAudio";
import { errorText } from "../../shared/ipc/errors";

function failureText(t: ReturnType<typeof useTranslation>["t"], failure: AudioFailure | null | undefined): string {
  switch (failure) {
    case "noDevice":
      return t("audio.failed.noDevice");
    case "permissionDenied":
      return t("audio.failed.permissionDenied");
    case "unsupportedFormat":
      return t("audio.failed.unsupportedFormat");
    default:
      return t("audio.failed.other");
  }
}

/** Says what is wrong with the audio output, if anything. Renders nothing while the output works. */
export function AudioBanner({ state, onRetry }: { state: AudioState; onRetry: () => void }) {
  const { t } = useTranslation();

  if (state.status === "loading") return null;
  if (state.status === "error") {
    return <p className="notice notice-error" role="alert">{state.error ? errorText(t, state.error) : t("errors.backendUnreachable")}</p>;
  }
  switch (state.audio.state) {
    case "running":
      return null;
    case "starting":
      return <p className="notice" role="status">{t("audio.starting")}</p>;
    case "recovering":
      return <p className="notice" role="status">{t("audio.recovering")}</p>;
    case "stopped":
      return <p className="notice" role="status">{t("audio.stopped")}</p>;
    case "failed":
      return (
        <p className="notice notice-error" role="alert">
          {failureText(t, state.audio.failure)}{" "}
          <button type="button" className="btn" onClick={onRetry}>
            {t("audio.retry")}
          </button>
        </p>
      );
  }
}
