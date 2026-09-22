# CR-02 OCR corpus and performance evidence

Generated: 2026-08-06T18:17:05Z

This report uses an approved synthetic corpus only; it contains no customer data. Sample size: **12** (10 supported processing samples and 2 negative/unsupported samples).

## Runtime and hardware

- Python: Python 3.13.13
- OCR engine: tesseract v5.5.3.20260724
- Platform: Windows-11-10.0.26200-SP0
- Processor: Intel64 Family 6 Model 141 Stepping 1, GenuineIntel
- Logical CPUs: 12

## Field accuracy

| Field          | Correct | Accuracy |
| -------------- | ------: | -------: |
| Invoice Number |   10/10 |   100.0% |
| Invoice Date   |   10/10 |   100.0% |
| Supplier       |   10/10 |   100.0% |
| Currency       |   10/10 |   100.0% |
| Total          |   10/10 |   100.0% |

- Processing failure rate: **0.0%**
- Mean processing time: **972.9 ms/document**
- Median processing time: **971.5 ms/document**
- Maximum processing time: **996 ms/document**

## Unsupported cases

- password-protected PDF
- handwritten invoice
- non-English OCR models
- files whose extension and byte signature disagree
- documents larger than 250 MiB

## Representative real/anonymized benchmark process

No real customer invoice is committed to the repository. Founder-owned or customer-consented samples must be anonymized, independently ground-truthed by two finance reviewers, run locally, reported only in aggregate, and deleted from temporary processing storage after the run. The exact process is recorded in the JSON evidence file.
