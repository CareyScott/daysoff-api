-- Approval workflow, request notes, half days, company-wide days off,
-- and login-screen privacy.

ALTER TABLE settings
    ADD COLUMN require_approval boolean NOT NULL DEFAULT false,
    ADD COLUMN hide_login_branding boolean NOT NULL DEFAULT false;

ALTER TABLE absences
    ADD COLUMN status text NOT NULL DEFAULT 'approved'
        CHECK (status IN ('pending', 'approved', 'denied')),
    ADD COLUMN decision_reason text,
    ADD COLUMN note text,
    ADD COLUMN day_part text NOT NULL DEFAULT 'full'
        CHECK (day_part IN ('full', 'am', 'pm'));

-- Half days count as 0.5.
ALTER TABLE absences ALTER COLUMN business_days TYPE double precision;
ALTER TABLE absences DROP CONSTRAINT absences_business_days_check;
ALTER TABLE absences ADD CONSTRAINT absences_business_days_check CHECK (business_days > 0);

-- Replace the overlap constraint: denied requests must not block dates, and
-- half-day combinations (am + pm on the same day) are validated in the app.
-- The constraint remains the race-proof backstop for full-day bookings.
DO $$
DECLARE c text;
BEGIN
    SELECT conname INTO c FROM pg_constraint
    WHERE conrelid = 'absences'::regclass AND contype = 'x';
    IF c IS NOT NULL THEN
        EXECUTE format('ALTER TABLE absences DROP CONSTRAINT %I', c);
    END IF;
END $$;

ALTER TABLE absences ADD CONSTRAINT absences_no_full_day_overlap
    EXCLUDE USING gist (user_id WITH =, daterange(start_date, end_date, '[]') WITH &&)
    WHERE (status <> 'denied' AND day_part = 'full');

-- Company-wide days off (bank holidays, company retreats, ...).
CREATE TABLE company_days (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text NOT NULL,
    start_date date NOT NULL,
    end_date   date NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (end_date >= start_date)
);
CREATE INDEX company_days_start_idx ON company_days (start_date);
