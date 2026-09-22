# Tally XML file pilot v1

This pilot consumes documented Tally XML `ENVELOPE` files through native file
selection. It imports `LEDGER` masters and purchase/sales `VOUCHER` records. It
does not connect to Tally's HTTP listener, request credentials, modify a Tally
company, or export Tally XML.

Imported vouchers enter AIFOS's dedicated external-invoice staging table with
`needs_review` status; they are not approved invoice snapshots and cannot
bypass the existing human-review state machine. Reimporting an identical
canonical record is skipped using a deterministic idempotency key.

## Compatibility matrix

| Interface                                    | Adapter status        | Evidence                                                         |
| -------------------------------------------- | --------------------- | ---------------------------------------------------------------- |
| XML `ENVELOPE` with `LEDGER`                 | Supported pilot       | Synthetic golden fixture and parser tests                        |
| XML `ENVELOPE` with purchase/sales `VOUCHER` | Supported pilot       | Synthetic fixture including nested ledger entries                |
| File-based import/export workflow            | Supported             | Tally's documented XML integration/import workflow               |
| HTTP XML listener                            | Deliberately disabled | Network and credential surface not required for WP-11            |
| JSON import, ODBC, custom TDL schemas        | Deferred              | Requires customer-version discovery and separate mappings        |
| Tally XML export from AIFOS                  | Deferred              | Requires approved ledger/account mappings to avoid unsafe output |

References: [Tally XML integration](https://help.tallysolutions.com/xml-integration/),
[XML tags](https://help.tallysolutions.com/understanding-tally-xml-tags/), and
[importing XML/JSON](https://help.tallysolutions.com/import-data-from-xml-or-json/).

The matrix describes the file contract tested by AIFOS, not blanket
certification of every TallyPrime release or customization.
