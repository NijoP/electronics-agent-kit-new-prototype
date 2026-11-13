# Sourcing test fixtures

Every file in this directory is **synthetic**: written by hand for the tests in
`../sourcing.rs`. None of it was retrieved from LCSC, JLCPCB or any other supplier, and none
of it describes a real part. The part numbers (`C99000000x`), the manufacturer
("EAK Test Fixtures") and all values are fictional, so nothing here can be mistaken for
supplier data.

The JSON files use the product-detail field names listed in `docs/engineering/sourcing.md`.
That mapping has not been checked against LCSC's own API documentation.

| File | Purpose |
|---|---|
| `lcsc/product_detail_resistor.json` | Complete record in the `{"code": 200, "result": {...}}` envelope |
| `lcsc/product_detail_bare_mcu.json` | Bare product object, no stock or price |
| `lcsc/product_detail_missing_fields.json` | Manufacturer, package, datasheet and parameters absent |
| `lcsc/product_detail_hostile.json` | Injection text, wrong types, bad URL scheme, self-certification attempts |
| `lcsc/malformed_truncated.json` | Response cut off mid-document |
| `lcsc/access_denied.html` | An HTML error page where JSON was expected |
| `lcsc/provider_error.json` | Provider-level error envelope |
| `datasheets/EAKFIX-R0603-10K.pdf` | Minimal PDF whose document title names the fixture MPN |
