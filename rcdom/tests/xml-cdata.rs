use std::rc::Rc;

use markup5ever::interface::tree_builder::{NodeOrText, TreeSink};
use markup5ever_rcdom::{NodeData, RcDom};
use xml5ever::tree_builder::XmlTreeSink;

#[test]
fn reference_dom_keeps_unmerged_node_identity_and_coalesces_adjacent_cdata_text() {
    let dom = RcDom::default();
    let first = dom.create_cdata("first".into());
    dom.append(&dom.document, NodeOrText::AppendNode(first.clone()));
    assert!(Rc::ptr_eq(&dom.document.children.borrow()[0], &first));

    let second = dom.create_cdata("second".into());
    dom.append(&dom.document, NodeOrText::AppendNode(second));
    assert_eq!(dom.document.children.borrow().len(), 1);
    let NodeData::Text { contents } = &first.data else {
        panic!("the reference DOM flattens CDATA to text");
    };
    assert_eq!(&**contents.borrow(), "firstsecond");

    dom.append(
        &dom.document,
        NodeOrText::AppendNode(dom.create_comment("separator".into())),
    );
    let third = dom.create_cdata("third".into());
    dom.append(&dom.document, NodeOrText::AppendNode(third.clone()));
    assert!(Rc::ptr_eq(&dom.document.children.borrow()[2], &third));
    assert!(Rc::ptr_eq(
        &third.parent.take().unwrap().upgrade().unwrap(),
        &dom.document
    ));
}
