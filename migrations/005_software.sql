-- migrations/005_software.sql — Software inventory per device.
CREATE TABLE software_inventory (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    version      TEXT,
    publisher    TEXT,
    install_date DATE,
    scanned_at   TIMESTAMPTZ DEFAULT now(),
    UNIQUE (device_id, name)
);
