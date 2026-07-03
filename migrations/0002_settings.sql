-- Single-row workspace settings (white-label branding).
CREATE TABLE settings (
    id           boolean PRIMARY KEY DEFAULT true CHECK (id),
    company_name text NOT NULL DEFAULT 'My Company',
    accent_color text NOT NULL DEFAULT '#0d9488',
    updated_at   timestamptz NOT NULL DEFAULT now()
);

INSERT INTO settings (id) VALUES (true);
