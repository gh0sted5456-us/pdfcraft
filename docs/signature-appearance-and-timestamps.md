# Signature appearance and trusted timestamps

The personal 0.13.1 build separates the signature graphic on the left from the
background behind the certificate details. The background starts as the existing
PdfCraft lion at 12% opacity. Choose **None** to remove it or **Choose background
image…** to replace it with a PNG or JPEG. The preview and the signed PDF use the
same background; the last successfully signed choice is remembered separately
from the foreground graphic.

In **Use a certificate**, **Timestamp** adds a standalone RFC 3161 document
timestamp and saves the result. **Add trusted timestamp** in **Sign as** adds an
RFC 3161 signature timestamp while signing (PAdES B-T). The server URL is editable
and remembered. Timestamping while signing starts unchecked.

The default server is `https://timestamp.sectigo.com`, the HTTPS endpoint of
[Sectigo's documented timestamp service](https://support.sectigo.com/PS_KnowledgeDetailPageFaq?Id=kA03l000000c4TX).
Requests happen only after an explicit timestamp/sign action. The service receives
a digest, nonce and protocol metadata, not the PDF or the CAC private key.
Requests use the OS TLS trust store, a 20-second timeout, a 4 MiB response limit
and no redirects. Remote servers require HTTPS; HTTP is allowed only for loopback
test authorities. PdfCraft verifies the returned token's signature, digest and
nonce before saving. A failure or cancellation leaves existing files unchanged.

Timestamp validation uses existing OS certificate authorities in a separate,
timestamp-only trust list. They do not establish the identity of an ordinary PDF
signer. Explicitly trusted certificates in PdfCraft continue to work for both
roles. No OS certificate store is changed. An unknown authority remains unknown.
The timestamp worker runs off the UI thread and refuses to write a result if the
document changed or closed while it was waiting.

For headless use, enable networking explicitly with `pdfcraft-cli run --network`
or `pdfcraft-cli mcp --network`. `doc_timestamp` takes `doc`, `out` and
`timestamp_server`; `sign_document` accepts an optional `timestamp_server`.
Omitting it keeps ordinary offline signing. The existing root restrictions and
atomic destination writes apply. `sign_document.background` accepts `"pdfcraft"`
or a graphic object such as `{"image":"watermark.png"}`. It is independent of
`graphic`, which controls the foreground.

Timestamp transport is desktop-only. It does not automatically collect revocation
evidence or make a PAdES B-LTA signature. New appearance/server labels are currently
English; existing translated signing labels retain their language.

The timestamp core was already present. The background-worker snapshot and HTTP
integration build on the approach in [upstream PR #4193 by @MarAvFe](https://github.com/storytold/pdfcraft/pull/4193),
with the scope limited here to RFC 3161 timestamps, strict HTTPS and separate TSA
trust. The PKCS #11 and revocation-fetching features of that proposal are not included.
