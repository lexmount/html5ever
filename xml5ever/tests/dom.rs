use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use markup5ever::buffer_queue::BufferQueue;
use markup5ever::interface::tree_builder::{ElementFlags, QuirksMode};
use markup5ever::{ns, Attribute, ExpandedName, QualName};
use xml5ever::driver::parse_document;
use xml5ever::tendril::{StrTendril, TendrilSink};
use xml5ever::tokenizer::{ProcessResult, Token, TokenSink, XmlTokenizer};
use xml5ever::tree_builder::{NodeOrText, TreeSink, XmlTreeSink};

#[derive(Debug, PartialEq)]
enum Data {
    Document,
    Element(QualName, Vec<Attribute>),
    Text(String),
    Cdata(String),
    Comment(String),
    Pi(String, String),
}

#[derive(Debug)]
struct Node {
    data: Data,
    children: RefCell<Vec<Rc<Node>>>,
}

impl Node {
    fn new(data: Data) -> Rc<Self> {
        Rc::new(Self {
            data,
            children: RefCell::new(Vec::new()),
        })
    }
}

struct Dom {
    document: Rc<Node>,
    errors: RefCell<Vec<String>>,
}

impl Default for Dom {
    fn default() -> Self {
        Self {
            document: Node::new(Data::Document),
            errors: RefCell::new(Vec::new()),
        }
    }
}

