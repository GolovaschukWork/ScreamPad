import { LibraryView } from "../features/library/LibraryView";
import { SettingsPanel } from "../features/settings/SettingsPanel";

export default function App() {
  return (
    <main className="app">
      <h1 className="app-title">ScreamPad</h1>
      <SettingsPanel />
      <LibraryView />
    </main>
  );
}
