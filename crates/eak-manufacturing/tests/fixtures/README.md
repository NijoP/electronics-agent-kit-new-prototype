# Sourcing test fixtures

Every file in this directory is **synthetic**: written by hand for the tests in
`../sourcing.rs`. None of it was retrieved from Distributor, Fabricator or any other supplier, and none
of it describes a real part. The part numbers (`C99000000x`), the manufacturer
("EAK Test Fixtures") and all values are fictional, so nothing here can be mistaken for
supplier data.

The JSON files use the product-detail field names listed in `docs/engineering/sourcing.md`.
That mapping has not been checked against Distributor's own API documentation.

| File | Purpose |
|---|---|
| `distributor/product_detail_resistor.json` | Complete record in the `{"code": 200, "result": {...}}` envelope |
| `distributor/product_detail_bare_mcu.json` | Bare product object, no stock or price |
| `distributor/product_detail_missing_fields.json` | Manufacturer, package, datasheet and parameters absent |
| `distributor/product_detail_hostile.json` | Injection text, wrong types, bad URL scheme, self-certification attempts |
| `distributor/malformed_truncated.json` | Response cut off mid-document |
| `distributor/access_denied.html` | An HTML error page where JSON was expected |
| `distributor/provider_error.json` | Provider-level error envelope |
| `datasheets/EAKFIX-R0603-10K.pdf` | Minimal PDF whose document title names the fixture MPN |
