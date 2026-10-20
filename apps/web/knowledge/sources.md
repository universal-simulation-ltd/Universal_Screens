Universal Screens knowledge base: the research, standards and guidance behind each article. The same in every language (titles are not translated), so it is kept once, here, and read by every client beside the articles: the browser client fetches it, the desktop hosts embed it with include_str!, and the Android app packs it as an asset. Format: one source per block of "key: value" lines, each block closed by a "---" line. Keys: article (the article's id), kind (paper, standard, report, guidance or law), title, authors, publisher, year, href (the canonical free link), and pdf + licence for our hosted copy under opensource.unisim.co.uk/kb/, ONLY where the licence allows redistribution (US Government works, IETF RFCs, and the Noise specification, which its section 16 places in the public domain). Sources show in the order they appear here. Original research first, then standards, then guidance, and only for what the app really does: checked against the Rust workspace on 2026-10-12 (pairing is SPAKE2 in crates/pake, then Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s; reconnecting is Noise_XX with the remembered keys; v0.3 clients still get the old NNpsk0; the PIN comes from getrandom; discovery is mDNS/DNS-SD; screens are H.264 decoded by WebCodecs in the browser). Screens does not use VNC/RFB, so the remote-framebuffer paper is deliberately not cited, and "saved-computers-file" has no source that genuinely fits. Every link was fetched on 2026-10-12; ISO's site bot-blocks curl, so its page was confirmed through a Wayback Machine capture, and Wiegand's author copy moved off hhi.de, so it links to the Wayback Machine's snapshot.

---
article: what-is-screens
kind: paper
title: Overview of the H.264/AVC Video Coding Standard
authors: Thomas Wiegand, Gary J. Sullivan, Gisle Bjøntegaard, Ajay Luthra
publisher: IEEE Transactions on Circuits and Systems for Video Technology
year: 2003
href: https://web.archive.org/web/20240813232949/https://iphome.hhi.de/wiegand/assets/pdfs/csvt_overview_0305.pdf
---
article: what-is-screens
kind: standard
title: Recommendation ITU-T H.264: Advanced video coding for generic audiovisual services
publisher: ITU Telecommunication Standardization Sector
year: 2003
href: https://www.itu.int/rec/T-REC-H.264
---
article: what-is-screens
kind: standard
title: WebCodecs
publisher: W3C
href: https://www.w3.org/TR/webcodecs/
---
article: connecting
kind: standard
title: ISO/IEC 18004:2015 — QR Code bar code symbology specification
publisher: ISO/IEC
year: 2015
href: https://www.iso.org/standard/62021.html
---
article: connecting
kind: standard
title: Multicast DNS (RFC 6762)
authors: Stuart Cheshire, Marc Krochmal
publisher: IETF
year: 2013
href: https://www.rfc-editor.org/rfc/rfc6762.html
---
article: connecting
kind: standard
title: DNS-Based Service Discovery (RFC 6763)
authors: Stuart Cheshire, Marc Krochmal
publisher: IETF
year: 2013
href: https://www.rfc-editor.org/rfc/rfc6763.html
---
article: the-pin
kind: standard
title: Randomness Requirements for Security (RFC 4086)
authors: Donald E. Eastlake 3rd, Jeffrey I. Schiller, Steve Crocker
publisher: IETF
year: 2005
href: https://www.rfc-editor.org/rfc/rfc4086.html
---
article: the-pin
kind: guidance
title: NIST SP 800-63B-4: Digital Identity Guidelines — Authentication and Authenticator Management (rate limiting)
publisher: NIST
year: 2025
href: https://doi.org/10.6028/NIST.SP.800-63B-4
pdf: papers/nist-sp-800-63b-4-authentication.pdf
licence: Public domain (US Government work)
---
article: encryption
kind: paper
title: Simple Password-Based Encrypted Key Exchange Protocols
authors: Michel Abdalla, David Pointcheval
publisher: CT-RSA 2005
year: 2005
href: https://www.di.ens.fr/~abdalla/papers/AbPo05a-letter.pdf
---
article: encryption
kind: standard
title: SPAKE2, a Password-Authenticated Key Exchange (RFC 9382)
authors: Watson Ladd
publisher: IETF
year: 2023
href: https://www.rfc-editor.org/rfc/rfc9382.html
---
article: encryption
kind: standard
title: The Noise Protocol Framework, revision 34
authors: Trevor Perrin
publisher: noiseprotocol.org
year: 2018
href: https://noiseprotocol.org/noise.pdf
pdf: papers/noise-protocol-framework-rev34.pdf
licence: Public domain (dedicated by the author, §16 of the specification)
---
article: encryption
kind: paper
title: Curve25519: new Diffie-Hellman speed records
authors: Daniel J. Bernstein
publisher: PKC
year: 2006
href: https://cr.yp.to/ecdh/curve25519-20060209.pdf
---
article: encryption
kind: standard
title: ChaCha20 and Poly1305 for IETF Protocols (RFC 8439)
authors: Yoav Nir, Adam Langley
publisher: IETF
year: 2018
href: https://www.rfc-editor.org/rfc/rfc8439.html
---
article: encryption
kind: paper
title: BLAKE2: simpler, smaller, fast as MD5
authors: Jean-Philippe Aumasson, Samuel Neves, Zooko Wilcox-O'Hearn, Christian Winnerlein
publisher: ACNS
year: 2013
href: https://www.blake2.net/blake2.pdf
---
article: what-leaves-your-network
kind: standard
title: Multicast DNS (RFC 6762)
authors: Stuart Cheshire, Marc Krochmal
publisher: IETF
year: 2013
href: https://www.rfc-editor.org/rfc/rfc6762.html
---
article: what-leaves-your-network
kind: standard
title: Uniform Resource Identifier (URI): Generic Syntax (RFC 3986), §3.5 fragment
authors: Tim Berners-Lee, Roy T. Fielding, Larry Masinter
publisher: IETF
year: 2005
href: https://www.rfc-editor.org/rfc/rfc3986.html
---
article: what-leaves-your-network
kind: standard
title: The Noise Protocol Framework, revision 34 (why the relay sees only ciphertext)
authors: Trevor Perrin
publisher: noiseprotocol.org
year: 2018
href: https://noiseprotocol.org/noise.pdf
pdf: papers/noise-protocol-framework-rev34.pdf
licence: Public domain (dedicated by the author, §16 of the specification)
---
