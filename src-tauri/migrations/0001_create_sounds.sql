-- Library of imported sounds. Timestamps are Unix milliseconds (UTC).
-- Managed media files are immutable after a successful import, so the columns that describe the file cannot change.
CREATE TABLE sounds (
    id                 INTEGER PRIMARY KEY,
    -- SHA-256 of the original imported bytes, lowercase hex (64 characters). Used for duplicate detection.
    -- GLOB and length() stop at a NUL byte, so the byte length is checked too.
    content_hash       TEXT    NOT NULL UNIQUE
        CHECK (length(content_hash) = 64 AND length(CAST(content_hash AS BLOB)) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*'),
    display_name       TEXT    NOT NULL
        CHECK (length(trim(display_name)) > 0),
    -- File name only; the source path is never stored.
    original_file_name TEXT
        CHECK (original_file_name IS NULL OR (length(original_file_name) > 0 AND original_file_name NOT GLOB '*[/\]*')),
    -- Safe relative storage key under the library root: lowercase letters, digits, '.', '_', '-' and '/' separators;
    -- no leading '/', no leading '.', no '..', no '//', no trailing '/'.
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
