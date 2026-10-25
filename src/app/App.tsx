import { LibraryView } from "../features/library/LibraryView";
import { SettingsPanel } from "../features/settings/SettingsPanel";
import { Toaster } from "@/shared/ui/sonner";

export default function App() {
  return (
    <main>
      <h1>ScreamPad</h1>
      <SettingsPanel />
      <LibraryView />
      <Toaster />
    </main>
  );
}
