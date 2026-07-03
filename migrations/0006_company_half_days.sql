-- Company days can be half days (single-date AM/PM), e.g. an afternoon off
-- before a holiday.
ALTER TABLE company_days ADD COLUMN day_part text NOT NULL DEFAULT 'full'
    CHECK (day_part IN ('full', 'am', 'pm'));
