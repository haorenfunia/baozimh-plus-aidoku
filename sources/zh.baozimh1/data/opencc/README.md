# Traditional to Simplified Dictionary

The unmodified `TSCharacters.txt` and `TSPhrases.txt` files are from OpenCC
release `ver.1.1.9`, under Apache-2.0. See the accompanying LICENSE.

Upstream: https://github.com/BYVoid/OpenCC/tree/ver.1.1.9/data/dictionary

SHA-256:

- TSCharacters.txt: 6b5a0a799bea2bb22c001f635eaa3fc2904310f0c08addbff275477a80ecf09a
- TSPhrases.txt: 504169029c43f7f234b8e2ae470720af3657675c5574ff8aa0feb257e1dc5ce2

`build.rs` combines and sorts both dictionaries at build time. The no_std
runtime uses longest-prefix matching, including phrase exceptions such as
乾坤 and 乾隆. No remote conversion service or runtime dictionary loading is used.
Only display text is converted; identifiers and URLs are not converted.
