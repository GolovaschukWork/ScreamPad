import { useTranslation } from "react-i18next";
import type { AudioFailure } from "../../shared/ipc/bindings";
import type { AudioState } from "./useAudio";
import { errorText } from "../../shared/ipc/errors";
import { Button } from "@/shared/ui/button";

const bannerClass = "rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive";
const noticeClass = "rounded-lg border bg-muted px-4 py-3 text-sm text-muted-foreground";

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
    return <p role="alert" className={bannerClass}>{state.error ? errorText(t, state.error) : t("errors.backendUnreachable")}</p>;
  }
  switch (state.audio.state) {
    case "running":
      return null;
    case "starting":
      return <p role="status" className={noticeClass}>{t("audio.starting")}</p>;
    case "recovering":
      return <p role="status" className={noticeClass}>{t("audio.recovering")}</p>;
    case "stopped":
      return <p role="status" className={noticeClass}>{t("audio.stopped")}</p>;
    case "failed":
      return (
        <p role="alert" className={bannerClass}>
          {failureText(t, state.audio.failure)}{" "}
          <Button type="button" size="sm" variant="outline" className="ml-2" onClick={onRetry}>
            {t("audio.retry")}
          </Button>
        </p>
      );
  }
}
