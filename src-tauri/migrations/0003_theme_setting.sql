-- Schema 3 (ADR 0011): the chosen UI theme is stored with the other application settings, never in the webview.
ALTER TABLE app_settings ADD COLUMN theme TEXT NOT NULL DEFAULT 'system'
    CHECK (theme IN ('system', 'dark', 'light'));
