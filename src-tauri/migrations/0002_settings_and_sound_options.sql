-- Schema 2 (ADR 0010): editable per-sound playback options and the application settings.
-- Gains are integer millibels (1/100 dB). The media columns of `sounds` stay immutable: the trigger of schema 1 lists them
-- explicitly, so the new columns can be updated.
ALTER TABLE sounds ADD COLUMN gain_millibels INTEGER NOT NULL DEFAULT 0
    CHECK (gain_millibels BETWEEN -6000 AND 600);
ALTER TABLE sounds ADD COLUMN play_mode TEXT NOT NULL DEFAULT 'overlap'
    CHECK (play_mode IN ('overlap', 'restart', 'ignore', 'toggle'));

-- Exactly one row (id = 1), created below. The supported languages are checked by the application, so adding a language
-- needs no migration; the database only checks the shape of the tag.
CREATE TABLE app_settings (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    language              TEXT    NOT NULL DEFAULT 'system'
        CHECK (language = 'system' OR (length(language) BETWEEN 2 AND 8 AND language NOT GLOB '*[^a-z-]*')),
    master_gain_millibels INTEGER NOT NULL DEFAULT 0 CHECK (master_gain_millibels BETWEEN -6000 AND 0),
    updated_at_ms         INTEGER NOT NULL DEFAULT 0 CHECK (updated_at_ms >= 0)
) STRICT;

INSERT INTO app_settings (id) VALUES (1);
