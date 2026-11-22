# Test fixtures of `vba`

Genuine VBA projects saved by Microsoft Office (with compiled p-code), used
to test the reader and the "rewrite an existing project" path of the `vba`
module (`tests/vba_*.rs`). They came with that code from the `ras-ovba` crate
of [RAS](https://github.com/cool0707/ras). None of them was created by these
repositories; all come from projects whose licences permit redistribution.
The licence texts are next to this file.

| File | Origin | Licence |
|---|---|---|
| `xlsxwriter_vbaProject.bin` | `examples/vbaProject.bin` of [XlsxWriter](https://github.com/jmcnamara/XlsxWriter) 3.3.1 (PyPI sdist), unmodified. Saved by Excel for Windows: code page 1252, `_VBA_PROJECT` version 0x0085, modules `ThisWorkbook`, `Sheet1`, `Module1`, `ThisWorkbook1`, `Sheet2`. | BSD-2-Clause, Copyright (c) 2013-2026 John McNamara — `LICENSE-XlsxWriter.txt` |
| `rust_xlsxwriter_mac_vbaProject.bin` | `examples/vbaProject.bin` of the [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter) 0.79.0 crate, unmodified. Saved by Excel for Mac: code page 10000, LF line endings, `PROJECTCOMPATVERSION` record, `_VBA_PROJECT` version 0x00DF. | MIT OR Apache-2.0, Copyright 2022-2024 John McNamara — `LICENSE-rust_xlsxwriter-MIT.txt` |
| `rust_xlsxwriter_macro04_signed.xlsm` | `tests/input/macro04.xlsm` of the rust_xlsxwriter 0.79.0 crate, unmodified. A workbook whose VBA project is digitally signed (`xl/vbaProjectSignature.bin`). | MIT OR Apache-2.0, Copyright 2022-2024 John McNamara — `LICENSE-rust_xlsxwriter-MIT.txt` |
| `oletools_oleform-PR314_vbaProject.bin` | The part `word/vbaProject.bin` extracted (no other change) from `tests/test-data/oleform/oleform-PR314.docm` of [oletools](https://github.com/decalage2/oletools) 0.60.2 (PyPI sdist). Saved by Word: two user forms with nested designer storages, all four kinds of reference records (`REFERENCEREGISTERED`, `REFERENCEPROJECT`, `REFERENCEORIGINAL` + `REFERENCECONTROL`). | BSD-2-Clause, Copyright (c) 2012-2024 Philippe Lagadec — `LICENSE-oletools.txt` |

SHA-256:

```
0ced1464b3677e98f5e3a8c5d80135e18dc98dca39299f1a8cfd2a00999fbf9f  xlsxwriter_vbaProject.bin
7657caf22c3fafc308beabe6bd6a0d359e0aa48256766fefcee458127fd6cca0  rust_xlsxwriter_mac_vbaProject.bin
d49e30414b59e66446689d77119fa5b9f4f99fa86dbcade003f4d3fe1b8e225d  rust_xlsxwriter_macro04_signed.xlsm
f561676e52a302b6354e0dc9259581f304f334f8994984e09e4283ffc175c5de  oletools_oleform-PR314_vbaProject.bin
```
