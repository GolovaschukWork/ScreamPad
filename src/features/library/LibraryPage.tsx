import { FolderOpenIcon, PlusIcon, SearchXIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PageBody, PageHeader } from "../../app/AppShell";
import type { AppError } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { Button } from "@/shared/ui/button";
import { AudioBanner } from "./AudioBanner";
import { filterSounds, sortSounds } from "./libraryView";
import { useLibraryViewState } from "./libraryViewStore";
import { LibraryToolbar } from "./LibraryToolbar";
import { SoundCollection } from "./SoundCollection";
import { SoundDetails } from "./SoundDetails";
import { useAudio } from "./useAudio";
import { useLibrary } from "./useLibrary";
import { usePlayingSounds } from "./usePlayingSounds";

function Message({
  icon,
  title,
  hint,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  hint?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-lg border border-dashed px-6 py-14 text-center">
      <div className="text-muted-foreground [&_svg]:size-8">{icon}</div>
      <div className="space-y-1">
        <p className="text-sm font-medium">{title}</p>
        {hint && <p className="text-sm text-muted-foreground">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

/** The library: every sound the user imported, as a grid or a list, with search, sorting and playback. */
export function LibraryPage() {
  const { t, i18n } = useTranslation();
  const { list, importing, importSound, reload, setOptions } = useLibrary();
  const { audio, problem, play, stop, stopAll, retry } = useAudio();
  const playing = usePlayingSounds();
  const [view, setView] = useLibraryViewState();
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const failureText = (error: AppError | null) => (error ? errorText(t, error) : t("errors.backendUnreachable"));
  const total = list.status === "ready" ? list.sounds.length : 0;

  const visible = useMemo(
    () =>
      list.status === "ready"
        ? sortSounds(filterSounds(list.sounds, view.query, i18n.language), view.sort, i18n.language)
        : [],
    [list, view.query, view.sort, i18n.language],
  );
  const selected = visible.find((sound) => sound.id === selectedId) ?? null;

  async function onImport() {
    const result = await importSound();
    if (result.kind === "done") {
      toast.success(
        result.alreadyInLibrary
          ? t("import.alreadyInLibrary", { name: result.name })
          : t("import.done", { name: result.name }),
      );
    } else if (result.kind === "error") {
      toast.error(failureText(result.error));
    }
  }

  const importButton = (
    <Button type="button" onClick={() => void onImport()} disabled={importing}>
      <PlusIcon aria-hidden="true" data-icon="inline-start" />
      {importing ? t("import.running") : t("import.button")}
    </Button>
  );

  return (
    <>
      <PageHeader
        title={t("nav.library")}
        subtitle={list.status === "ready" ? t("library.count", { count: total }) : undefined}
        actions={importButton}
      />
      <PageBody>
        <div className="space-y-4">
          <AudioBanner state={audio} onRetry={() => void retry()} />
          {problem !== undefined && (
            <p role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {failureText(problem)}
            </p>
          )}

          {list.status === "loading" && (
            <p role="status" className="py-10 text-center text-sm text-muted-foreground">
              {t("library.opening")}
            </p>
          )}

          {list.status === "error" && (
            <Message icon={<FolderOpenIcon aria-hidden="true" />} title={t("library.loadFailed")}>
              <p role="alert" className="text-sm text-destructive">
                {failureText(list.error)}
              </p>
              <Button type="button" variant="outline" onClick={() => void reload()}>
                {t("library.retry")}
              </Button>
            </Message>
          )}

          {list.status === "ready" && total === 0 && (
            <Message icon={<FolderOpenIcon aria-hidden="true" />} title={t("library.emptyTitle")} hint={t("library.empty")}>
              {importButton}
            </Message>
          )}

          {list.status === "ready" && total > 0 && (
            <>
              <LibraryToolbar
                query={view.query}
                sort={view.sort}
                mode={view.mode}
                onQuery={(query) => setView({ query })}
                onSort={(sort) => setView({ sort })}
                onMode={(mode) => setView({ mode })}
                onStopAll={() => void stopAll()}
              />
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start">
                <div className="min-w-0 flex-1">
                  {visible.length === 0 ? (
                    <Message
                      icon={<SearchXIcon aria-hidden="true" />}
                      title={t("library.noMatches", { query: view.query.trim() })}
                    >
                      <Button type="button" variant="outline" onClick={() => setView({ query: "" })}>
                        {t("library.clearSearch")}
                      </Button>
                    </Message>
                  ) : (
                    <SoundCollection
                      mode={view.mode}
                      sounds={visible}
                      playing={playing}
                      selectedId={selectedId}
                      onSelect={(id) => setSelectedId((current) => (current === id ? null : id))}
                      onPlay={(id) => void play(id)}
                      onStop={(id) => void stop(id)}
                    />
                  )}
                </div>
                {selected && (
                  <div className="lg:sticky lg:top-0 lg:w-80 lg:shrink-0">
                    <SoundDetails sound={selected} onChange={setOptions} onClose={() => setSelectedId(null)} />
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </PageBody>
    </>
  );
}
