import { useTranslation } from "react-i18next";
import { useMediaQuery } from "@/shared/lib/useMediaQuery";
import { cn } from "@/shared/lib/utils";
import { Separator } from "@/shared/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { BRAND_ICON, NAV, type NavItem, type PageId } from "./navigation";

/** Below this window width the sidebar shows icons only and the labels move into tooltips. */
const COMPACT_QUERY = "(max-width: 719px)";

function NavButton({
  item,
  current,
  compact,
  onNavigate,
}: {
  item: NavItem;
  current: PageId;
  compact: boolean;
  onNavigate: (page: PageId) => void;
}) {
  const { t } = useTranslation();
  const Icon = item.icon;
  const active = item.id === current;
  const label = t(item.labelKey);
  return (
    <li>
      <Tooltip disabled={!compact}>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-current={active ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
              className={cn(
                "relative flex h-9 w-full items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors outline-none",
                "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                "focus-visible:ring-3 focus-visible:ring-ring/50",
                active &&
                  "bg-accent text-accent-foreground before:absolute before:inset-y-1.5 before:-left-2 before:w-1 before:rounded-full before:bg-primary",
                compact && "justify-center px-0",
              )}
            />
          }
        >
          <Icon aria-hidden="true" className="size-4 shrink-0" />
          <span className={cn(compact && "sr-only")}>{label}</span>
        </TooltipTrigger>
        <TooltipContent side="right">{label}</TooltipContent>
      </Tooltip>
    </li>
  );
}

export function AppSidebar({ current, onNavigate }: { current: PageId; onNavigate: (page: PageId) => void }) {
  const { t } = useTranslation();
  const compact = useMediaQuery(COMPACT_QUERY);
  const Brand = BRAND_ICON;
  return (
    <aside
      className={cn(
        "flex h-full shrink-0 flex-col gap-3 border-r bg-card py-3 text-card-foreground",
        compact ? "w-14 px-2" : "w-56 px-3",
      )}
    >
      <div className={cn("flex h-9 items-center gap-2 px-1", compact && "justify-center")}>
        <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground">
          <Brand aria-hidden="true" className="size-4" />
        </span>
        <span className={cn("truncate text-sm font-semibold", compact && "sr-only")}>ScreamPad</span>
      </div>
      <Separator />
      <nav aria-label={t("nav.label")} className="flex min-h-0 flex-1 flex-col justify-between">
        <ul className="space-y-1 overflow-y-auto">
          {NAV.primary.map((item) => (
            <NavButton key={item.id} item={item} current={current} compact={compact} onNavigate={onNavigate} />
          ))}
        </ul>
        <ul className="space-y-1">
          {NAV.footer.map((item) => (
            <NavButton key={item.id} item={item} current={current} compact={compact} onNavigate={onNavigate} />
          ))}
        </ul>
      </nav>
    </aside>
  );
}
