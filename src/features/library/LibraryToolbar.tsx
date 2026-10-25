import { LayoutGridIcon, ListIcon, SearchIcon, SquareIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ChoiceSelect } from "@/shared/components/ChoiceSelect";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/shared/ui/toggle-group";
import { SORT_KEYS, VIEW_MODES, type SortKey, type ViewMode } from "./libraryView";

type Props = {
  query: string;
  sort: SortKey;
  mode: ViewMode;
  onQuery: (query: string) => void;
  onSort: (sort: SortKey) => void;
  onMode: (mode: ViewMode) => void;
  onStopAll: () => void;
};

/** Search, sorting, the grid/list switch and Stop all. */
export function LibraryToolbar({ query, sort, mode, onQuery, onSort, onMode, onStopAll }: Props) {
  const { t } = useTranslation();
  const sortNames: Record<SortKey, string> = {
    name: t("library.sortName"),
    duration: t("library.sortDuration"),
    recent: t("library.sortRecent"),
  };

  return (
    <div role="toolbar" aria-label={t("library.toolbar")} className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-48 flex-1 sm:max-w-sm">
        <SearchIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          type="search"
          value={query}
          placeholder={t("library.searchPlaceholder")}
          aria-label={t("library.search")}
          className="pl-8"
          onChange={(event) => onQuery(event.currentTarget.value)}
        />
      </div>
      <ChoiceSelect
        ariaLabel={t("library.sort")}
        value={sort}
        options={SORT_KEYS.map((key) => ({ value: key, label: sortNames[key] }))}
        className="min-w-40"
        onChange={(value) => {
          const next = SORT_KEYS.find((key) => key === value);
          if (next) onSort(next);
        }}
      />
      <ToggleGroup
        aria-label={t("library.view")}
        variant="outline"
        spacing={0}
        value={[mode]}
        onValueChange={(value) => {
          // A toggle group lets the active item be switched off; one view is always active, so that is ignored.
          const next = VIEW_MODES.find((candidate) => candidate === value[0]);
          if (next) onMode(next);
        }}
      >
        <ToggleGroupItem value="grid" aria-label={t("library.viewGrid")}>
          <LayoutGridIcon aria-hidden="true" />
        </ToggleGroupItem>
        <ToggleGroupItem value="list" aria-label={t("library.viewList")}>
          <ListIcon aria-hidden="true" />
        </ToggleGroupItem>
      </ToggleGroup>
      <div className="ml-auto">
        <Button type="button" variant="destructive" onClick={onStopAll}>
          <SquareIcon aria-hidden="true" data-icon="inline-start" />
          {t("sound.stopAll")}
        </Button>
      </div>
    </div>
  );
}
