import { LibraryView } from "../features/library/LibraryView";
import { SettingsPanel } from "../features/settings/SettingsPanel";

export default function App() {
  return (
    <main>
      <h1>ScreamPad</h1>
      <SettingsPanel />
      <LibraryView />
    </main>
  );
}
