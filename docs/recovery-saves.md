# Saving recovered work

After recovering a document, the first Save opens Save As in the recorded folder.
The path in the recovery metadata is a suggestion until you choose a destination.
After a successful save, later saves use the chosen file normally. Cancelling or
a failed write leaves the recovery entry available.

Unsigned recovery copies saved to another file are rewritten cleanly, without
retaining the earlier incremental revisions. The existing signed-document and
read-only protections still apply. Recovery metadata reads are capped at 64 KiB;
an oversized sidecar is treated as damaged metadata and its PDF is offered without
an original path.

This personal build adapts [upstream PR #4327 by @Lcoding15](https://github.com/storytold/pdfcraft/pull/4327)
for [issue #3923](https://github.com/storytold/pdfcraft/issues/3923), with an
additional full-copy guard and regression assertion for the first recovery save.
Recovery remains desktop-only.
