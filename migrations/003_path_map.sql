-- Path prefixes as the client sees them, mapped to where Graft sees the same files (JSON).
ALTER TABLE clients ADD COLUMN path_map TEXT NOT NULL DEFAULT '[]';