impl TreeSink for Dom {
    type Handle = Rc<Node>;
    type Output = Self;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, message: Cow<'static, str>) {
        self.errors.borrow_mut().push(message.into_owned());
    }

    fn get_document(&self) -> Rc<Node> {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, node: &'a Rc<Node>) -> ExpandedName<'a> {
        let Data::Element(name, _) = &node.data else {
            panic!("expected an element");
        };
        name.expanded()
    }

    fn create_element(
        &self,
        name: QualName,
        attributes: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Rc<Node> {
        Node::new(Data::Element(name, attributes))
    }

    fn create_comment(&self, text: StrTendril) -> Rc<Node> {
        Node::new(Data::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, text: StrTendril) -> Rc<Node> {
        Node::new(Data::Pi(target.to_string(), text.to_string()))
    }

    fn append(&self, parent: &Rc<Node>, child: NodeOrText<Rc<Node>>) {
        let node = match child {
            NodeOrText::AppendNode(node) => node,
            NodeOrText::AppendText(text) => Node::new(Data::Text(text.to_string())),
        };
        parent.children.borrow_mut().push(node);
    }

    fn append_based_on_parent_node(
        &self,
        _element: &Rc<Node>,
        _previous: &Rc<Node>,
        _child: NodeOrText<Rc<Node>>,
    ) {
        unreachable!("HTML foster parenting is not used by these XML documents");
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public: StrTendril,
        _system: StrTendril,
    ) {
        unreachable!("these documents do not contain a doctype");
    }

    fn get_template_contents(&self, _target: &Rc<Node>) -> Rc<Node> {
        unreachable!("these documents do not contain HTML templates");
    }

    fn same_node(&self, left: &Rc<Node>, right: &Rc<Node>) -> bool {
        Rc::ptr_eq(left, right)
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, _sibling: &Rc<Node>, _child: NodeOrText<Rc<Node>>) {
        unreachable!("these XML documents append children in source order");
    }

    fn add_attrs_if_missing(&self, _target: &Rc<Node>, _attributes: Vec<Attribute>) {
        unreachable!("XML attributes are supplied at element creation");
    }

    fn remove_from_parent(&self, _target: &Rc<Node>) {
        unreachable!("these XML documents do not reparent nodes");
    }

    fn reparent_children(&self, _node: &Rc<Node>, _parent: &Rc<Node>) {
        unreachable!("these XML documents do not reparent nodes");
    }
}

impl XmlTreeSink for Dom {
    fn create_cdata(&self, text: StrTendril) -> Rc<Node> {
        Node::new(Data::Cdata(text.to_string()))
    }
}

#[derive(Default)]
struct Collector(RefCell<Vec<Token>>);

impl TokenSink for Collector {
    type Handle = ();

    fn process_token(&self, token: Token) -> ProcessResult<()> {
        self.0.borrow_mut().push(token);
        ProcessResult::Continue
    }
}

fn tokens(chunks: &[&str]) -> Vec<Token> {
    let tokenizer = XmlTokenizer::new(Collector::default(), Default::default());
    let input = BufferQueue::default();
    for chunk in chunks {
        input.push_back(StrTendril::from(*chunk));
        let _ = tokenizer.feed(&input);
    }
    tokenizer.end();
    tokenizer.sink.0.into_inner()
}

#[test]
fn cdata_preserves_empty_sections_and_boundaries_for_every_input_split() {
    let source = "<root>before<![CDATA[α<>&]]x]]]><![CDATA[]]>after</root>";
    for (split, _) in source
        .char_indices()
        .chain(std::iter::once((source.len(), '\0')))
    {
        let mut parser = parse_document(Dom::default(), Default::default());
        parser.process(StrTendril::from(&source[..split]));
        parser.process(StrTendril::from(&source[split..]));
        let dom = parser.finish();
        assert!(dom.errors.borrow().is_empty(), "split at {split}");
        let root = dom.document.children.borrow()[0].clone();
        let children = root.children.borrow();
        let sections: Vec<_> = children
            .iter()
            .filter_map(|node| match &node.data {
                Data::Cdata(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(sections, ["α<>&]]x]", ""], "split at {split}");
        assert!(
            matches!(&children.first().unwrap().data, Data::Text(text) if text.starts_with('b'))
        );
        assert!(matches!(&children.last().unwrap().data, Data::Text(text) if text.ends_with('r')));
    }
}

#[test]
fn unfinished_cdata_reports_an_error_and_keeps_all_trailing_brackets() {
    for suffix in ["value", "value]", "value]]", "value]]]"] {
        let source = format!("<![CDATA[{suffix}");
        let result = tokens(&[&source]);
        assert!(result
            .iter()
            .any(|token| matches!(token, Token::ParseError(_))));
        assert!(!result.iter().any(|token| matches!(token, Token::Cdata(_))));
        let text: String = result
            .iter()
            .filter_map(|token| match token {
                Token::Characters(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(text, suffix);
    }
}

#[test]
fn numeric_references_use_xml_char_in_text_and_attributes() {
    for value in [
        0x9, 0xa, 0xd, 0x20, 0x7f, 0x80, 0x85, 0x9f, 0xd7ff, 0xe000, 0xfdd0, 0xfffd, 0x10000,
        0x10ffff,
    ] {
        for reference in [format!("&#{value};"), format!("&#x{value:x};")] {
            let expected = char::from_u32(value).unwrap();
            let source = format!("<root a='{reference}'>{reference}</root>");
            let dom = parse_document(Dom::default(), Default::default()).one(source);
            assert!(
                dom.errors.borrow().is_empty(),
                "{reference}: {:?}",
                dom.errors
            );
            let root = dom.document.children.borrow()[0].clone();
            let Data::Element(_, attrs) = &root.data else {
                unreachable!()
            };
            assert_eq!(attrs[0].value.as_ref(), expected.to_string(), "{reference}");
            assert_eq!(
                root.children.borrow()[0].data,
                Data::Text(expected.to_string())
            );
        }
    }
}

#[test]
fn numeric_references_reject_non_xml_char_values() {
    for value in [
        0,
        1,
        8,
        0xb,
        0xc,
        0xe,
        0x1f,
        0xd800,
        0xdfff,
        0xfffe,
        0xffff,
        0x110000,
        u32::MAX,
    ] {
        let source = format!("<root a='&#x{value:x};'>&#x{value:x};</root>");
        let dom = parse_document(Dom::default(), Default::default()).one(source);
        assert!(!dom.errors.borrow().is_empty(), "{value:x}");
        let root = dom.document.children.borrow()[0].clone();
        let Data::Element(_, attrs) = &root.data else {
            unreachable!()
        };
        assert_eq!(attrs[0].value.as_ref(), "\u{fffd}");
        assert_eq!(
            root.children.borrow()[0].data,
            Data::Text("\u{fffd}".to_owned())
        );
    }
}

#[test]
fn namespace_declarations_remain_attributes_in_source_order() {
    let dom = parse_document(Dom::default(), Default::default()).one(
        "<p:root p:first='1' plain='2' xmlns:p='urn:p' xmlns='urn:default' xml:lang='en' lang='fr'/>",
    );
    assert!(dom.errors.borrow().is_empty(), "{:?}", dom.errors);
    let root = dom.document.children.borrow()[0].clone();
    let Data::Element(name, attrs) = &root.data else {
        unreachable!()
    };
    assert_eq!(&*name.ns, "urn:p");
    let identities: Vec<_> = attrs
        .iter()
        .map(|attr| {
            (
                attr.name.prefix.as_ref().map(|prefix| prefix.as_ref()),
                attr.name.local.as_ref(),
                attr.name.ns.as_ref(),
                attr.value.as_ref(),
            )
        })
        .collect();
    assert_eq!(
        identities,
        [
            (Some("p"), "first", "urn:p", "1"),
            (None, "plain", "", "2"),
            (Some("xmlns"), "p", ns!(xmlns).as_ref(), "urn:p"),
            (None, "xmlns", ns!(xmlns).as_ref(), "urn:default"),
            (Some("xml"), "lang", ns!(xml).as_ref(), "en"),
            (None, "lang", "", "fr"),
        ]
    );
}

#[test]
fn invalid_namespace_declarations_keep_their_xmlns_identity() {
    let dom =
        parse_document(Dom::default(), Default::default()).one("<root xmlns:xml='urn:invalid'/>");
    assert!(!dom.errors.borrow().is_empty());
    let root = dom.document.children.borrow()[0].clone();
    let Data::Element(_, attrs) = &root.data else {
        unreachable!()
    };
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].name.ns, ns!(xmlns));
    assert_eq!(&**attrs[0].name.prefix.as_ref().unwrap(), "xmlns");
    assert_eq!(&*attrs[0].name.local, "xml");
}

#[test]
fn expanded_attribute_duplicates_are_dropped_after_namespace_binding() {
    let dom = parse_document(Dom::default(), Default::default())
        .one("<root xmlns:a='urn:a' xmlns:b='urn:a' a:key='1' b:key='2'/>");
    let root = dom.document.children.borrow()[0].clone();
    let Data::Element(_, attrs) = &root.data else {
        unreachable!()
    };
    assert_eq!(
        attrs
            .iter()
            .filter(|attr| &*attr.name.local == "key")
            .count(),
        1
    );
}

#[test]
fn unexpected_eof_reports_an_unclosed_element() {
    for source in ["<root>", "<root><child/>", "<root><child>"] {
        let dom = parse_document(Dom::default(), Default::default()).one(source);
        assert!(
            dom.errors
                .borrow()
                .iter()
                .any(|error| error.contains("unclosed XML element")),
            "{source}"
        );
    }
    for source in ["<root/>", "<root><child/></root>"] {
        let dom = parse_document(Dom::default(), Default::default()).one(source);
        assert!(dom.errors.borrow().is_empty(), "{source}: {:?}", dom.errors);
    }
}
