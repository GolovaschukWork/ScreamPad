import { AudioLinesIcon, LibraryIcon, SettingsIcon, type LucideIcon } from "lucide-react";

/** Pages of the application. Add a page here, in `PAGES` and in the page switch of `App` when its feature exists. */
export type PageId = "library" | "settings";

export type NavItem = {
  id: PageId;
  /** Translation key of the label. */
  labelKey: "nav.library" | "nav.settings";
  icon: LucideIcon;
};

/** The one place that lists the navigation. Sections whose feature does not exist yet (boards, audio routing) are not
 *  listed at all, so they never appear as dead screens. */
export const NAV = {
  primary: [{ id: "library", labelKey: "nav.library", icon: LibraryIcon }],
  footer: [{ id: "settings", labelKey: "nav.settings", icon: SettingsIcon }],
} as const satisfies { primary: readonly NavItem[]; footer: readonly NavItem[] };

export const DEFAULT_PAGE: PageId = "library";

/** The brand mark shown at the top of the sidebar. A placeholder icon until a logo exists. */
export const BRAND_ICON = AudioLinesIcon;
