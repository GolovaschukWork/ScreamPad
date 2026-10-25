-- Frozen snapshot of a database created by schema version 1, with sample rows. Never edit this file: it stands in for
-- databases already in users' hands. Add schema_vN.sql for every new migration instead.
CREATE TABLE sounds (
    id                 INTEGER PRIMARY KEY,
    content_hash       TEXT    NOT NULL UNIQUE
        CHECK (length(content_hash) = 64 AND length(CAST(content_hash AS BLOB)) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*'),
    display_name       TEXT    NOT NULL
        CHECK (length(trim(display_name)) > 0),
    original_file_name TEXT
        CHECK (original_file_name IS NULL OR (length(original_file_name) > 0 AND original_file_name NOT GLOB '*[/\]*')),
    managed_path       TEXT    NOT NULL UNIQUE
        CHECK (
            length(managed_path) BETWEEN 1 AND 255
            AND length(CAST(managed_path AS BLOB)) = length(managed_path)
            AND managed_path NOT GLOB '*[^a-z0-9._/-]*'
            AND managed_path NOT GLOB '/*'
            AND managed_path NOT GLOB '.*'
            AND managed_path NOT GLOB '*/'
            AND managed_path NOT GLOB '*//*'
            AND managed_path NOT GLOB '*..*'
            AND managed_path NOT GLOB '*/.*'
        ),
    format             TEXT    NOT NULL CHECK (length(format) > 0),
    duration_ms        INTEGER NOT NULL CHECK (duration_ms >= 0),
    sample_rate        INTEGER NOT NULL CHECK (sample_rate > 0),
    channels           INTEGER NOT NULL CHECK (channels > 0),
    size_bytes         INTEGER NOT NULL CHECK (size_bytes >= 0),
    created_at_ms      INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms      INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
) STRICT;
CREATE INDEX sounds_display_name ON sounds (display_name COLLATE NOCASE);
CREATE TRIGGER sounds_media_columns_are_immutable
BEFORE UPDATE OF content_hash, managed_path, format, duration_ms, sample_rate, channels, size_bytes, created_at_ms ON sounds
BEGIN
    SELECT RAISE(ABORT, 'managed media columns are immutable');
END;

INSERT INTO sounds (content_hash, display_name, original_file_name, managed_path, format, duration_ms, sample_rate, channels, size_bytes, created_at_ms, updated_at_ms) VALUES
 ('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855', 'Airhorn', 'airhorn.wav', 'e3/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.wav', 'wav', 1500, 48000, 2, 288044, 1760000000000, 1760000000000),
 ('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad', 'Rimshot', NULL, 'ba/ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.mp3', 'mp3', 800, 44100, 1, 12800, 1760000001000, 1760000002000);
