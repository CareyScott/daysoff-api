-- Cancelling an already-started vacation becomes a request an admin approves.
ALTER TABLE absences DROP CONSTRAINT absences_status_check;
ALTER TABLE absences ADD CONSTRAINT absences_status_check
    CHECK (status IN ('pending', 'approved', 'denied', 'cancel_pending'));
