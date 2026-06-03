-- NIC card type for DPA interfaces (SVPC vs ASTRA).
CREATE TYPE dpa_interface_type AS ENUM ('SVPC', 'ASTRA');

ALTER TABLE dpa_interfaces
    ADD COLUMN interface_type dpa_interface_type NOT NULL;
