import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LibraryView } from "../features/library/LibraryView";
import { SettingsPage } from "../features/settings/SettingsPage";
import { Toaster } from "@/shared/ui/sonner";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { AppShell, PageBody, PageHeader } from "./AppShell";
import { DEFAULT_PAGE, type PageId } from "./navigation";

export default function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<PageId>(DEFAULT_PAGE);

  return (
    <TooltipProvider>
      <AppShell current={page} onNavigate={setPage}>
        {page === "library" && (
          <>
            <PageHeader title={t("nav.library")} />
            <PageBody>
              <LibraryView />
            </PageBody>
          </>
        )}
        {page === "settings" && (
          <>
            <PageHeader title={t("nav.settings")} />
            <PageBody>
              <SettingsPage />
            </PageBody>
          </>
        )}
      </AppShell>
      <Toaster />
    </TooltipProvider>
  );
}
