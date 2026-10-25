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
and remembered. Timestamping while signing starts unchecked. Both dialogs share
the same service selector: **Sectigo (default)**, **FreeTSA (manual trust)** and
**Custom…**. Choosing a service does not contact it. The selected URL remains
editable and is remembered across launches; Custom opens an empty URL field.

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

[FreeTSA documents its HTTPS RFC 3161 endpoint](https://www.freetsa.org/index_en.php).
It returned a cryptographically verified token in a live synthetic check, but its
authority was unknown to the existing OS roots. Selecting it adds no trust; review
and explicitly trust its certificate separately if your policy permits it. Its
[public service policy](https://www.freetsa.org/freetsa.html) limits use to one
request per second per IP and offers no availability guarantee. There is no
automatic fallback between providers. Sectigo remains the default.

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

## DoD time services

Ordinary signing defaults to the **computer clock**, including CAC signing, and
the appearance dialog identifies this source. **Add trusted timestamp** is off
for a new signing draft; enabling it additionally requests a signed timestamp
from the selected service. A local signing date is never reported as a trusted
server timestamp. PdfCraft neither changes nor synchronizes the system clock.

No publicly documented, accessible DoD RFC 3161 endpoint was verified in the
2026-10-24 research pass. The [Naval Observatory's documented NTP services](https://www.cnmoc.usff.navy.mil/Our-Commands/United-States-Naval-Observatory/Precise-Time-Department/Network-Time-Protocol-NTP/)
synchronize clocks; they do not issue the signed tokens that
[RFC 3161](https://www.rfc-editor.org/info/rfc3161/) requires for PDF timestamps.
Its authenticated DoD NTP service is restricted to NIPRnet. CAC presence alone
does not establish access to that network or identify an authorized PDF TSA.

Sectigo remains the timestamp default. There is no automatic DoD routing, clock
change or guessed military endpoint. A verified organization-provided HTTPS
RFC 3161 URL can already be entered as Custom and used by the shared transport.
Automatic CAC-based selection remains pending a verified compatible service and
its access/trust requirements; private-key or client-certificate authentication
must not be assumed from merely inserting a card.
