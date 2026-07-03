CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE users (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email                text NOT NULL,
    name                 text NOT NULL,
    password_hash        text NOT NULL,
    role                 text NOT NULL DEFAULT 'member' CHECK (role IN ('admin', 'member')),
    active               boolean NOT NULL DEFAULT true,
    must_change_password boolean NOT NULL DEFAULT false,
    created_at           timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX users_email_key ON users (lower(email));

CREATE TABLE allowances (
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    year    integer NOT NULL CHECK (year BETWEEN 2000 AND 2100),
    days    integer NOT NULL CHECK (days >= 0),
    PRIMARY KEY (user_id, year)
);

CREATE TABLE absences (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind          text NOT NULL CHECK (kind IN ('vacation', 'sick')),
    start_date    date NOT NULL,
    end_date      date NOT NULL,
    business_days integer NOT NULL CHECK (business_days > 0),
    created_at    timestamptz NOT NULL DEFAULT now(),
    CHECK (end_date >= start_date),
    CHECK (date_part('year', start_date) = date_part('year', end_date)),
    EXCLUDE USING gist (user_id WITH =, daterange(start_date, end_date, '[]') WITH &&)
);

CREATE INDEX absences_user_year_idx ON absences (user_id, start_date);
