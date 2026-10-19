# Moli XML parser fork

The `moli-xml` branch carries Moli's XML DOM changes on top of upstream
`7760920edff08e6dfd4b62affee39d8084e9dc45` (0.40.1). These replace the
`vendor/xml5ever-0.40.0` copy in Moli.

The tokenizer emits `Token::Cdata` for complete sections, including empty ones.
XML tree sinks implement `tree_builder::XmlTreeSink::create_cdata` to create a
distinct CDATA node. Incomplete sections remain text and report a parse error.
The reference `RcDom` implements this operation by flattening CDATA to text,
consistent with its existing node model; browser sinks preserve the distinction.
Namespace declarations retain their XMLNS namespace and source attribute order;
namespace binding still happens before resolving ordinary attributes. Numeric
references follow the XML 1.0 Char production, without HTML C1 substitutions.
Unclosed elements report an error at EOF. Upstream processing instruction and
qualified-attribute duplicate fixes are retained.

The parser remains an error-recovering parser. Moli's stricter lexical XML
validation stays in Moli; it is not part of this fork.

`xml5ever` deliberately depends on registry `markup5ever` rather than its Git
workspace path. This lets a downstream `[patch.crates-io]` replace only
`xml5ever`, while HTML parsing and Stylo keep the same registry atom, name and
tendril types. The root development patch redirects `markup5ever` to the local
workspace for this repository's own builds and does not propagate to consumers.

Run the XML crate checks from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy -p xml5ever --all-targets --all-features -- -D warnings
cargo nextest run -p xml5ever --no-fail-fast
cargo test -p xml5ever --doc
```

The XML regression tests use a DOM sink implementing `XmlTreeSink`; they cover
CDATA boundaries under every input split, unfinished sections, XML character
reference boundaries, namespace attribute identities/order and unexpected EOF.
