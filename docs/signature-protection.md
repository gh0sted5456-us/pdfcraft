# Signature removal and protection in the personal build

Before choosing **Sign**, use **After signing** to choose:

- No additional restrictions: an ordinary approval signature.
- Lock the entire document: standard PDF DocMDP certification with no changes allowed.
- Allow forms/signatures or comments/forms/signatures: the other standard certification levels. Certification must be the first signature.
- Protect existing pages while allowing new pages, protect the signature's page, or choose individual pages: a PdfCraft-specific policy stored inside the signed signature dictionary.

PdfCraft page protection follows page object references rather than page numbers. Inserting a page ahead of a protected page does not unlock it. The engine checks page dictionaries, inherited geometry/resources, annotations, content streams and reachable resources before committing an edit. Protected pages cannot be deleted or change order relative to each other. Unprotected pages can be edited, inserted, deleted or moved subject to the document's other permissions. The organizer shows padlocks and a protection notice. Shared resources may conservatively prevent an edit on another page; bounded inspection refuses an edit when it cannot safely check protection.

Page protection is a PdfCraft workflow rule, not a standard per-page PDF permission or DRM. Other readers may ignore it. New pages and subsequent edits are not covered by the earlier signature; existing validation still reports changes. Standard DocMDP certification applies to the whole document and does not provide an add-pages permission. Restrictions must be chosen before signing; changing them requires removing the signature and signing again.

Validation reads protection from the actual cryptographically verified signed revision, not a later redefinition of its dictionary. Certificate trust and signature integrity are separate: an intact signature from an untrusted certificate can still carry a protection policy. Multiple signatures retain their independent policies.

Right-click a visible signature and choose **Remove signature…**, or use the Signatures panel. The page hit test uses the signature's bounds even when a matching form-cache row is absent. Removal requires the exact signing certificate plus a fresh, domain-separated private-key challenge. A matching name, cached certificate, different key or replayed challenge is insufficient. For a CAC/card, Windows may request its PIN; canceled or unavailable keys leave the document unchanged. A file digital ID requires its password. Physical CAC behavior still needs a native hardware check.

Removal saves a reviewed unsigned copy and keeps the original. An original destination is refused; failed saves roll back. Other signatures' certification/field locks and encryption permissions continue to apply. With no signature remaining, Save As writes a clean copy without the old signature revisions. When other signatures remain, the copy is incremental and the removed signature may remain recoverable in its earlier history.

## Headless use

`sign_document` accepts `certify` (`no_changes`, `form_fill`, `comments`) and optional `protected_pages` (at most 10000 existing 1-based page numbers), plus `allow_insert` (default true). `sign_list` exposes verified integrity and page object references for the signed policy. The existing page tools honor engine protection.

`sign_clear` requires `id` and, for file IDs, `password` when removing a digital signature. IDs can be root-confined PKCS #12 paths or native `windows:` / `keychain:` references. Visual Fill & Sign marks do not require a cryptographic identity. The low-level engine edit remains a document-edit primitive; applications must use the ownership proof before requesting digital removal.

Synthetic checks cover key proof replay/removal, wrong credentials, reviewed-copy rollback, signed policy rewriting, shared fonts, page rotation/deletion/insertion, persistence after save/reopen and unlocking by verified removal. No live private certificate or personal document is used.
